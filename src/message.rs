use block::{Vote, VoteCounts, CurrentBlocks};
use name::Name;
use self::MessageContent::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Message {
    pub sender: Name,
    pub recipient: Name,
    pub content: MessageContent,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageContent {
    /// Vote for a block to succeed another block.
    VoteMsg(Vote),
    /// Notification that we believe this vote to be agreed by all the listed members.
    VoteAgreedMsg((Vote, BTreeSet<Name>)),
    /// Collection of agreed votes, sent during a merge.
    VoteBundle(BTreeSet<(Vote, BTreeSet<Name>)>),
    /// Message sent from joining node (sender) to all section members (recipients).
    NodeJoined,
    /// Message sent to a joining node to get it up to date on the current blocks.
    BootstrapMsg(VoteCounts),
    /// Connect and disconnect represent the connection or disconnection of two nodes.
    /// Can be sent from node-to-node or from the simulation to a pair of nodes (for disconnects
    /// and reconnects).
    /// See handling in node.rs.
    Connect,
    /// ^See above.
    Disconnect
}

impl MessageContent {
    pub fn recipients(&self, current_blocks: &CurrentBlocks) -> BTreeSet<Name> {
        match *self {
            // Send votes only to our section.
            VoteMsg(Vote { ref from, ref to }) => {
                &from.members | &to.members
            }
            // Send anything else to all connected neighbours.
            _ => {
                current_blocks.iter()
                    .flat_map(|block| block.members.iter().cloned())
                    .collect()
            }
        }
    }
}
