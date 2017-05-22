use name::{Prefix, Name};

use std::collections::BTreeSet;

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
