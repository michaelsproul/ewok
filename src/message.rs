use block::{Vote, VoteCounts};
use name::Name;

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Event {
    pub src: Name,
    pub dst: Name,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Content {
    Message(Message),
    Notification(Notification)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Message {
    /// Vote for a block to succeed another block.
    VoteMsg(Vote),
    /// Notification that we believe this vote to be agreed by all the listed members.
    VoteAgreedMsg((Vote, BTreeSet<Name>)),
    /// Message sent to a joining node to get it up to date on the current blocks.
    BootstrapMsg(VoteCounts),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Notification {
    /// Message sent from joining node (sender) to all section members (recipients).
    NodeJoined,
    /// Pseudo-message sent to tell recipient that they've lost their connection to sender.
    ConnectionLost,
    /// Pseudo-message sent to tell recipient that they've regained a connection to sender.
    ConnectionRegained,
}
