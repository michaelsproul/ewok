use std::ops::Deref;
use std::collections::{BTreeSet, BTreeMap, HashMap};
use std::borrow::Borrow;

use block::{BlockId, Block, Vote};
use name::{Name, Prefix};

pub type ValidBlocks = BTreeSet<BlockId>;
pub type CurrentBlocks = BTreeSet<BlockId>;

/// Mapping from votes to voters: (vote.from -> (vote.to -> names)).
pub type VoteCounts = BTreeMap<BlockId, BTreeMap<BlockId, BTreeSet<Name>>>;

pub struct Blocks(HashMap<BlockId, Block>);

impl Deref for Blocks {
    type Target = HashMap<BlockId, Block>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Blocks {
    pub fn new() -> Blocks {
        Blocks(HashMap::new())
    }

    pub fn insert(&mut self, block: Block) -> BlockId {
        let id = block.get_id();
        self.0.insert(id, block);
        id
    }

    /// Compute the set of blocks that become valid as a result of adding `new_vote`.
    ///
    /// * `valid_blocks`: the set of valid blocks.
    /// * `vote_counts`: the vote counts, including the vote for `new_vote` that was just added.
    /// * `new_vote`: a vote that just voted for by a node.
    ///
    /// Return value:
    /// Set of votes that become valid as a result of `new_vote`. The `to` blocks of these
    /// votes are the new valid blocks that should be added to `valid_blocks`.
    pub fn new_valid_blocks(
        &self,
        valid_blocks: &ValidBlocks,
        vote_counts: &VoteCounts,
        new_votes: BTreeSet<Vote>,
    ) -> Vec<(Vote, BTreeSet<Name>)> {
        // Set of valid blocks to branch out from.
        // Stored as a set of votes where the frontier blocks are the "to" component,
        // and the nodes that voted for them are held alongside (a little hacky).
        let mut frontier: BTreeSet<(Vote, BTreeSet<Name>)> = BTreeSet::new();

        // Set of votes for new valid blocks.
        let mut new_valid_votes = vec![];

        // If a new vote extends an existing valid block, we need to add it to the frontier set
        // so we can branch out from it.
        for new_vote in new_votes {
            if valid_blocks.contains(&new_vote.from) {
                // This dummy vote is a bit of hack, we really just need init_vote.to = new_vote.from.
                let init_vote = Vote {
                    from: new_vote.from,
                    to: new_vote.from,
                };
                frontier.insert((init_vote, BTreeSet::new()));
            }
        }

        let mut visited_edges = BTreeSet::new();
        // the frontier votes' `to` blocks are already validated
        while !frontier.is_empty() {
            let mut new_frontier = BTreeSet::new();

            for &(ref vote, _) in frontier.iter() {
                visited_edges.insert(vote.clone());
            }

            for (vote, voters) in frontier {
                // Branch out to all now valid successors of this block which we haven't visited
                // yet.
                new_frontier.extend(self.successors(vote_counts, vote.to).into_iter().filter(
                    |&(ref vote, _)| !visited_edges.contains(vote),
                ));

                // Frontier block is valid. If new, add its vote to the set of new valid votes.
                if !valid_blocks.contains(&vote.to) {
                    new_valid_votes.push((vote, voters));
                }
            }

            frontier = new_frontier;
        }

        new_valid_votes
    }

    /// Return all votes for blocks that succeed the given block.
    ///
    /// a succeeds b == b witnesses a.
    fn successors<'a>(
        &self,
        vote_counts: &'a VoteCounts,
        from: BlockId,
    ) -> Vec<(Vote, BTreeSet<Name>)> {
        let from_block = from.into_block(self);
        vote_counts
            .get(&from)
            .into_iter()
            .flat_map(|inner_map| inner_map.iter())
            .map(|(id, votes)| (id.into_block(self), votes))
            .filter(move |&(succ, _)| {
                succ.prefix.is_neighbour(&from_block.prefix) || succ.is_admissible_after(from_block)
            })
            .map(move |(succ, voters)| {
                let vote = Vote {
                    from: from,
                    to: succ.get_id(),
                };
                (vote, voters.clone())
            })
            .filter(|&(ref vote, ref voters)| vote.is_quorum(self, voters))
            .collect()
    }

    /// Compute the set of candidates for current blocks from a set of valid blocks.
    pub fn compute_current_candidate_blocks(&self, valid_blocks: ValidBlocks) -> ValidBlocks {
        // 1. Sort by version.
        let mut blocks_by_version: BTreeMap<u64, BTreeSet<&Block>> = btreemap!{};
        for block_id in valid_blocks {
            let block = block_id.into_block(self);
            blocks_by_version
                .entry(block.version)
                .or_insert_with(BTreeSet::new)
                .insert(block);
        }

        // 2. Collect blocks not covered by higher-version prefixes.
        let mut current_blocks: BTreeSet<&Block> = btreeset!{};
        let mut current_pfxs: BTreeSet<Prefix> = btreeset!{};
        for (_, blocks) in blocks_by_version.into_iter().rev() {
            let new_current: Vec<&Block> = blocks
                .into_iter()
                .filter(|block| !block.prefix.is_covered_by(&current_pfxs))
                .collect();
            current_pfxs.extend(new_current.iter().map(|block| block.prefix));
            current_blocks.extend(new_current);
        }

        current_blocks.into_iter().map(|b| b.get_id()).collect()
    }

    pub fn compute_current_blocks(&self, candidate_blocks: &ValidBlocks) -> CurrentBlocks {
        self.block_contents(candidate_blocks)
            .into_iter()
            .filter(|b| {
                !self.block_contents(candidate_blocks).into_iter().any(|c| {
                    c.outranks(b)
                })
            })
            .map(|b| b.get_id())
            .collect()
    }

    /// Blocks that we can legitimately vote on successors for, because we are part of them.
    pub fn our_blocks<'a>(&'a self, blocks: &BTreeSet<BlockId>, our_name: Name) -> Vec<&'a Block> {
        self.block_contents(blocks)
            .into_iter()
            .filter(move |b| b.members.contains(&our_name))
            .collect()
    }

    /// Set of current prefixes we belong to
    pub fn our_prefixes(&self, blocks: &BTreeSet<BlockId>, our_name: Name) -> BTreeSet<Prefix> {
        self.our_blocks(blocks, our_name)
            .into_iter()
            .map(|b| b.prefix)
            .collect()
    }

    /// Blocks that match our name, but that we are not necessarily a part of.
    pub fn section_blocks<'a>(
        &'a self,
        blocks: &BTreeSet<BlockId>,
        our_name: Name,
    ) -> Vec<&'a Block> {
        self.block_contents(blocks)
            .into_iter()
            .filter(move |block| block.prefix.matches(our_name))
            .collect()
    }

    /// Blocks that contain a given prefix.
    pub fn blocks_for_prefix<'a>(
        &'a self,
        blocks: &BTreeSet<BlockId>,
        prefix: Prefix,
    ) -> Vec<&'a Block> {
        self.block_contents(blocks)
            .into_iter()
            .filter(move |&b| b.prefix == prefix)
            .collect()
    }

    /// Find predecessors of the given block with a quorum of votes.
    ///
    /// Return the predecessors (blocks), as well as the votes (edges) from those predecessors to `block`.
    pub fn predecessors(
        &self,
        block: &BlockId,
        rev_votes: &VoteCounts,
    ) -> BTreeSet<(BlockId, Vote, BTreeSet<Name>)> {
        rev_votes.get(block).map_or_else(BTreeSet::new, |map| {
            map.into_iter()
                .filter(|&(block_from, votes)| {
                    let vote = Vote {
                        from: *block_from,
                        to: *block,
                    };
                    vote.is_quorum(self, votes)
                })
                .map(|(block_from, votes)| {
                    (
                        *block_from,
                        Vote {
                            from: *block_from,
                            to: *block,
                        },
                        votes.clone(),
                    )
                })
                .collect()
        })
    }

    /// Get all the votes for the history of `block` back to the last split.
    pub fn chain_segment(
        &self,
        block: &BlockId,
        rev_votes: &VoteCounts,
    ) -> BTreeSet<(Vote, BTreeSet<Name>)> {
        let mut segment_votes = btreeset!{};

        let mut oldest_block = block.into_block(self);
        let block = oldest_block;

        // Go back in history until we find the block that our section split out of.
        while block.prefix.is_prefix_of(&oldest_block.prefix) && oldest_block.version > 0 {
            let predecessors = self.predecessors(&oldest_block.get_id(), rev_votes);
            match predecessors.into_iter().find(|&(_, ref vote, _)| {
                !vote.is_witnessing(self)
            }) {
                Some((predecessor, vote, voters)) => {
                    segment_votes.insert((vote, voters));
                    oldest_block = predecessor.into_block(self);
                }
                None => {
                    warn!(
                        "WARNING: couldn't find a predecessor for: {:?}",
                        oldest_block
                    );
                    break;
                }
            }
        }

        segment_votes
    }

    pub fn block_contents<'a, K, I: IntoIterator<Item = K>>(&'a self, blocks: I) -> Vec<&'a Block>
    where
        K: Borrow<BlockId>,
    {
        blocks
            .into_iter()
            .map(|b| b.borrow().into_block(self))
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn short_name(name: u8) -> Name {
        Name((name as u64) << (64 - 8))
    }

    #[test]
    fn covering() {
        let mut blocks = Blocks::new();
        let block1 = blocks.insert(Block {
            prefix: Prefix::empty(),
            version: 0,
            members: btreeset!{ Name(0), short_name(0b10000000) },
        });
        let block2 = blocks.insert(Block {
            prefix: Prefix::short(1, 0),
            version: 1,
            members: btreeset!{ Name(0) },
        });
        let valid_blocks = btreeset![block1, block2];

        let expected_current = btreeset![block1];

        let candidates = blocks.compute_current_candidate_blocks(valid_blocks);
        let current_blocks = blocks.compute_current_blocks(&candidates);

        assert_eq!(expected_current, current_blocks);
    }

    #[test]
    fn segment() {
        let b1_members =
            btreeset!{ Name(0), Name(1), Name(2), Name(3 & (1 << 63)), Name(4 & (1 << 63)) };
        let b1 = Block {
            prefix: Prefix::empty(),
            version: 0,
            members: b1_members.clone(),
        };
        let b2_members = btreeset!{ Name(0), Name(1), Name(2) };
        let b2 = Block {
            prefix: Prefix::short(1, 0),
            version: 1,
            members: b2_members.clone(),
        };
        let b3_members = &b2_members | &btreeset!{ Name(5) };
        let b3 = Block {
            prefix: Prefix::short(1, 0),
            version: 2,
            members: b3_members.clone(),
        };
        let mut blocks = Blocks::new();
        let b1_id = blocks.insert(b1);
        let b2_id = blocks.insert(b2);
        let b3_id = blocks.insert(b3);

        let rev_votes =
            btreemap! {
            b2_id => btreemap! {
                b1_id => b1_members.clone(),
            },
            b3_id => btreemap! {
                b2_id => b2_members.clone(),
            },
        };

        let segment_votes = blocks.chain_segment(&b3_id, &rev_votes);

        let v12 = Vote {
            from: b1_id,
            to: b2_id,
        };
        let v23 = Vote {
            from: b2_id,
            to: b3_id,
        };
        let expected =
            btreeset! {
            (v12, b1_members),
            (v23, b2_members),
        };
        assert_eq!(segment_votes, expected);
    }
}
