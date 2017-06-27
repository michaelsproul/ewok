use name::Name;
use block::{Block, Vote};
use blocks::{CurrentBlocks, Blocks};
use std::collections::BTreeSet;
use std::cmp;

pub fn merge_blocks(
    blocks: &mut Blocks,
    current_blocks: &CurrentBlocks,
    connections: &BTreeSet<Name>,
    our_name: Name,
    min_section_size: usize,
) -> Vec<Vote> {
    let mut result = merge_rule(blocks, current_blocks, our_name, min_section_size);
    result.extend(force_merge_rule(
        blocks,
        current_blocks,
        connections,
        our_name,
    ));
    result.into_iter().collect()
}

fn force_merge_rule(
    blocks: &mut Blocks,
    current_blocks: &CurrentBlocks,
    connections: &BTreeSet<Name>,
    our_name: Name,
) -> BTreeSet<Vote> {
    let (votes, blocks_to_insert) = {
        let mut votes = BTreeSet::new();
        let mut blocks_to_insert = BTreeSet::new();
        for candidate in current_blocks
            .iter()
            .map(|b| blocks.get(b).unwrap())
            .filter(|&b| lost_quorum(b, connections))
        {
            for our_block in blocks
                .our_blocks(current_blocks, our_name)
                .into_iter()
                .filter(|b| b.prefix.sibling() == Some(candidate.prefix))
            {
                let target = merged_block(candidate, our_block);
                let target_id = target.get_id();
                blocks_to_insert.insert(target);
                let vote = Vote {
                    from: our_block.get_id(),
                    to: target_id,
                };
                votes.insert(vote);
            }
        }
        (votes, blocks_to_insert)
    };
    for block in blocks_to_insert {
        blocks.insert(block);
    }
    votes
}

fn lost_quorum(block: &Block, connections: &BTreeSet<Name>) -> bool {
    let num_active = block
        .members
        .iter()
        .filter(|&name| connections.contains(name))
        .count();
    num_active <= block.members.len() / 2
}

fn merge_rule(
    blocks: &mut Blocks,
    current_blocks: &CurrentBlocks,
    our_name: Name,
    min_section_size: usize,
) -> BTreeSet<Vote> {
    let (votes, blocks_to_insert) = {
        // find blocks that describe sections below threshold
        let candidates = find_small_blocks(blocks, current_blocks, min_section_size);
        let mut votes = BTreeSet::new();
        let mut blocks_to_insert = BTreeSet::new();
        for candidate in candidates {
            if candidate.members.contains(&our_name) {
                // if the block contains our name, vote for merging with all current siblings
                let sibling_prefix = candidate.prefix.sibling().unwrap();
                for block in blocks.blocks_for_prefix(current_blocks, sibling_prefix) {
                    let target = merged_block(candidate, block);
                    let target_id = target.get_id();
                    blocks_to_insert.insert(target);
                    let vote = Vote {
                        from: candidate.get_id(),
                        to: target_id,
                    };
                    votes.insert(vote);
                }
            } else if let Some(candidate_sibling_pfx) = candidate.prefix.sibling() {
                // The block doesn't contain our name - it might be our sibling or a sibling of our
                // ancestor (for each our block it might be different). If that is the case, we vote
                // for merging from our block with every sibling of that block.
                for block in blocks.our_blocks(current_blocks, our_name) {
                    if candidate_sibling_pfx.is_prefix_of(&block.prefix) {
                        if let Some(block_sibling) = block.prefix.sibling() {
                            for sibling_block in blocks.blocks_for_prefix(
                                current_blocks,
                                block_sibling,
                            )
                            {
                                let target = merged_block(sibling_block, block);
                                let target_id = target.get_id();
                                blocks_to_insert.insert(target);
                                let vote = Vote {
                                    from: block.get_id(),
                                    to: target_id,
                                };
                                votes.insert(vote);
                            }
                        }
                    }
                }
            }
        }
        (votes, blocks_to_insert)
    };
    for block in blocks_to_insert {
        blocks.insert(block);
    }
    votes
}

fn find_small_blocks<'a>(
    blocks: &'a Blocks,
    current_blocks: &CurrentBlocks,
    min_section_size: usize,
) -> Vec<&'a Block> {
    current_blocks
        .into_iter()
        .map(|b| blocks.get(b).unwrap())
        .filter(move |&b| {
            b.prefix.bit_count() > 0 && b.members.len() < min_section_size
        })
        .collect()
}

fn merged_block(b0: &Block, b1: &Block) -> Block {
    assert_eq!(b0.prefix.sibling(), Some(b1.prefix));
    Block {
        prefix: b0.prefix.popped(),
        version: cmp::max(b0.version, b1.version) + 1,
        members: b0.members.union(&b1.members).cloned().collect(),
    }
}
