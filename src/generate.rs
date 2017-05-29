//! Functions for generating sections of a certain size.

use block::{Block, CurrentBlocks};
use name::{Name, Prefix};
use node::Node;
use params::NodeParams;
use random::random;

use std::collections::{BTreeMap, BTreeSet};

/// Generate a bunch of nodes based on sizes specified for sections.
///
/// `sections`: map from prefix to desired size for that section.
pub fn generate_network(sections: &BTreeMap<Prefix, usize>,
                        params: &NodeParams)
                        -> (BTreeMap<Name, Node>, CurrentBlocks) {
    // Check that the supplied prefixes describe a whole network.
    assert!(Prefix::empty().is_covered_by(sections.keys()),
            "Prefixes should cover the whole namespace");

    let mut nodes_by_section = btreemap!{};

    for (prefix, &size) in sections {
        let node_names: BTreeSet<_> = (0..size)
            .map(|_| prefix.substituted_in(random()))
            .collect();
        nodes_by_section.insert(*prefix, node_names);
    }

    let current_blocks = construct_blocks(nodes_by_section.clone());

    let nodes = nodes_by_section
        .into_iter()
        .flat_map(|(_, names)| names)
        .map(|name| (name, Node::new(name, current_blocks.clone(), params.clone())))
        .collect();

    (nodes, current_blocks)
}

/// Construct a set of blocks to describe the given sections.
fn construct_blocks(nodes: BTreeMap<Prefix, BTreeSet<Name>>) -> CurrentBlocks {
    nodes
        .into_iter()
        .map(|(prefix, members)| {
                 Block {
                     prefix,
                     members,
                     version: 0,
                 }
             })
        .collect()
}
