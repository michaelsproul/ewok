use block::{BlockId, Vote};
use blocks::{VoteCounts, CurrentBlocks, Blocks};
use name::{Name, Prefix};
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
    /// Vote to approve a candidate. Set is of all nodes voting to approve this candidate.
    ApproveCandidate(Name, BTreeSet<Name>),
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

// XOR distance between the lower bounds of two prefixes.
fn prefix_dist(p1: &Prefix, p2: &Prefix) -> u64 {
    p1.lower_bound().0 ^ p2.lower_bound().0
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
            VoteAgreedMsg((Vote { ref from, ref to }, _)) => {
                let from = from.into_block(blocks);
                let to = to.into_block(blocks);

                // Only broadcast vote agreement if we are in the `from` or `to` block.
                if !from.members.contains(&our_name) && !to.members.contains(&our_name) {
                    return btreeset!{};
                }

                let current_blocks = blocks.block_contents(current_blocks);

                // Send vote agreements to any neighbour section N with block `b1`, such that:
                // 1. N is a neighbour of the `from` or `to` prefix, and
                // 2. Our current prefix is the closest (by XOR distance) to N's prefix,
                // amongst the set of current sections which have prefixes compatible with
                // either the `from` prefix or the `to` prefix.
                current_blocks
                    .iter()
                    .filter(|b1| {
                        let cond1 =
                            b1.prefix.is_neighbour(&from.prefix) ||
                            b1.prefix.is_neighbour(&to.prefix);
                        // (2)
                        let cond2 = current_blocks
                            .iter()
                            .filter(|b2| {
                                b2.prefix.is_compatible(&from.prefix) ||
                                b2.prefix.is_compatible(&to.prefix)
                            })
                            // Min by XOR distance.
                            .min_by_key(|b2| prefix_dist(&b1.prefix, &b2.prefix))
                            // Are we in the section that's closest by XOR distance?
                            .map(|closest| closest.prefix.matches(our_name))
                            .unwrap_or(false);

                        cond1 && cond2
                    })
                    .inspect(|block| debug!("Node({}): broadcasting vote from {:?} v{} to {:?} v{} to neighbour {:?}",
                        our_name, from.prefix, from.version, to.prefix, to.version, block.prefix
                    ))
                    .flat_map(|block| block.members.iter().cloned())
                    .collect()
            }
            // Send candidate-related votes to our own section.
            ApproveCandidate(..) => {
                blocks.section_blocks(current_blocks, our_name)
                    .into_iter()
                    .flat_map(|block| block.members.clone())
                    .collect()
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
