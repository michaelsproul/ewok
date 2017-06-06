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
use random::{sample, do_with_probability, seed};
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Growth,
    Stable { since_step: u64 },
    Shrinking,
    Finishing { since_step: u64 },
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
    /// Which phase the simulation is currently in.
    phase: Phase,
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
        let random_events = RandomEvents::new(params.clone(), node_params.clone());

        Simulation {
            nodes,
            genesis_set,
            network,
            params,
            node_params,
            phase: Phase::Starting,
            disconnected: BTreeSet::new(),
            random_events,
            event_schedule,
        }
    }

    fn apply_add_node(&mut self, joining: Name, step: u64) {
        // Make the node active, and let it build its way up from the genesis block(s).
        let genesis_set = self.genesis_set.clone();
        let params = self.node_params.clone();
        let node = Node::new(joining, genesis_set, params, step);
        self.nodes.insert(joining, node);
    }

    fn apply_remove_node(&mut self, leaving_node: Name) {
        debug!("Node({}): dying...", leaving_node);

        // Remove the node.
        self.nodes.remove(&leaving_node);

        // Remove any "disconnections" associated with this node.
        let disconnected = mem::replace(&mut self.disconnected, BTreeSet::new());
        self.disconnected = disconnected
            .into_iter()
            .filter(|pair| pair.lower() != leaving_node && pair.higher() != leaving_node)
            .collect();
    }

    fn apply_event(&mut self, event: &Event, step: u64) {
        match *event {
            Event::AddNode(name) => self.apply_add_node(name, step),
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

        debug!("Node({}) and Node({}) disconnecting from each other...",
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

        self.disconnected.insert(pair);
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
               do_with_probability(self.params.prob_reconnect(self.phase)) {
                debug!("Node({}) and Node({}) reconnecting to each other...",
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
                self.disconnected.insert(pair);
            }
        }
        messages
    }

    /// Generate events to occur at the given step, and send messages for them.
    pub fn generate_events(&mut self, step: u64) {
        let mut events = vec![];
        events.extend(self.event_schedule.get_events(step));
        events.extend(self.random_events.get_events(self.phase, &self.nodes));
        trace!("events: {:?}", events);

        let mut ev_messages = vec![];

        for ev in events {
            if let Some(ev) = ev.normalise(&self.nodes) {
                ev_messages.extend(ev.broadcast(&self.nodes));
                self.apply_event(&ev, step);
            }
        }

        self.network.send(step, ev_messages);

        // Kill a connection between two nodes if we're past the stabilisation threshold.
        if do_with_probability(self.params.prob_disconnect(self.phase)) {
            let disconnect_messages = self.disconnect_pair();
            self.network.send(step, disconnect_messages);
        }

        // Try to reconnect any previously-disconnected pairs.
        let reconnect_messages = self.reconnect_pairs();
        self.network.send(step, reconnect_messages);
    }

    /// Run the simulation, returning Ok iff the network was consistent upon termination.
    pub fn run(&mut self) -> Result<(), [u32; 4]> {
        let max_extra_steps = 1000;
        let mut no_op_step_count = 0;

        for step in 0.. {
            // Generate events unless we're in the finishing phase, in which case we let the event
            // queue empty out.
            if let Phase::Finishing { since_step } = self.phase {
                if step > since_step + max_extra_steps {
                    break;
                }
                if self.network.queue_is_empty() {
                    if no_op_step_count > self.node_params.join_timeout {
                        break;
                    } else {
                        no_op_step_count += 1;
                    }
                } else {
                    no_op_step_count = 0;
                }
                info!("-- step {} ({:?}) {} nodes --",
                      step,
                      self.phase,
                      self.nodes.len());
            } else {
                info!("-- step {} ({:?}) {} nodes --",
                      step,
                      self.phase,
                      self.nodes.len());
                self.generate_events(step);
            }

            let delivered = self.network.receive(step);
            for message in delivered {
                match self.nodes.get_mut(&message.recipient) {
                    Some(node) => {
                        let new_messages = node.handle_message(message, step);
                        self.network.send(step, new_messages);
                    }
                    None => {
                        debug!("dropping message for dead node {}", message.recipient);
                    }
                }
            }

            // Shutdown nodes that have failed to join.
            let mut to_shutdown = BTreeSet::new();
            for (name, node) in &self.nodes {
                if node.should_shutdown(step) {
                    to_shutdown.insert(*name);
                }
            }

            for name in to_shutdown {
                self.apply_remove_node(name);
                let removal_msgs = Event::RemoveNode(name).broadcast(&self.nodes);
                self.network.send(step, removal_msgs);
            }

            // Send pending votes independently of churn events
            for node in self.nodes.values_mut() {
                self.network.send(step, node.broadcast_new_votes(step));
                match node.our_current_blocks().count() {
                    0 => {
                        if step > node.step_created() + self.node_params.self_shutdown_timeout {
                            // The node should have joined by now and received the votes showing it
                            // existing in at least one current block.
                            panic!("{:?}\ndoesn't have any current blocks", node)
                        }
                    }
                    1 => node.check_conflicting_block_count(),
                    count => panic!("{:?}\nhas {} current blocks for own section.", node, count),
                }
            }

            self.phase = self.phase_for_next_step(step);
        }

        debug!("-- final node states --");
        for node in self.nodes.values() {
            debug!("{:?}", node);
        }

        assert!(no_op_step_count > self.node_params.join_timeout,
                "Votes were still being sent and received after {} extra steps during which no \
                 churn was triggered.",
                max_extra_steps);

        check_consistency(&self.nodes, self.node_params.min_section_size as usize)
            .map_err(|_| seed())
    }

    fn phase_for_next_step(&self, step: u64) -> Phase {
        use self::Phase::*;

        match self.phase {
            Starting => {
                if self.nodes.len() >= self.params.starting_complete {
                    if self.params.grow_prob_join > 0.0 {
                        Growth
                    } else {
                        Stable { since_step: step + 1 }
                    }
                } else {
                    Starting
                }
            }
            Growth => {
                if self.nodes.len() >= self.params.grow_complete {
                    Stable { since_step: step + 1 }
                } else {
                    Growth
                }
            }
            Stable { since_step } => {
                if step >= since_step + self.params.stable_steps {
                    if self.params.shrink_prob_drop > 0.0 {
                        Shrinking
                    } else {
                        Finishing { since_step: step + 1 }
                    }
                } else {
                    Stable { since_step }
                }
            }
            Shrinking => {
                // If the next node removal would take us below quorum, move to `Finishing`.
                if (self.nodes.len() - 1) * 2 <= self.node_params.min_section_size {
                    Finishing { since_step: step + 1 }
                } else {
                    Shrinking
                }
            }
            Finishing { since_step } => Finishing { since_step },
        }
    }
}
