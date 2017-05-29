use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use network::Network;
use event::Event;
use event_schedule::EventSchedule;
use node::Node;
use name::{Name, Prefix};
use block::Block;
use generate::generate_network;
use consistency::check_consistency;
use message::Message;
use message::MessageContent::*;
use params::{NodeParams, SimulationParams};
use random::{sample, do_with_probability};
use random_events::RandomEvents;
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
    /// Collection of disconnected pairs which should be trying to reconnect.
    disconnected: BTreeSet<DisconnectedPair>,
    /// Generator of random events.
    random_events: RandomEvents,
    /// Event schedule - specifying events to happen at various steps.
    event_schedule: EventSchedule,
}

impl Simulation {
    /// Create a new simulation with a single seed node.
    pub fn new(params: SimulationParams, node_params: NodeParams) -> Self {
        let single_node_genesis = btreemap! {
            Prefix::empty() => 1
        };
        Self::new_from(single_node_genesis,
                       EventSchedule::empty(),
                       params,
                       node_params)
    }

    /// Create a new simulation with sections whose prefixes and sizes are specified by `sections`.
    ///
    /// Note: the `num_nodes` parameter is entirely ignored by this constructor.
    pub fn new_from(sections: BTreeMap<Prefix, usize>,
                    event_schedule: EventSchedule,
                    params: SimulationParams,
                    node_params: NodeParams)
                    -> Self {
        let (nodes, genesis_set) = generate_network(&sections, &node_params);
        let network = Network::new(params.max_delay);
        let random_events = RandomEvents::new(params.clone());

        Simulation {
            nodes,
            genesis_set,
            network,
            params,
            node_params,
            disconnected: BTreeSet::new(),
            random_events,
            event_schedule,
        }
    }

    fn apply_add_node(&mut self, joining: Name) {
        // Make the node active, and let it build its way up from the genesis block(s).
        let genesis_set = self.genesis_set.clone();
        let params = self.node_params.clone();
        let node = Node::new(joining, genesis_set, params);
        self.nodes.insert(joining, node);
    }

    fn apply_remove_node(&mut self, leaving_node: Name) {
        println!("Node({}): dying...", leaving_node);

        // Remove the node.
        self.nodes.remove(&leaving_node);

        // Remove any "disconnections" associated with this node.
        let disconnected = mem::replace(&mut self.disconnected, BTreeSet::new());
        self.disconnected = disconnected
            .into_iter()
            .filter(|pair| pair.lower() != leaving_node && pair.higher() != leaving_node)
            .collect();
    }

    fn apply_event(&mut self, event: &Event) {
        match *event {
            Event::AddNode(name) => self.apply_add_node(name),
            Event::RemoveNode(name) => self.apply_remove_node(name),
            Event::RemoveNodeFrom(_) => panic!("normalise RemoveNodeFrom before applying"),
        }
    }

    /// Kill a connection between a pair of nodes which aren't already disconnected.
    fn disconnect_pair(&mut self) -> Vec<Message> {
        let mut pair;
        loop {
            let rnd_pair = sample(&self.nodes, 2);
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
        let messages = vec![Message {
                                sender: pair.lower(),
                                recipient: pair.higher(),
                                content: ConnectionLost,
                            },
                            Message {
                                sender: pair.higher(),
                                recipient: pair.lower(),
                                content: ConnectionLost,
                            }];

        let _ = self.disconnected.insert(pair);
        messages
    }

    /// Try to reconnect all pairs of nodes which have previously become disconnected. Each pair
    /// will only succeed with `SimulationParams::prob_reconnect` probability.
    fn reconnect_pairs(&mut self) -> Vec<Message> {
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
                messages.push(Message {
                                  sender: pair.lower(),
                                  recipient: pair.higher(),
                                  content: ConnectionRegained,
                              });
                messages.push(Message {
                                  sender: pair.higher(),
                                  recipient: pair.lower(),
                                  content: ConnectionRegained,
                              });
            } else {
                let _ = self.disconnected.insert(pair);
            }
        }
        messages
    }

    /// Generate events to occur at the given step, and send messages for them.
    pub fn generate_events(&mut self, step: u64) {
        let mut events = vec![];
        events.extend(self.event_schedule.get_events(step));
        events.extend(self.random_events.get_events(step, &self.nodes));

        let mut ev_messages = vec![];

        for ev in &mut events {
            ev.normalise(&self.nodes);
            ev_messages.extend(ev.broadcast(&self.nodes));
            self.apply_event(ev);
        }

        self.network.send(step, ev_messages);

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
    }

    /// Run the simulation, returning Ok iff the network was consistent upon termination.
    pub fn run(&mut self) -> Result<(), ()> {
        let max_extra_steps = 1000;
        // TODO: Use actual max value (probably from `NodeParams`) once the RmConv rule is added.
        let max_timeout_steps = 60;
        let mut no_op_step_count = 0;
        let mut ran_to_completion = false;

        for step in 0..(self.params.num_steps + max_extra_steps) {
            println!("-- step {} --", step);

            // Generate events.
            // We only generate events for at most `num_steps` steps, after which point
            // we wait for the network to empty out.
            if step < self.params.num_steps {
                self.generate_events(step);
            } else if self.network.queue_is_empty() {
                if no_op_step_count > max_timeout_steps {
                    ran_to_completion = true;
                    break;
                } else {
                    no_op_step_count += 1;
                }
            } else {
                no_op_step_count = 0;
            }

            let delivered = self.network.receive(step);
            for message in delivered {
                match self.nodes.get_mut(&message.recipient) {
                    Some(node) => {
                        let new_messages = node.handle_message(message, step);
                        self.network.send(step, new_messages);
                    }
                    None => {
                        println!("dropping message for dead node {}", message.recipient);
                    }
                }
            }
        }

        println!("-- final node states --");
        for node in self.nodes.values() {
            println!("{}: current_blocks: {:#?}", node, node.current_blocks);
        }

        assert!(ran_to_completion,
                "there were undelivered messages at termination");

        check_consistency(&self.nodes, self.node_params.min_section_size as usize)
    }
}
