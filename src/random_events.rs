use std::collections::{BTreeMap, BTreeSet};
use std::cmp;
use itertools::Itertools;
use params::{SimulationParams, NodeParams, quorum};
use blocks::Blocks;
use name::Name;
use node::Node;
use event::Event;
use random::{random, do_with_probability, shuffle};
use simulation::Phase;

pub struct RandomEvents {
    params: SimulationParams,
    node_params: NodeParams,
}

impl RandomEvents {
    pub fn new(params: SimulationParams, node_params: NodeParams) -> Self {
        RandomEvents {
            params,
            node_params,
        }
    }

    pub fn get_events(
        &self,
        phase: Phase,
        blocks: &Blocks,
        nodes: &BTreeMap<Name, Node>,
    ) -> Vec<Event> {
        let mut events = vec![];

        // Random join.
        if do_with_probability(self.params.prob_join(phase)) {
            events.push(self.random_add());
        }

        // Random remove.
        if do_with_probability(self.params.prob_drop(phase)) {
            if let Some(event) = self.random_remove(blocks, nodes) {
                events.push(event);
            }
        }

        events
    }

    fn random_add(&self) -> Event {
        Event::AddNode(random())
    }

    fn random_remove(&self, blocks: &Blocks, nodes: &BTreeMap<Name, Node>) -> Option<Event> {
        self.find_node_to_remove(blocks, nodes).map(
            Event::RemoveNode,
        )
    }

    // Remove a randomly-selected node which is in a section with at least quorum + 2 members. The
    // section's member count is calculated by removing any dead nodes from the node's own current
    // block's member list. If no suitable node can be found, the function returns `None`.
    fn find_node_to_remove(&self, blocks: &Blocks, nodes: &BTreeMap<Name, Node>) -> Option<Name> {
        let names_sorted: BTreeSet<_> = nodes.keys().cloned().collect();
        let mut names = nodes.keys().cloned().collect_vec();
        shuffle(&mut names);
        for name in names {
            if let Some(our_current_block) = nodes[&name].our_current_blocks(blocks).first() {
                let num_live = our_current_block
                    .members
                    .intersection(&names_sorted)
                    .count();
                // Don't sink below a quorum of our current block, OR the min section size.
                let min_nodes = quorum(cmp::max(
                    our_current_block.members.len(),
                    self.node_params.min_section_size,
                ));
                if num_live >= min_nodes + 2 {
                    trace!(
                        "Node({}): removed from section with {} live nodes",
                        name,
                        num_live
                    );
                    return Some(name);
                }
            }
        }
        warn!("All sections are at 'quorum' - can't find a node to remove.");
        None
    }
}
