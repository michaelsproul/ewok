use name::{Prefix, Name};

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    pub prefix: Prefix,
    pub version: u64,
    pub members: BTreeSet<Name>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vote {
    pub from: Block,
    pub to: Block,
}

pub type ValidBlocks = BTreeSet<Block>;
pub type CurrentBlocks = BTreeSet<Block>;

/// Mapping from votes to voters: (vote.from -> (vote.to -> names)).
pub type VoteCounts = BTreeMap<Block, BTreeMap<Block, BTreeSet<Name>>>;

impl Block {
    /// Create a genesis block.
    pub fn genesis(name: Name) -> Self {
        Block {
            prefix: Prefix::default(),
            version: 0,
            members: btreeset!{name},
        }
    }

    /// Create a new block with a node added.
    pub fn add_node(&self, added: Name) -> Self {
        let mut members = self.members.clone();
        members.insert(added);
        Block {
            prefix: self.prefix,
            version: self.version + 1,
            members,
        }
    }

    /// Create a new block with a node removed.
    pub fn remove_node(&self, removed: Name) -> Self {
        let mut members = self.members.clone();
        assert!(members.remove(&removed));
        Block {
            prefix: self.prefix,
            version: self.version + 1,
            members,
        }
    }

    /// Is this block admissible after the given other block?
    pub fn is_admissible_after(&self, other: &Block) -> bool {
        if self.version <= other.version {
            return false;
        }

        // Add/remove case.
        if self.prefix == other.prefix {
            self.members
                .symmetric_difference(&other.members)
                .count() == 1
        }
        // Split case.
        else if self.prefix.popped() == other.prefix {
            let filtered = other
                .members
                .iter()
                .filter(|name| self.prefix.matches(**name));
            self.members.iter().eq(filtered)
        }
        // Merge case
        else if other.prefix.popped() == self.prefix {
            let filtered = self.members
                .iter()
                .filter(|name| other.prefix.matches(**name));
            other.members.iter().eq(filtered)
        } else {
            false
        }
    }
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
pub fn new_valid_blocks(valid_blocks: &ValidBlocks,
                        vote_counts: &VoteCounts,
                        new_vote: &Vote)
                        -> Vec<(Vote, BTreeSet<Name>)> {
    // Set of valid blocks to branch out from.
    // Stored as a set of votes where the frontier blocks are the "to" component,
    // and the nodes that voted for them are held alongside (a little hacky).
    let mut frontier: BTreeSet<(Vote, BTreeSet<Name>)> = BTreeSet::new();

    // Set of votes for new valid blocks.
    let mut new_valid_votes = vec![];

    // If the new vote extends an existing valid block, we need to add it to the frontier set
    // so we can branch out from it.
    if valid_blocks.contains(&new_vote.from) {
        // This dummy vote is a bit of hack, we really just need init_vote.to = new_vote.from.
        let init_vote = Vote {
            from: new_vote.from.clone(),
            to: new_vote.from.clone(),
        };
        frontier.insert((init_vote, BTreeSet::new()));
    } else {
        return new_valid_votes;
    }

    while !frontier.is_empty() {
        let mut new_frontier = BTreeSet::new();

        for (vote, voters) in frontier {
            // Branch out to all now valid successors of this block.
            new_frontier.extend(successors(vote_counts, &vote.to));

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
fn successors<'a>(vote_counts: &'a VoteCounts,
                  from: &'a Block)
                  -> Box<Iterator<Item = (Vote, BTreeSet<Name>)> + 'a> {
    let iter = vote_counts
        .get(from)
        .into_iter()
        .flat_map(|inner_map| inner_map.iter())
        .filter(move |&(succ, _)| {
                    succ.prefix.is_neighbour(&from.prefix) || succ.is_admissible_after(from)
                })
        .filter(move |&(_, voters)| is_quorum_of(voters, &from.members))
        .map(move |(succ, voters)| {
                 let vote = Vote {
                     from: from.clone(),
                     to: succ.clone(),
                 };
                 (vote, voters.clone())
             });

    Box::new(iter)
}

/// Compute the set of candidates for current blocks from a set of valid blocks.
pub fn compute_current_candidate_blocks(valid_blocks: BTreeSet<Block>) -> ValidBlocks {
    // 1. Sort by version.
    let mut blocks_by_version: BTreeMap<u64, BTreeSet<Block>> = btreemap!{};
    for block in valid_blocks {
        blocks_by_version
            .entry(block.version)
            .or_insert_with(BTreeSet::new)
            .insert(block);
    }

    // 2. Collect blocks not covered by higher-version prefixes.
    let mut current_blocks: BTreeSet<Block> = btreeset!{};
    let mut current_pfxs: BTreeSet<Prefix> = btreeset!{};
    for (_, blocks) in blocks_by_version.into_iter().rev() {
        let new_current: Vec<Block> = blocks
            .into_iter()
            .filter(|block| !block.prefix.is_covered_by(&current_pfxs))
            .collect();
        current_pfxs.extend(new_current.iter().map(|block| block.prefix));
        current_blocks.extend(new_current);
    }

    current_blocks
}

pub fn compute_current_blocks(candidate_blocks: &BTreeSet<Block>) -> CurrentBlocks {
    let current_pfxs: BTreeSet<Prefix> = candidate_blocks.into_iter().map(|b| b.prefix).collect();
    // Remove all blocks whose prefix is a descendant of any other current prefix
    let current_blocks: BTreeSet<Block> = candidate_blocks
        .into_iter()
        .filter(|b| {
                    !current_pfxs
                         .iter()
                         .any(|pfx| pfx.is_prefix_of(&b.prefix) && *pfx != b.prefix)
                })
        .cloned()
        .collect();

    // Remove blocks with fewer members than any other block with the same prefix
    let mut max_members: BTreeMap<Prefix, usize> = btreemap!{};
    for block in &current_blocks {
        let members_for_pfx = max_members.entry(block.prefix).or_insert(0);
        if *members_for_pfx < block.members.len() {
            *members_for_pfx = block.members.len();
        }
    }
    current_blocks
        .into_iter()
        .filter(|b| Some(&b.members.len()) == max_members.get(&b.prefix))
        .collect()
}

/// Return true if `voters` form a quorum of `members`.
pub fn is_quorum_of(voters: &BTreeSet<Name>, members: &BTreeSet<Name>) -> bool {
    let valid_voters = voters & members;
    assert_eq!(voters.len(), valid_voters.len());
    valid_voters.len() * 2 > members.len()
}

/// Blocks that we can legitimately vote on successors for, because we are part of them.
pub fn our_blocks<'a>(blocks: &'a BTreeSet<Block>,
                      our_name: Name)
                      -> Box<Iterator<Item = &'a Block> + 'a> {
    let ours = blocks
        .iter()
        .filter(move |b| b.members.contains(&our_name));
    Box::new(ours)
}

/// Set of current prefixes we belong to
pub fn our_prefixes(blocks: &BTreeSet<Block>, our_name: Name) -> BTreeSet<Prefix> {
    our_blocks(blocks, our_name).map(|b| b.prefix).collect()
}

/// Check whether we have more than one current prefix
pub fn is_ambiguous(blocks: &BTreeSet<Block>, our_name: Name) -> bool {
    our_prefixes(blocks, our_name).len() > 1
}

/// Blocks that match our name, but that we are not necessarily a part of.
pub fn section_blocks<'a>(blocks: &'a BTreeSet<Block>,
                          our_name: Name)
                          -> Box<Iterator<Item = &'a Block> + 'a> {
    let section_blocks = blocks
        .iter()
        .filter(move |block| block.prefix.matches(our_name));
    Box::new(section_blocks)
}

/// Blocks that contain a given prefix.
pub fn blocks_for_prefix<'a>(blocks: &'a BTreeSet<Block>,
                             prefix: Prefix)
                             -> Box<Iterator<Item = &'a Block> + 'a> {
    let result = blocks.iter().filter(move |&b| b.prefix == prefix);
    Box::new(result)
}

#[cfg(test)]
mod test {
    use super::*;

    fn short_name(name: u8) -> Name {
        Name((name as u64) << (64 - 8))
    }

    #[test]
    fn covering() {
        let valid_blocks = btreeset![
            Block {
                prefix: Prefix::empty(),
                version: 0,
                members: btreeset!{ Name(0), short_name(0b10000000) }
            },
            Block {
                prefix: Prefix::short(1, 0),
                version: 1,
                members: btreeset!{ Name(0) }
            },
        ];

        let expected_current = btreeset![
            Block {
                prefix: Prefix::empty(),
                version: 0,
                members: btreeset!{ Name(0), short_name(0b10000000) }
            },
        ];

        let candidates = compute_current_candidate_blocks(valid_blocks);
        let current_blocks = compute_current_blocks(&candidates);

        assert_eq!(expected_current, current_blocks);
    }
}
