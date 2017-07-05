//! Functions for generating sections of a certain size.

use block::{Block, BlockId};
use blocks::{Blocks, CurrentBlocks};
use name::{Name, Prefix};
use node::Node;
use params::NodeParams;
use random::random;

use std::collections::{BTreeMap, BTreeSet};

/// Generate a bunch of nodes based on sizes specified for sections.
///
/// `sections`: map from prefix to desired size for that section.
pub fn generate_network(
    blocks: &mut Blocks,
    sections: &BTreeMap<Prefix, usize>,
    params: &NodeParams,
) -> (BTreeMap<Name, Node>, BTreeSet<BlockId>) {
    // Check that the supplied prefixes describe a whole network.
    assert!(
        Prefix::empty().is_covered_by(sections.keys()),
        "Prefixes should cover the whole namespace"
    );

    let mut nodes_by_section = btreemap!{};

    for (prefix, &size) in sections {
        let node_names: BTreeSet<_> = (0..size).map(|_| prefix.substituted_in(random())).collect();
        nodes_by_section.insert(*prefix, node_names);
    }

    let current_blocks: CurrentBlocks = construct_blocks(nodes_by_section.clone())
        .into_iter()
        .map(|b| blocks.insert(b))
        .collect();

    let nodes = nodes_by_section
        .into_iter()
        .flat_map(|(_, names)| names)
        .map(|name| {
            (
                name,
                Node::new(name, blocks, current_blocks.clone(), params.clone(), 0),
            )
        })
        .collect();

    (nodes, current_blocks)
}

/// Construct a set of blocks to describe the given sections.
fn construct_blocks(nodes: BTreeMap<Prefix, BTreeSet<Name>>) -> BTreeSet<Block> {
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
