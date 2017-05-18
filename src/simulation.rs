use std::collections::BTreeMap;

use network::Network;
use node::Node;
use node::Node::*;
use name::Name;
use block::Block;
use message::Message;
use message::MessageContent::*;
use random::{random, sample};

pub struct Simulation {
    nodes: BTreeMap<Name, Node>,
    network: Network,
    genesis: Block,
    max_steps: u64,
    active_peer_cutoff: u64,
    /// Join a node once every this many steps, e.g. join_rate = 10 means join at 0, 10, 20...
    join_rate: u64
    // TODO: track node connections out here.
}

impl Simulation {
    pub fn new(max_steps: u64, max_delay: u64, num_nodes: u64, apc: u64, join_rate: u64) -> Self {
        let mut init_nodes = BTreeMap::new();

        let first_name = random();
        let genesis = Block::genesis(first_name);
        init_nodes.insert(first_name, Node::first(first_name, genesis.clone(), apc));

        for _ in 0..(num_nodes - 1) {
            init_nodes.insert(random(), Node::joining());
        }

        Simulation {
            nodes: init_nodes,
            genesis,
            network: Network::new(max_delay),
            max_steps,
            active_peer_cutoff: apc,
            join_rate
        }
    }

    fn active_nodes<'a>(&'a self) -> Box<Iterator<Item=(&'a Name, &'a Node)> + 'a> {
        let active_nodes = self.nodes.iter()
            .filter(|&(_, node)| node.is_active());
        Box::new(active_nodes)
    }

    fn find_joining_node(&self) -> Option<Name> {
        let joining_nodes = self.nodes.iter()
            .filter(|&(_, node)| node.is_joining())
            .map(|(name, _)| *name);

        sample(joining_nodes, 1).first().cloned()
    }

    /// Choose a currently waiting node and start its join process.
    pub fn join_node(&mut self) -> Vec<Message> {
        let joining = match self.find_joining_node() {
            Some(name) => name,
            None => return vec![]
        };

        // TODO: send only to this node's section (for now, send to the whole network).
        let messages = self.active_nodes().map(|(&neighbour, _)| {
            Message {
                sender: joining,
                recipient: neighbour,
                content: NodeJoined
            }
        }).collect();

        // Make the node active, and let it build its way up from the genesis block.
        let genesis = self.genesis.clone();
        let apc = self.active_peer_cutoff;
        self.nodes.get_mut(&joining).unwrap().make_active(joining, genesis, apc);

        messages
    }

    pub fn run(&mut self) {
        for step in 0..self.max_steps {
            println!("-- step {} --", step);

            // Join an existing node if one exists, and it's been long enough since the last join.
            if step % self.join_rate == 0 {
                let join_messages = self.join_node();
                self.network.send(step, join_messages);
            }

            let delivered = self.network.receive(step);

            for message in delivered {
                match *self.nodes.get_mut(&message.recipient).unwrap() {
                    Active(ref mut node) => {
                        let new_messages = node.handle_message(message, step);
                        self.network.send(step, new_messages);
                    }
                    /*
                    Dead => {
                        println!("dropping message for dead node {}", message.recipient);
                    }
                    */
                    WaitingToJoin => panic!("invalid")
                }
            }
        }

        println!("-- final node states --");
        for node in self.nodes.values() {
            match *node {
                Active(ref node) => {
                    println!("{}: current_blocks: {:#?}", node, node.current_blocks);
                }
                _ => ()
            }
        }
    }
}
