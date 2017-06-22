use std::rc::Rc;
use name::{Prefix, Name};

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    pub prefix: Prefix,
    pub version: u64,
    pub members: BTreeSet<Name>,
}

impl Block {
    /// Returns `true` if `other` should be removed from the current blocks when `self` is a
    /// current candidate.
    fn outranks(&self, other: &Block) -> bool {
        if self.prefix == other.prefix {
            if self.members.len() != other.members.len() {
                self.members.len() > other.members.len()
            } else {
                self.members > other.members
            }
        } else {
            self.prefix.is_compatible(&other.prefix) &&
            self.prefix.bit_count() < other.prefix.bit_count()
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vote {
    pub from: Rc<Block>,
    pub to: Rc<Block>,
}

pub type ValidBlocks = BTreeSet<Rc<Block>>;
pub type CurrentBlocks = BTreeSet<Rc<Block>>;

/// Mapping from votes to voters: (vote.from -> (vote.to -> names)).
pub type VoteCounts = BTreeMap<Rc<Block>, BTreeMap<Rc<Block>, BTreeSet<Name>>>;

#[cfg(feature = "fast")]
fn abs_diff(x: usize, y: usize) -> usize {
    if x >= y {
        x - y
    } else {
        y - x
    }
}

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
    pub fn add_node(&self, added: Name) -> Rc<Self> {
        let mut members = self.members.clone();
        members.insert(added);
        Rc::new(Block {
            prefix: self.prefix,
            version: self.version + 1,
            members,
        })
    }

    /// Create a new block with a node removed.
    pub fn remove_node(&self, removed: Name) -> Rc<Self> {
        let mut members = self.members.clone();
        assert!(members.remove(&removed));
        Rc::new(Block {
            prefix: self.prefix,
            version: self.version + 1,
            members,
        })
    }


    /// Is this block admissible after the given other block?
    ///
    /// We have 2 versions of this function: a real BFT one and a fast one that's only
    /// safe in the simulation.
    #[cfg(not(feature = "fast"))]
    fn is_admissible_after(&self, other: &Block) -> bool {
        // This is the proper BFT version of `is_admissible_after`.
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

    #[cfg(feature = "fast")]
    fn is_admissible_after(&self, other: &Block) -> bool {
        // This is an approximate version of `is_admissible_after` that is sufficient
        // for the simulation (because nobody votes invalidly), and is much faster.
        if self.version <= other.version {
            return false;
        }

        // Add/remove case.
        if self.prefix == other.prefix {
            abs_diff(self.members.len(), other.members.len()) == 1
        }
        // Split case.
        else if self.prefix.popped() == other.prefix {
            true
        }
        // Merge case
        else if other.prefix.popped() == self.prefix {
            true
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
                        new_votes: BTreeSet<Vote>)
                        -> Vec<(Vote, BTreeSet<Name>)> {
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
                from: new_vote.to,
                to: new_vote.from,
            };
            frontier.insert((init_vote, BTreeSet::new()));
        }
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
                  from: &'a Rc<Block>)
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
pub fn compute_current_candidate_blocks(valid_blocks: ValidBlocks) -> ValidBlocks {
    // 1. Sort by version.
    let mut blocks_by_version: BTreeMap<u64, BTreeSet<Rc<Block>>> = btreemap!{};
    for block in valid_blocks {
        blocks_by_version
            .entry(block.version)
            .or_insert_with(BTreeSet::new)
            .insert(block);
    }

    // 2. Collect blocks not covered by higher-version prefixes.
    let mut current_blocks: BTreeSet<Rc<Block>> = btreeset!{};
    let mut current_pfxs: BTreeSet<Prefix> = btreeset!{};
    for (_, blocks) in blocks_by_version.into_iter().rev() {
        let new_current: Vec<Rc<Block>> = blocks
            .into_iter()
            .filter(|block| !block.prefix.is_covered_by(&current_pfxs))
            .collect();
        current_pfxs.extend(new_current.iter().map(|block| block.prefix));
        current_blocks.extend(new_current);
    }

    current_blocks
}

pub fn compute_current_blocks(candidate_blocks: &ValidBlocks) -> CurrentBlocks {
    candidate_blocks
        .iter()
        .filter(|b| !candidate_blocks.iter().any(|c| c.outranks(b)))
        .cloned()
        .collect()
}

/// Return true if `voters` form a quorum of `members`.
pub fn is_quorum_of(voters: &BTreeSet<Name>, members: &BTreeSet<Name>) -> bool {
    #[cfg(not(feature = "fast"))]
    let valid_voters = voters & members;
    #[cfg(feature = "fast")]
    let valid_voters = voters;

    assert_eq!(voters.len(), valid_voters.len());
    valid_voters.len() * 2 > members.len()
}

/// Blocks that we can legitimately vote on successors for, because we are part of them.
pub fn our_blocks<'a>(blocks: &'a BTreeSet<Rc<Block>>,
                      our_name: Name)
                      -> Box<Iterator<Item = &'a Rc<Block>> + 'a> {
    let ours = blocks
        .iter()
        .filter(move |b| b.members.contains(&our_name));
    Box::new(ours)
}

/// Set of current prefixes we belong to
pub fn our_prefixes(blocks: &BTreeSet<Rc<Block>>, our_name: Name) -> BTreeSet<Prefix> {
    our_blocks(blocks, our_name).map(|b| b.prefix).collect()
}

/// Blocks that match our name, but that we are not necessarily a part of.
pub fn section_blocks<'a>(blocks: &'a BTreeSet<Rc<Block>>,
                          our_name: Name)
                          -> Box<Iterator<Item = &'a Rc<Block>> + 'a> {
    let section_blocks = blocks
        .iter()
        .filter(move |block| block.prefix.matches(our_name));
    Box::new(section_blocks)
}

/// Blocks that contain a given prefix.
pub fn blocks_for_prefix<'a>(blocks: &'a BTreeSet<Rc<Block>>,
                             prefix: Prefix)
                             -> Box<Iterator<Item = &'a Rc<Block>> + 'a> {
    let result = blocks.iter().filter(move |&b| b.prefix == prefix);
    Box::new(result)
}

/// Find a predecessor of the given block with a quorum of votes.
///
/// Return the predecessor (block), as well as the vote (edge) from that predecessor to `block`.
// TODO: an index by `to` block would make this O(max_degree) instead of O(n^2).
fn predecessor<'a>(
    block: &Rc<Block>,
    votes: &'a VoteCounts,
) -> Option<(&'a Rc<Block>, Vote, BTreeSet<Name>)>
{
    for (from, map) in votes {
        for (to, voters) in map {
            if to == block && is_quorum_of(voters, &from.members) {
                let vote = Vote {
                    from: from.clone(),
                    to: to.clone(),
                };

                return Some((from, vote, voters.clone()));
            }
        }
    }
    None
}

/// Get all the votes for the history of `block` back to the last split.
pub fn chain_segment(block: &Rc<Block>, votes: &VoteCounts) -> BTreeSet<(Vote, BTreeSet<Name>)> {
    let mut segment_votes = btreeset!{};

    let mut oldest_block = block;

    // Go back in history until we find the block that our section split out of.
    while block.prefix.is_prefix_of(&oldest_block.prefix) && oldest_block.version > 0 {
        match predecessor(oldest_block, votes) {
            Some((predecessor, vote, voters)) => {
                segment_votes.insert((vote, voters));
                oldest_block = predecessor;
            }
            None => {
                warn!("WARNING: couldn't find a predecessor for: {:?}", oldest_block);
                break;
            }
        }
    }

    segment_votes
}

/* FIXME: re-enable tests
#[cfg(test)]
mod test {
    use super::*;

    fn short_name(name: u8) -> Name {
        Name((name as u64) << (64 - 8))
    }

    #[test]
    fn covering() {
        let valid_blocks = btreeset![
            Rc::new(Block {
                prefix: Prefix::empty(),
                version: 0,
                members: btreeset!{ Name(0), short_name(0b10000000) }
            }),
            Rc::new(Block {
                prefix: Prefix::short(1, 0),
                version: 1,
                members: btreeset!{ Name(0) }
            }),
        ];

        let expected_current = btreeset![
            Rc::new(Block {
                prefix: Prefix::empty(),
                version: 0,
                members: btreeset!{ Name(0), short_name(0b10000000) }
            }),
        ];

        let candidates = compute_current_candidate_blocks(valid_blocks);
        let current_blocks = compute_current_blocks(&candidates);

        assert_eq!(expected_current, current_blocks);
    }

    #[test]
    fn segment() {
        let b1_members = btreeset!{ Name(0), Name(1), Name(2), Name(3), Name(4) };
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

        let votes = btreemap! {
            b1.clone() => btreemap! {
                b2.clone() => b1_members.clone(),
            },
            b2.clone() => btreemap! {
                b3.clone() => b2_members.clone(),
            },
        };

        let segment_votes = chain_segment(&b3, &votes);

        let v12 = Vote { from: b1.clone(), to: b2.clone() };
        let v23 = Vote { from: b2.clone(), to: b3.clone() };
        let expected = btreeset! {
            (v12, b1_members),
            (v23, b2_members),
        };
        assert_eq!(segment_votes, expected);
    }
}
*/
