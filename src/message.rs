use block::Vote;
use name::Name;

#[derive(Debug)]
pub struct Message {
    pub sender: Name,
    pub recipient: Name,
    pub content: MessageContent
}

#[derive(Debug, Clone)]
pub enum MessageContent {
    /// Vote for a block to succeed another block.
    VoteMsg(Vote),
    /// Pseudo-message sent from joining node (sender) to all section members (recipients).
    NodeJoined,
    // Pseudo-message sent to tell recipient that they've lost their connection to sender.
    //ConnectionLost,
    //Pseudo-message sent to tell sender that they've (re)gained a connection to sender.
    //ConnectionEstablished,
}
