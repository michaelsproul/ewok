use std::collections::HashMap;

use network::Network;
use node::Node;
use node::Node::*;
use name::Name;
use message::Message;
use message::MessageContent::*;
use random::{random, sample};

pub struct Simulation {
    nodes: HashMap<Name, Node>,
    network: Network,
    max_steps: u64
    // TODO: track node connections out here.
}

impl Simulation {
    pub fn new(max_steps: u64, max_delay: u64, num_nodes: u64) -> Self {
        let mut init_nodes = HashMap::new();
        // apc := active peer cutoff.
        let apc = max_delay;

        let first_name = random();
        init_nodes.insert(first_name, Node::first(first_name, apc));

        for _ in 0..(num_nodes - 1) {
            init_nodes.insert(random::<u64>(), Node::joining());
        }

        Simulation {
            nodes: init_nodes,
            network: Network::new(max_delay),
            max_steps
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

        // TODO: pick a node, send to its idea of this node's section.
        // for now, send to all.
        let messages = self.active_nodes().map(|(&neighbour, _)| {
            Message {
                sender: joining,
                recipient: neighbour,
                content: NodeJoined
            }
        }).collect();

        // Steal blocks off the first valid node.
        // FIXME
        let (valid, current, votes, apc) = match self.active_nodes().next().unwrap() {
            (_, &Active(ref node)) => {
                (node.current_blocks.clone(),
                 node.valid_blocks.clone(),
                 node.vote_counts.clone(),
                 node.active_peer_cutoff)
            }
            _ => panic!()
        };

        self.nodes.get_mut(&joining).unwrap().make_active(joining, valid, current, votes, apc);

        messages
    }

    pub fn run(&mut self) {
        for step in 0..self.max_steps {
            println!("-- step {} --", step);

            // Join an existing node if one exists.
            let join_messages = self.join_node();
            self.network.send(step, join_messages);

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


