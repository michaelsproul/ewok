use name::Name;
use block::{Block, Vote, CurrentBlocks, our_blocks, our_prefixes, blocks_for_prefix};
use std::collections::BTreeSet;
use std::cmp;

pub fn merge_blocks(current_blocks: &CurrentBlocks,
                    ambiguous_step: Option<u64>,
                    our_name: Name,
                    min_section_size: usize,
                    mergeconv_timeout: u64,
                    step: u64)
                    -> Vec<Vote> {
    let mut result = merge_rule(current_blocks, our_name, min_section_size);
    result.extend(mergeconv_rule(current_blocks,
                                 ambiguous_step,
                                 our_name,
                                 mergeconv_timeout,
                                 step));
    result.into_iter().collect()
}

fn mergeconv_rule(current_blocks: &CurrentBlocks,
                  ambiguous_step: Option<u64>,
                  our_name: Name,
                  mergeconv_timeout: u64,
                  step: u64)
                  -> BTreeSet<Vote> {
    match ambiguous_step {
        Some(amb_step) if step - amb_step > mergeconv_timeout => {
            let mut prefixes = our_prefixes(current_blocks, our_name);
            let shortest = *prefixes
                                .iter()
                                .min_by_key(|pfx| pfx.bit_count())
                                .unwrap(); // safe, because there will be at least one
            // drop the shortest prefix and merge all blocks with longer prefixes
            prefixes.remove(&shortest);
            let mut votes = btreeset!{};
            for block in
                our_blocks(current_blocks, our_name).filter(|b| prefixes.contains(&b.prefix)) {
                let sibling_prefix = block.prefix.sibling().unwrap(); // safe, because it had an ancestor in our_prefixes
                for sibling in blocks_for_prefix(current_blocks, sibling_prefix) {
                    let target = merged_block(sibling, block);
                    let vote = Vote {
                        from: block.clone(),
                        to: target,
                    };
                    votes.insert(vote);
                }
            }
            votes
        }
        _ => btreeset!{},
    }
}

fn merge_rule(current_blocks: &CurrentBlocks,
              our_name: Name,
              min_section_size: usize)
              -> BTreeSet<Vote> {
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
        } else if let Some(candidate_sibling_pfx) = candidate.prefix.sibling() {
            // The block doesn't contain our name - it might be our sibling or a sibling of our
            // ancestor (for each our block it might be different). If that is the case, we vote
            // for merging from our block with every sibling of that block.
            for block in our_blocks(current_blocks, our_name) {
                if candidate_sibling_pfx.is_prefix_of(&block.prefix) {
                    if let Some(block_sibling) = block.prefix.sibling() {
                        for sibling_block in blocks_for_prefix(current_blocks, block_sibling) {
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
    votes
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

fn merged_block(b0: &Block, b1: &Block) -> Block {
    assert_eq!(b0.prefix.sibling(), Some(b1.prefix));
    Block {
        prefix: b0.prefix.popped(),
        version: cmp::max(b0.version, b1.version) + 1,
        members: b0.members.union(&b1.members).cloned().collect(),
    }
}
