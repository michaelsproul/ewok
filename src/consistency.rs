use name::{Name, Prefix};
use node::Node;
use blocks::Blocks;
use block::Block;
use std::collections::{BTreeMap, BTreeSet};
use itertools::Itertools;

/// Check that all the nodes have a consistent view of the network.
pub fn check_consistency(
    blocks: &Blocks,
    nodes: &BTreeMap<Name, Node>,
    min_section_size: usize,
) -> Result<BTreeMap<Prefix, Block>, ()> {
    let mut sections = btreemap!{};
    let mut result = btreemap!{};
    let mut failed = false;

    for node in nodes.values() {
        for block in blocks.block_contents(&node.current_blocks) {
            let section_versions = sections.entry(block.prefix).or_insert_with(BTreeSet::new);
            section_versions.insert(block.clone());
        }
    }

    let num_sections = sections.len();

    for (prefix, blocks) in sections {
        if blocks.len() > 1 {
            failed = true;
            error!(
                "multiple versions of {:?}, they are: {:#?}",
                prefix,
                blocks
            );
            continue;
        }

        let block = blocks.into_iter().next().unwrap();

        // Allow a quorum if we have only one section, otherwise require `min_section_size`.
        if (num_sections == 1 && block.members.len() * 2 <= min_section_size) ||
            (num_sections > 1 && block.members.len() < min_section_size)
        {
            failed = true;
            error!("section too small: {:?} with members {:?}", prefix, block.members);
        }

        // Check that all members are alive.
        for member in &block.members {
            if !nodes.contains_key(member) {
                failed = true;
                error!(
                    "node {:?} is dead but appears in the block for {:?}",
                    member,
                    prefix
                );
            }
        }

        result.insert(prefix, block);
    }

    for (p1, p2) in result.keys().tuple_combinations() {
        if p1.is_compatible(p2) {
            failed = true;
            error!("prefixes {:?} and {:?} overlap", p1, p2);
        }
    }

    if failed {
        error!("network not consistent: see above");
        Err(())
    } else {
        info!("network is consistent!");
        Ok(result)
    }
}
