use block::{BlockId, Vote};
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
    VoteBundle(Vec<(Vote, BTreeSet<Name>)>),
    /// Request for a proof for the given block
    RequestProof(BlockId, CurrentBlocks),
    /// Means that the node couldn't prove the requested block
    NoProof(BlockId),
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
            // Send votes to members of the `from` and `to` blocks.
            VoteMsg(ref vote) => {
                let from = vote.from.into_block(blocks);
                let to = vote.to.into_block(blocks);
                &from.members | &to.members
            }
            // Send vote agreements to all our neighbours if we are in the `from` or `to` block.
            VoteAgreedMsg((Vote { ref from, ref to }, _)) => {
                let from = from.into_block(blocks);
                let to = to.into_block(blocks);
                if from.members.contains(&our_name) || to.members.contains(&our_name) {
                    blocks
                        .block_contents(current_blocks)
                        .into_iter()
                        .flat_map(|block| block.members.iter().cloned())
                        .collect()
                } else {
                    btreeset!{}
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
