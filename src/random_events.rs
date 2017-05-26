use random::sample_single;
use std::collections::BTreeMap;
use params::SimulationParams;
use name::Name;
use node::Node;
use event::Event;
use random::{random, do_with_probability};

pub struct RandomEvents {
    params: SimulationParams
}

impl RandomEvents {
    pub fn new(params: SimulationParams) -> Self {
        RandomEvents { params }
    }

    pub fn get_events(&self, step: u64, nodes: &BTreeMap<Name, Node>) -> Vec<Event> {
        let mut events = vec![];

        // Random join.
        if do_with_probability(self.params.prob_join) {
            events.extend(self.random_add(nodes));
        }

        // Random remove.
        if step >= self.params.drop_step && do_with_probability(self.params.prob_drop) {
            events.extend(self.random_remove(nodes));
        }

        events
    }

    fn random_add(&self, nodes: &BTreeMap<Name, Node>) -> Vec<Event> {
        if nodes.len() < self.params.max_num_nodes {
            vec![Event::AddNode(random())]
        } else {
            vec![]
        }
    }

    fn random_remove(&self, nodes: &BTreeMap<Name, Node>) -> Vec<Event> {
        Self::find_node_to_remove(nodes)
            .map(Event::RemoveNode)
            .into_iter()
            .collect()
    }

    fn find_node_to_remove(nodes: &BTreeMap<Name, Node>) -> Option<Name> {
        sample_single(nodes.iter()).map(|(name, _)| *name)
    }
}


