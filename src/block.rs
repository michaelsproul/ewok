use name::{Prefix, Name};
use blocks::Blocks;

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

    pub fn is_quorum(&self, blocks: &Blocks, voters: &BTreeSet<Name>) -> bool {
        let from = self.from.into_block(blocks);
        let to = self.to.into_block(blocks);
        let members = if to.members.len() == from.members.len() - 1 &&
            from.members.difference(&to.members).count() == 1
        {
            &to.members
        } else {
            &from.members
        };
        is_quorum_of(voters, members)
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
        }
        // Merge case
        else if other.prefix.popped() == self.prefix {
            let filtered = self.members.iter().filter(
                |name| other.prefix.matches(**name),
            );
            other.members.iter().eq(filtered)
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
        }
        // Merge case
        else if other.prefix.popped() == self.prefix {
            true
        } else {
            false
        }
    }
}

/// Return true if `voters` form a quorum of `members`.
fn is_quorum_of(voters: &BTreeSet<Name>, members: &BTreeSet<Name>) -> bool {
    #[cfg(not(feature = "fast"))]
    let valid_voters = voters & members;
    #[cfg(feature = "fast")]
    let valid_voters = voters;

    assert_eq!(voters.len(), valid_voters.len());
    valid_voters.len() * 2 > members.len()
}
