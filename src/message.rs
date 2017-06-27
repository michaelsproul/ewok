use block::Vote;
use blocks::{VoteCounts, CurrentBlocks, Blocks};
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
    Disconnect,
}

impl MessageContent {
    pub fn recipients(
        &self,
        blocks: &Blocks,
        current_blocks: &CurrentBlocks,
        our_name: Name,
    ) -> BTreeSet<Name> {
        match *self {
            // Send votes only to our section.
            VoteMsg(Vote { ref from, ref to }) => {
                let from = blocks.get(from).unwrap();
                let to = blocks.get(to).unwrap();
                &from.members | &to.members
            }
            // Send agreed votes only to neighbours of from and to
            VoteAgreedMsg((Vote { ref from, ref to }, _)) => {
                let from = blocks.get(from).unwrap();
                let to = blocks.get(to).unwrap();
                if from.members.contains(&our_name) {
                    blocks
                        .block_contents(current_blocks)
                        .into_iter()
                        .filter(|b| {
                            b.prefix.is_neighbour(&from.prefix) || b.prefix.is_neighbour(&to.prefix)
                        })
                        .flat_map(|block| block.members.iter().cloned())
                        .collect()
                } else {
                    blocks
                        .block_contents(current_blocks)
                        .into_iter()
                        .filter(|b| {
                            b.prefix.is_neighbour(&to.prefix) &&
                                !b.prefix.is_neighbour(&from.prefix) &&
                                b.prefix != from.prefix
                        })
                        .flat_map(|block| block.members.iter().cloned())
                        .collect()
                }
            }
            // Send anything else to all connected neighbours.
            _ => {
                blocks
                    .block_contents(current_blocks)
                    .into_iter()
                    .flat_map(|block| block.members.iter().cloned())
                    .collect()
            }
        }
    }
}
