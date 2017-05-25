use random::sample_single;
use std::collections::BTreeMap;
use params::SimulationParams;
use name::Name;
use node::Node;
use event::Event;
use random::do_with_probability;

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
            events.extend(random_add(nodes));
        }

        // Random remove.
        if step >= self.params.drop_step && do_with_probability(self.params.prob_drop) {
            events.extend(random_remove(nodes));
        }

        events
    }
}

fn random_add(nodes: &BTreeMap<Name, Node>) -> Vec<Event> {
    find_joining_node(nodes)
        .map(Event::AddNode)
        .into_iter()
        .collect()
}

fn find_joining_node(nodes: &BTreeMap<Name, Node>) -> Option<Name> {
    let joining_nodes = nodes
        .iter()
        .filter(|&(_, node)| node.is_joining())
        .map(|(name, _)| *name);

    sample_single(joining_nodes)
}

fn random_remove(nodes: &BTreeMap<Name, Node>) -> Vec<Event> {
    find_node_to_remove(nodes)
        .map(Event::RemoveNode)
        .into_iter()
        .collect()
}

fn find_node_to_remove(nodes: &BTreeMap<Name, Node>) -> Option<Name> {
    let active_nodes = nodes
        .iter()
        .filter(|&(_, node)| node.is_active())
        .map(|(name, _)| *name);

    sample_single(active_nodes)
}
