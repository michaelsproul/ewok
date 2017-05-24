use name::Name;
use block::{Block, Vote, CurrentBlocks, our_blocks, blocks_for_prefix};
use std::collections::{BTreeSet, HashSet};

pub fn merge_blocks(current_blocks: &CurrentBlocks,
                    our_name: Name,
                    min_section_size: usize)
                    -> Vec<Vote> {
    let candidates = find_candidates(current_blocks, our_name, min_section_size);
    let mut votes = BTreeSet::new();
    for candidate in candidates {
        if candidate.members.contains(&our_name) {
            let sibling_prefix = candidate.prefix.sibling().unwrap();
            for block in blocks_for_prefix(current_blocks, sibling_prefix) {
                let target = merged_block(candidate, block);
                let vote = Vote {
                    from: candidate.clone(),
                    to: target,
                };
                votes.insert(vote);
            }
        } else {
            let sibling_prefix = candidate.prefix.sibling().unwrap();
            for block in our_blocks(current_blocks, our_name).filter(move |b| {
                                                                         b.prefix == sibling_prefix
                                                                     }) {
                let target = merged_block(candidate, block);
                let vote = Vote {
                    from: block.clone(),
                    to: target,
                };
                votes.insert(vote);
            }
        }
    }
    votes.into_iter().collect()
}

fn find_candidates(current_blocks: &CurrentBlocks,
                   our_name: Name,
                   min_section_size: usize)
                   -> BTreeSet<&Block> {
    let mut small_blocks = BTreeSet::<&Block>::new();
    let mut processed_siblings = HashSet::new();
    let ours = our_blocks(current_blocks, our_name).filter(|&b| b.prefix.bit_count() > 0);
    for block in ours {
        if block.members.len() < min_section_size {
            small_blocks.insert(block);
        }
        let sibling_prefix = block.prefix.sibling().unwrap(); // safe because block.prefix.bit_count > 0
        if !processed_siblings.contains(&sibling_prefix) {
            processed_siblings.insert(sibling_prefix);
            for sibling in blocks_for_prefix(current_blocks, sibling_prefix) {
                if sibling.members.len() < min_section_size {
                    small_blocks.insert(sibling);
                }
            }
        }
    }
    small_blocks
}

fn merged_block(b0: &Block, b1: &Block) -> Block {
    assert!(b0.prefix.sibling() == Some(b1.prefix));
    Block {
        prefix: b0.prefix.popped(),
        version: ::std::cmp::max(b0.version, b1.version) + 1,
        members: b0.members.union(&b1.members).cloned().collect(),
    }
}
