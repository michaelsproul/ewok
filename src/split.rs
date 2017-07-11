use name::Name;
use block::{Block, Vote};
use blocks::{Blocks, CurrentBlocks};
use std::collections::BTreeSet;

pub fn split_blocks(
    blocks: &mut Blocks,
    current_blocks: &CurrentBlocks,
    our_name: Name,
    min_split_size: usize,
) -> Vec<Vote> {
    // TODO: find a way to satisfy the borrow checker without cloning
    let our_blocks = blocks
        .our_blocks(current_blocks, our_name)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    our_blocks
        .into_iter()
        .flat_map(|block| {
            split_block(blocks, &block, current_blocks, min_split_size)
        })
        .collect()
}

/// If a section as described by `block` can split, return the two blocks it splits into.
/// rule:Split
fn split_block(
    blocks: &mut Blocks,
    block: &Block,
    current_blocks: &CurrentBlocks,
    min_split_size: usize,
) -> Vec<Vote> {
    if block.should_split(min_split_size) &&
        neighbours_ok(blocks, block, current_blocks, min_split_size)
    {
        let p0 = block.prefix.pushed(false);
        let p1 = block.prefix.pushed(true);
        let (s0, s1): (BTreeSet<_>, _) = block.members.iter().partition(|name| p0.matches(**name));
        let b0 = blocks.insert(Block {
            prefix: p0,
            version: block.version + 1,
            members: s0,
        });
        let b1 = blocks.insert(Block {
            prefix: p1,
            version: block.version + 1,
            members: s1,
        });

        let v0 = Vote {
            from: block.get_id(),
            to: b0,
        };
        let v1 = Vote {
            from: block.get_id(),
            to: b1,
        };

        vec![v0, v1]
    } else {
        vec![]
    }
}

/// True if all neighbouring and compatible blocks of `block` are of `min_split_size`.
fn neighbours_ok(
    blocks: &Blocks,
    block: &Block,
    current_blocks: &CurrentBlocks,
    min_split_size: usize,
) -> bool {
    current_blocks
        .iter()
        .map(|b| blocks.get(b).unwrap())
        .filter(move |other_block| {
            other_block.prefix.is_sibling_of_ancestor_of(&block.prefix)
        })
        .all(|other_block| other_block.members.len() >= min_split_size)
}
