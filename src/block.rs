use name::{Prefix, Name};
use blocks::{Blocks, VoteCounts};

use std::cmp;
use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockId(u64);

impl BlockId {
    pub fn into_block<'a>(&self, blocks: &'a Blocks) -> &'a Block {
        blocks.get(self).unwrap()
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    pub prefix: Prefix,
    pub version: u64,
    pub members: BTreeSet<Name>,
}

impl Block {
    /// Returns `true` if `other` should be removed from the current blocks when `self` is a
    /// current candidate.
    pub fn outranks(&self, other: &Block) -> bool {
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

    pub fn should_split(&self, min_split_size: usize) -> bool {
        let p0 = self.prefix.pushed(false);
        let mut len0 = 0;
        let mut len1 = 0;
        for name in &self.members {
            if p0.matches(*name) {
                len0 += 1;
            } else {
                len1 += 1;
            }
        }
        len0 >= min_split_size && len1 >= min_split_size
    }

    pub fn get_id(&self) -> BlockId {
        let mut s = DefaultHasher::new();
        self.hash(&mut s);
        BlockId(s.finish())
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vote {
    pub from: BlockId,
    pub to: BlockId,
}

impl Vote {
    pub fn as_debug<'a>(&self, blocks: &'a Blocks) -> DebugVote<'a> {
        DebugVote {
            from: self.from.into_block(blocks),
            to: self.to.into_block(blocks),
        }
    }

    pub fn is_witnessing(&self, blocks: &Blocks) -> bool {
        !self.to.into_block(blocks).is_admissible_after(
            self.from.into_block(blocks),
        )
    }
}

#[derive(Debug)]
pub struct DebugVote<'a> {
    pub from: &'a Block,
    pub to: &'a Block,
}

#[cfg(feature = "fast")]
fn abs_diff(x: usize, y: usize) -> usize {
    if x >= y { x - y } else { y - x }
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
    ///
    /// We have 2 versions of this function: a real BFT one and a fast one that's only
    /// safe in the simulation.
    #[cfg(not(feature = "fast"))]
    pub fn is_admissible_after(&self, other: &Block) -> bool {
        // This is the proper BFT version of `is_admissible_after`.
        if self.version <= other.version {
            return false;
        }

        // Add/remove case.
        if self.prefix == other.prefix {
            self.members.symmetric_difference(&other.members).count() == 1
        }
        // Split case.
        else if self.prefix.popped() == other.prefix {
            let filtered = other.members.iter().filter(
                |name| self.prefix.matches(**name),
            );
            self.members.iter().eq(filtered)
        } else {
            false
        }
    }

    #[cfg(feature = "fast")]
    pub fn is_admissible_after(&self, other: &Block) -> bool {
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
        } else {
            false
        }
    }

    /// Return true if this block is valid with respect to a set of valid blocks.
    pub fn is_valid_from(&self, anchor: &Block, rev_votes: &VoteCounts, blocks: &Blocks) -> bool {
        let block_id = self.get_id();

        let from_votes = match rev_votes.get(&block_id) {
            Some(map) => map,
            None => return false,
        };

        let mut zero_parent = None;
        let mut one_parent = None;

        for (from_block_id, voters) in from_votes {
            let from_block = from_block_id.into_block(blocks);

            if !is_quorum_of(voters, &from_block.members) {
                continue;
            }

            // Non-merge block, we're all good.
            if (self.is_admissible_after(from_block) ||
                self.prefix.is_neighbour(&from_block.prefix)) &&
                anchor == from_block
            {
                return true
            }

            // Check for merge parents.
            // TODO: might need to try different pairings of zero and one parent blocks??
            if from_block.prefix == self.prefix.pushed(false) {
                zero_parent = Some(from_block);
            } else if from_block.prefix == self.prefix.pushed(true) {
                one_parent = Some(from_block);
            }
        }

        if let (Some(zero_parent), Some(one_parent)) = (zero_parent, one_parent) {
            (anchor == zero_parent || anchor == one_parent) &&
            self.version == cmp::max(zero_parent.version, one_parent.version) + 1 &&
            self.members == (&zero_parent.members) | (&one_parent.members)
        } else {
            false
        }
    }
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
