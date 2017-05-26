use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use network::Network;
use node::Node;
use node::Node::*;
use name::{Name, Prefix};
use block::Block;
use generate::generate_network;
use consistency::check_consistency;
use message::Event;
use message::Content;
use message::Notification::*;
use params::{NodeParams, SimulationParams};
use random::{random, sample, sample_single, do_with_probability};
use self::detail::DisconnectedPair;

mod detail {
    use name::Name;

    /// Holds a pair of names sorted by lowest first.
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct DisconnectedPair {
        lower: Name,
        higher: Name,
    }

    impl DisconnectedPair {
        pub fn new(x: Name, y: Name) -> DisconnectedPair {
            if x < y {
                DisconnectedPair {
                    lower: x,
                    higher: y,
                }
            } else if x > y {
                DisconnectedPair {
                    lower: y,
                    higher: x,
                }
            } else {
                panic!("Node({}) can't disconnect from itself.", x);
            }
        }

        pub fn lower(&self) -> Name {
            self.lower
        }

        pub fn higher(&self) -> Name {
            self.higher
        }
    }
}

pub struct Simulation {
    nodes: BTreeMap<Name, Node>,
    network: Network,
    /// Set of blocks that all nodes start from (often just a single genesis block).
    genesis_set: BTreeSet<Block>,
    /// Parameters for the network and the simulation.
    params: SimulationParams,
    /// Parameters for nodes.
    node_params: NodeParams,
    /// Set of connections between nodes. Connections have a direction (from, to). Due to random
    /// disconnects and reconnects, an entry here does not necessarily mean that both peers consider
    /// themselves to be connected; they may have not yet handled the `ConnectionRegained`
    /// pseudo-message. Similarly, their entries may have been removed from here, but they may not
    /// have handled the corresponding `ConnectionLost`s.
    connections: BTreeSet<(Name, Name)>,
    /// Collection of disconnected pairs which should be trying to reconnect. These do not have a
    /// direction, so in the case of a successful reconnect, one entry here will be removed and will
    /// result in two entries being added to `connections`.
    disconnected: BTreeSet<DisconnectedPair>,
}

impl Simulation {
    /// Create a new simulation with a single seed node.
    pub fn new(params: SimulationParams, node_params: NodeParams) -> Self {
        let mut nodes = BTreeMap::new();

        let first_name = random();
        let genesis_set = btreeset!{ Block::genesis(first_name) };
        let first_node = Node::active(first_name, genesis_set.clone(), node_params.clone());
        nodes.insert(first_name, first_node);

        for _ in 0..(params.num_nodes - 1) {
            nodes.insert(random(), Node::joining());
        }

        let connections = Self::complete_connections(nodes.keys().cloned().collect());
        let network = Network::new(params.max_delay);

        Simulation {
            nodes,
            genesis_set,
            network,
            params,
            node_params,
            connections,
            disconnected: BTreeSet::new(),
        }
    }

    /// Create a new simulation with sections whose prefixes and sizes are specified by `sections`.
    ///
    /// Note: the `num_nodes` parameter is entirely ignored by this constructor.
    pub fn new_from(sections: BTreeMap<Prefix, usize>,
                    params: SimulationParams,
                    node_params: NodeParams)
                    -> Self {
        let (nodes, genesis_set) = generate_network(&sections, node_params.clone());

        let connections = Self::complete_connections(nodes.keys().cloned().collect());
        let network = Network::new(params.max_delay);

        Simulation {
            nodes,
            genesis_set,
            network,
            params,
            node_params,
            connections,
            disconnected: BTreeSet::new(),
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
    fn join_node(&mut self) -> Vec<Event> {
        let joining = match self.find_joining_node() {
            Some(name) => name,
            None => return vec![],
        };

        // TODO: send only to this node's section (for now, send to the whole network).
        let messages = self.active_nodes()
            .map(|(&neighbour, _)| {
                     Event {
                         src: joining,
                         dst: neighbour,
                         content: Content::Notification(NodeJoined),
                     }
                 })
            .collect();

        // Make the node active, and let it build its way up from the genesis block(s).
        let genesis_set = self.genesis_set.clone();
        let params = self.node_params.clone();
        self.nodes
            .get_mut(&joining)
            .unwrap()
            .make_active(joining, genesis_set, params);

        messages
    }

    /// Drop an existing node if one exists to drop.
    fn drop_node(&mut self) -> Vec<Event> {
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
                     Event {
                         src: leaving_node,
                         dst: neighbour,
                         content: Content::Notification(ConnectionLost),
                     }
                 })
            .collect();

        // Block the connections to and from this node.
        self.block_all_connections(leaving_node);
        let disconnected = mem::replace(&mut self.disconnected, BTreeSet::new());
        self.disconnected = disconnected
            .into_iter()
            .filter(|pair| pair.lower() != leaving_node && pair.higher() != leaving_node)
            .collect();

        messages
    }

    /// Kill a connection between a pair of nodes which aren't already disconnected.
    fn disconnect_pair(&mut self) -> Vec<Event> {
        let mut pair;
        loop {
            let rnd_pair = sample(self.active_nodes(), 2);
            if rnd_pair.len() != 2 {
                return vec![];
            }
            pair = DisconnectedPair::new(*rnd_pair[0].0, *rnd_pair[1].0);
            if !self.nodes[&pair.lower()].is_disconnected_from(&pair.higher()) &&
               !self.nodes[&pair.higher()].is_disconnected_from(&pair.lower()) {
                break;
            }
        }

        println!("Node({}) and Node({}) disconnecting from each other...",
                 pair.lower(),
                 pair.higher());
        let messages = vec![Event {
                                src: pair.lower(),
                                dst: pair.higher(),
                                content: Content::Notification(ConnectionLost),
                            },
                            Event {
                                src: pair.higher(),
                                dst: pair.lower(),
                                content: Content::Notification(ConnectionLost),
                            }];

        let _ = self.connections.remove(&(pair.lower(), pair.higher()));
        let _ = self.connections.remove(&(pair.higher(), pair.lower()));
        let _ = self.disconnected.insert(pair);
        messages
    }

    /// Try to reconnect all pairs of nodes which have previously become disconnected. Each pair
    /// will only succeed with `SimulationParams::prob_reconnect` probability.
    fn reconnect_pairs(&mut self) -> Vec<Event> {
        let disconnected = mem::replace(&mut self.disconnected, BTreeSet::new());
        let mut messages = vec![];
        for pair in disconnected {
            // Ensure both have realised they're disconnected.
            if self.nodes[&pair.lower()].is_disconnected_from(&pair.higher()) &&
               self.nodes[&pair.higher()].is_disconnected_from(&pair.lower()) &&
               do_with_probability(self.params.prob_reconnect) {
                println!("Node({}) and Node({}) reconnecting to each other...",
                         pair.lower(),
                         pair.higher());
                let _ = self.connections.insert((pair.lower(), pair.higher()));
                let _ = self.connections.insert((pair.higher(), pair.lower()));
                messages.push(Event {
                                  src: pair.lower(),
                                  dst: pair.higher(),
                                  content: Content::Notification(ConnectionRegained),
                              });
                messages.push(Event {
                                  src: pair.higher(),
                                  dst: pair.lower(),
                                  content: Content::Notification(ConnectionRegained),
                              });
            } else {
                let _ = self.disconnected.insert(pair);
            }
        }
        messages
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

    fn event_allowed(&self, event: &Event) -> bool {
        match event.content {
            Content::Message(..) => {
                !self.nodes[&event.src].is_disconnected_from(&event.dst) &&
                !self.nodes[&event.dst].is_disconnected_from(&event.src)
            }
            Content::Notification(NodeJoined) | Content::Notification(ConnectionRegained) => {
                self.connections
                    .contains(&(event.src, event.dst))
            }
            Content::Notification(ConnectionLost) => true,
        }
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

            // Try to reconnect any previously-disconnected pairs if we're past the stabilisation
            // threshold. Exclude any which were disconnected in this step.
            if step >= self.params.drop_step {
                let reconnect_messages = self.reconnect_pairs();
                self.network.send(step, reconnect_messages);
            }

            let delivered = self.network.receive(step);

            for event in delivered {
                if !self.event_allowed(&event) {
                    println!("dropping this message: {:?}", event);
                    continue;
                }

                match *self.nodes.get_mut(&event.dst).unwrap() {
                    Active(ref mut node) => {
                        let new_messages = node.handle_event(event, step);
                        self.network.send(step, new_messages);
                    }
                    Dead => {
                        println!("dropping message for dead node {}", event.dst);
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

        check_consistency(&self.nodes, self.node_params.min_section_size as usize)
    }
}
