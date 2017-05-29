use name::{Name, Prefix};
use block::{Block, Vote, CurrentBlocks, our_blocks, blocks_for_prefix};
use std::collections::BTreeSet;

pub fn merge_blocks(current_blocks: &CurrentBlocks,
                    our_name: Name,
                    min_section_size: usize)
                    -> Vec<Vote> {
    // find blocks that describe sections below threshold
    let candidates = find_small_blocks(current_blocks, min_section_size);
    let mut votes = BTreeSet::new();
    for candidate in candidates {
        if candidate.members.contains(&our_name) {
            // if the block contains our name, vote for merging with all current siblings
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
            // The block doesn't contain our name - it might be our sibling or a sibling of our
            // ancestor (for each our block it might be different).
            // For each our block, we will check:
            // - if it's a sibling: then we vote for merging from our block to the union of our
            // block and that block
            // - if it's an ancestor's sibling: then we vote for merging from our block with every
            // sibling of that block
            for block in our_blocks(current_blocks, our_name) {
                if is_sibling(block.prefix, candidate.prefix) {
                    let target = merged_block(candidate, block);
                    let vote = Vote {
                        from: block.clone(),
                        to: target,
                    };
                    votes.insert(vote);
                } else if is_sibling_of_ancestor(block.prefix, candidate.prefix) {
                    if let Some(sibling_prefix) = block.prefix.sibling() {
                        for sibling_block in blocks_for_prefix(current_blocks, sibling_prefix) {
                            let target = merged_block(sibling_block, block);
                            let vote = Vote {
                                from: block.clone(),
                                to: target,
                            };
                            votes.insert(vote);
                        }
                    }
                }
            }
        }
    }
    votes.into_iter().collect()
}

fn find_small_blocks<'a>(current_blocks: &'a CurrentBlocks,
                         min_section_size: usize)
                         -> Box<Iterator<Item = &'a Block> + 'a> {
    Box::new(current_blocks
                 .iter()
                 .filter(move |&b| {
                             b.prefix.bit_count() > 0 && b.members.len() < min_section_size
                         }))
}

fn is_sibling_of_ancestor(our_prefix: Prefix, target: Prefix) -> bool {
    target.bit_count() < our_prefix.bit_count() && !target.is_prefix_of(&our_prefix) &&
    target.popped().is_prefix_of(&our_prefix)
}

fn is_sibling(our_prefix: Prefix, target: Prefix) -> bool {
    target.sibling() == Some(our_prefix)
}

fn merged_block(b0: &Block, b1: &Block) -> Block {
    assert_eq!(b0.prefix.sibling(), Some(b1.prefix));
    Block {
        prefix: b0.prefix.popped(),
        version: ::std::cmp::max(b0.version, b1.version) + 1,
        members: b0.members.union(&b1.members).cloned().collect(),
    }
}
