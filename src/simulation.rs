use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use network::Network;
use node::Node;
use node::Node::*;
use name::Name;
use block::Block;
use consistency::check_consistency;
use message::Message;
use message::MessageContent::*;
use params::{NodeParams, SimulationParams};
use random::{random, sample, sample_single, do_with_probability};

pub struct Simulation {
    nodes: BTreeMap<Name, Node>,
    network: Network,
    genesis: Block,
    /// Parameters for the network and the simulation.
    params: SimulationParams,
    /// Parameters for nodes.
    node_params: NodeParams,
    /// Set of connections between nodes. Connections have a direction (from, to).
    connections: BTreeSet<(Name, Name)>,
}

impl Simulation {
    pub fn new(params: SimulationParams, node_params: NodeParams) -> Self {
        let mut nodes = BTreeMap::new();

        let first_name = random();
        let genesis = Block::genesis(first_name);
        let first_node = Node::first(first_name, genesis.clone(), node_params.clone());
        nodes.insert(first_name, first_node);

        for _ in 0..(params.num_nodes - 1) {
            nodes.insert(random(), Node::joining());
        }

        let connections = Self::complete_connections(nodes.keys().cloned().collect());
        let network = Network::new(params.max_delay);

        Simulation {
            nodes,
            genesis,
            network,
            params,
            node_params,
            connections,
        }
    }

    fn active_nodes<'a>(&'a self) -> Box<Iterator<Item = (&'a Name, &'a Node)> + 'a> {
        let active_nodes = self.nodes.iter().filter(|&(_, node)| node.is_active());
        Box::new(active_nodes)
    }

    fn find_joining_node(&self) -> Option<Name> {
        let joining_nodes = self.nodes
            .iter()
            .filter(|&(_, node)| node.is_joining())
            .map(|(name, _)| *name);

        sample_single(joining_nodes)
    }

    /// Choose a currently waiting node and start its join process.
    fn join_node(&mut self) -> Vec<Message> {
        let joining = match self.find_joining_node() {
            Some(name) => name,
            None => return vec![],
        };

        // TODO: send only to this node's section (for now, send to the whole network).
        let messages = self.active_nodes()
            .map(|(&neighbour, _)| {
                     Message {
                         sender: joining,
                         recipient: neighbour,
                         content: NodeJoined,
                     }
                 })
            .collect();

        // Make the node active, and let it build its way up from the genesis block.
        let genesis = self.genesis.clone();
        let params = self.node_params.clone();
        self.nodes
            .get_mut(&joining)
            .unwrap()
            .make_active(joining, genesis, params);

        messages
    }

    /// Drop an existing node if one exists to drop.
    fn drop_node(&mut self) -> Vec<Message> {
        let leaving_node = match sample_single(self.active_nodes()) {
            Some((name, _)) => *name,
            None => return vec![],
        };

        println!("Node({}): dying...", leaving_node);

        // Mark the node dead.
        self.nodes.get_mut(&leaving_node).unwrap().kill();

        // Send disconnect messages, ensuring we don't send a disconnect if the connection
        // already dropped.
        let messages = self.active_nodes()
            .filter(|&(&neighbour, _)| self.connections.contains(&(neighbour, leaving_node)))
            .map(|(&neighbour, _)| {
                     Message {
                         sender: leaving_node,
                         recipient: neighbour,
                         content: ConnectionLost,
                     }
                 })
            .collect();

        // Block the connections to and from this node.
        self.block_all_connections(leaving_node);

        messages
    }

    /// Kill a connection between a pair of nodes.
    fn disconnect_pair(&mut self) -> Vec<Message> {
        let (name0, name1) = {
            let pair = sample(self.active_nodes(), 2);
            if pair.len() != 2 {
                return vec![];
            }
            (*pair[0].0, *pair[1].0)
        };

        println!("Node({}) and Node({}) disconnecting from each other...",
                 name0,
                 name1);

        let _ = self.connections.remove(&(name0, name1));
        let _ = self.connections.remove(&(name1, name0));

        vec![Message {
                 sender: name0,
                 recipient: name1,
                 content: ConnectionLost,
             },
             Message {
                 sender: name1,
                 recipient: name0,
                 content: ConnectionLost,
             }]
    }

    fn complete_connections(names: Vec<Name>) -> BTreeSet<(Name, Name)> {
        let mut connections = BTreeSet::new();
        for n1 in &names {
            for n2 in &names {
                if n1 != n2 {
                    connections.insert((*n1, *n2));
                }
            }
        }
        connections
    }

    fn block_all_connections(&mut self, name: Name) {
        let connections = mem::replace(&mut self.connections, BTreeSet::new());

        for (sender, recipient) in connections {
            if sender != name && recipient != name {
                self.connections.insert((sender, recipient));
            }
        }
    }

    fn message_allowed(&self, message: &Message) -> bool {
        message.content == ConnectionLost ||
        self.connections
            .contains(&(message.sender, message.recipient))
    }

    /// Run the simulation, returning Ok iff the network was consistent upon termination.
    pub fn run(&mut self) -> Result<(), ()> {
        for step in 0..self.params.num_steps {
            println!("-- step {} --", step);

            // Join an existing node if one exists, and it's been long enough since the last join.
            if do_with_probability(self.params.prob_join) {
                let join_messages = self.join_node();
                self.network.send(step, join_messages);
            }

            // Remove an existing node if one exists, and we're past the stabilisation threshold.
            if step >= self.params.drop_step && do_with_probability(self.params.prob_drop) {
                let remove_messages = self.drop_node();
                self.network.send(step, remove_messages);
            }

            // Kill a connection between two nodes if we're past the stabilisation threshold.
            if step >= self.params.drop_step && do_with_probability(self.params.prob_disconnect) {
                let disconnect_messages = self.disconnect_pair();
                self.network.send(step, disconnect_messages);
            }

            let delivered = self.network.receive(step);

            for message in delivered {
                if !self.message_allowed(&message) {
                    println!("dropping this message: {:?}", message);
                    continue;
                }

                match *self.nodes.get_mut(&message.recipient).unwrap() {
                    Active(ref mut node) => {
                        let new_messages = node.handle_message(message, step);
                        self.network.send(step, new_messages);
                    }
                    Dead => {
                        println!("dropping message for dead node {}", message.recipient);
                    }
                    WaitingToJoin => panic!("invalid"),
                }
            }
        }

        println!("-- final node states --");
        for node in self.nodes.values() {
            if let Active(ref node) = *node {
                println!("{}: current_blocks: {:#?}", node, node.current_blocks);
            }
        }

        check_consistency(&self.nodes)
    }
}
