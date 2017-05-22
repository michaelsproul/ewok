use name::{Prefix, Name};

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    pub prefix: Prefix,
    pub version: u64,
    pub members: BTreeSet<Name>
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vote {
    pub from: Block,
    pub to: Block
}

pub type ValidBlocks = BTreeSet<Block>;
pub type CurrentBlocks = BTreeSet<Block>;
pub type VoteCounts = BTreeMap<Vote, BTreeSet<Name>>;

impl Block {
    /// Create a genesis block.
    pub fn genesis(name: Name) -> Self {
        Block {
            prefix: Prefix::default(),
            version: 0,
            members: btreeset!{name}
        }
    }

    /// Create a new block with a node added.
    pub fn add_node(&self, added: Name) -> Self {
        let mut members = self.members.clone();
        members.insert(added);
        Block {
            prefix: self.prefix,
            version: self.version + 1,
            members
        }
    }

    /// Create a new block with a node removed.
    pub fn remove_node(&self, removed: Name) -> Self {
        let mut members = self.members.clone();
        assert!(members.remove(&removed));
        Block {
            prefix: self.prefix,
            version: self.version + 1,
            members
        }
    }

    // Is this block admissable after the given other block?
    pub fn is_admissable_after(&self, other: &Block) -> bool {
        // FIXME: super incomplete, but should work for Adds
        self.version > other.version &&
        self.prefix == other.prefix
    }
}

/// Compute the set of blocks that become valid as a result of adding `new_vote`.
///
/// * valid_blocks: the set of valid blocks.
/// * vote_counts: the vote counts, including the vote for `new_vote` that was just added.
/// * new_vote: a vote that just voted for by a node.
///
/// Return value:
/// Set of votes that become valid as a result of `new_vote`. The `to` blocks of these
/// votes are the new valid blocks that should be added to `valid_blocks`.
pub fn new_valid_blocks(valid_blocks: &ValidBlocks,
                        vote_counts: &VoteCounts,
                        new_vote: &Vote) -> Vec<(Vote, BTreeSet<Name>)> {
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
        let init_vote = Vote { from: new_vote.from.clone(), to: new_vote.from.clone() };
        frontier.insert((init_vote, BTreeSet::new()));
    } else {
        return new_valid_votes;
    }

    while !frontier.is_empty() {
        let mut new_frontier = BTreeSet::new();

        for (vote, voters) in frontier {
            // Frontier block is valid. If new, add its vote to the set of new valid votes.
            if !valid_blocks.contains(&vote.to) {
                new_valid_votes.push((vote.clone(), voters));
            }

            // Branch out to all now valid successors of this block.
            new_frontier.extend(successors(vote_counts, &vote.to));
        }

        frontier = new_frontier;
    }

    new_valid_votes
}

/// Return all votes for blocks that succeed the given block.
///
/// a succeeds b == b witnesses a.
fn successors<'a>(vote_counts: &'a VoteCounts,
                  from: &'a Block) -> Box<Iterator<Item=(Vote, BTreeSet<Name>)> + 'a>
{
    // TODO: could be more efficient with look-up by `from` block.
    let iter = vote_counts.iter()
        .filter(move |&(vote, _)| {
            &vote.from == from
        })
        .filter(|&(vote, _)| {
            vote.to.is_admissable_after(&vote.from)
        })
        .filter(|&(vote, voters)| {
            is_quorum_of(voters, &vote.from.members)
        })
        .map(|(vote, voters)| {
            (vote.clone(), voters.clone())
        });

    Box::new(iter)
}

/// Compute the set of current blocks from a set of valid blocks.
pub fn compute_current_blocks(mut valid_blocks: Vec<Block>) -> CurrentBlocks {
    let mut current_blocks = btreeset!{};

    // 1. Sort by descending version.
    valid_blocks.sort_by(|b1, b2| b2.version.cmp(&b1.version));

    // 2. Take blocks that have a prefix we haven't covered yet,
    // or the same version as a prefix we have covered.
    for block in valid_blocks {
        let is_current = {
            let compatible: Vec<_> = current_blocks.iter().filter(|current: &&Block| {
                current.prefix.is_compatible(block.prefix)
            }).collect();

            if compatible.is_empty() {
                true
            } else {
                compatible.iter().any(|current| block.version == current.version)
            }
        };

        if is_current {
            current_blocks.insert(block);
        }
    }

    current_blocks
}

/// Return true if `voters` form a quorum of `members`.
pub fn is_quorum_of(voters: &BTreeSet<Name>, members: &BTreeSet<Name>) -> bool {
    let valid_voters = voters & members;
    //assert_eq!(voters.len(), valid_voters.len());
    valid_voters.len() * 2 > members.len()
}
