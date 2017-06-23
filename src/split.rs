use name::Name;
use block::{Block, Vote, CurrentBlocks, our_blocks};
use std::collections::BTreeSet;
use std::rc::Rc;

pub fn split_blocks(
    current_blocks: &CurrentBlocks,
    our_name: Name,
    min_split_size: usize,
) -> Vec<Vote> {
    our_blocks(current_blocks, our_name)
        .flat_map(|block| split_block(block, current_blocks, min_split_size))
        .collect()
}

/// If a section as described by `block` can split, return the two blocks it splits into.
/// rule:Split
fn split_block(
    block: &Rc<Block>,
    current_blocks: &CurrentBlocks,
    min_split_size: usize,
) -> Vec<Vote> {
    let p0 = block.prefix.pushed(false);
    let p1 = block.prefix.pushed(true);
    let (s0, s1): (BTreeSet<_>, _) = block.members.iter().partition(|name| p0.matches(**name));

    if s0.len() >= min_split_size && s1.len() >= min_split_size &&
        neighbours_ok(block, current_blocks, min_split_size)
    {
        let b0 = Block {
            prefix: p0,
            version: block.version + 1,
            members: s0,
        };
        let b1 = Block {
            prefix: p1,
            version: block.version + 1,
            members: s1,
        };

        let v0 = Vote {
            from: block.clone(),
            to: Rc::new(b0),
        };
        let v1 = Vote {
            from: block.clone(),
            to: Rc::new(b1),
        };

        vec![v0, v1]
    } else {
        vec![]
    }
}

/// True if all neighbouring and compatible blocks of `block` are of `min_split_size`.
fn neighbours_ok(block: &Rc<Block>, current_blocks: &CurrentBlocks, min_split_size: usize) -> bool {
    current_blocks
        .iter()
        .filter(move |other_block| {
            other_block.prefix.is_sibling_of_ancestor_of(&block.prefix)
        })
        .all(|other_block| other_block.members.len() >= min_split_size)
}
