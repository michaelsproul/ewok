use message::Message;
use message::MessageContent;
use message::MessageContent::*;
use name::Name;
use block::{Block, Vote, ValidBlocks, CurrentBlocks, VoteCounts, new_valid_blocks,
            compute_current_blocks, our_blocks};
use peer_state::{PeerStates, in_all_current, in_any_current};
use params::NodeParams;
use split::split_blocks;

use std::iter::FromIterator;
use std::collections::{BTreeMap, BTreeSet};
use std::mem;
use std::fmt;

use self::Node::*;

pub enum Node {
    WaitingToJoin,
    Dead,
    Active(ActiveNode),
}

impl Node {
    pub fn first(name: Name, genesis: Block, params: NodeParams) -> Self {
        Active(ActiveNode::new(name, genesis, params))
    }

    pub fn joining() -> Self {
        WaitingToJoin
    }

    pub fn is_joining(&self) -> bool {
        match *self {
            WaitingToJoin => true,
            _ => false,
        }
    }

    pub fn is_active(&self) -> bool {
        match *self {
            Active(..) => true,
            _ => false,
        }
    }

    pub fn kill(&mut self) {
        *self = Dead;
    }

    pub fn make_active(&mut self, name: Name, genesis: Block, params: NodeParams) {
        println!("Node({}): starting up!", name);
        *self = Active(ActiveNode::new(name, genesis, params));
    }

    /// Returns true if this node is active, the peer is known and its state is `Disconnected`.
    pub fn is_disconnected_from(&self, name: &Name) -> bool {
        match *self {
            Active(ref node) => node.peer_states.is_disconnected_from(name),
            _ => false,
        }
    }
}

pub struct ActiveNode {
    /// Our node's name.
    pub our_name: Name,
    /// All valid blocks.
    pub valid_blocks: ValidBlocks,
    /// Our current blocks.
    pub current_blocks: CurrentBlocks,
    /// Map from blocks to voters for that block.
    pub vote_counts: VoteCounts,
    /// States for peers.
    pub peer_states: PeerStates,
    /// Filter for messages we've already sent and shouldn't resend.
    pub message_filter: BTreeSet<Message>,
    /// Network configuration parameters.
    pub params: NodeParams,
}

impl fmt::Display for ActiveNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Node({})", self.our_name)
    }
}

impl ActiveNode {
    pub fn new(name: Name, genesis: Block, params: NodeParams) -> Self {
        ActiveNode {
            our_name: name,
            valid_blocks: BTreeSet::from_iter(vec![genesis.clone()]),
            current_blocks: BTreeSet::from_iter(vec![genesis]),
            vote_counts: BTreeMap::new(),
            peer_states: PeerStates::new(params.clone()),
            message_filter: BTreeSet::new(),
            params,
        }
    }

    /// Minimum size that all sections must be before splitting.
    fn min_split_size(&self) -> usize {
        self.params.min_section_size as usize + self.params.split_buffer as usize
    }

    /// Insert a vote into our local cache of votes.
    fn add_vote_to_cache<I>(&mut self, vote: Vote, voted_for: I)
        where I: IntoIterator<Item = Name>
    {
        let voters = self.vote_counts
            .entry(vote)
            .or_insert_with(BTreeSet::new);
        voters.extend(voted_for);
    }

    /// Update valid and current block sets, return set of newly valid blocks to broadcast.
    fn update_valid_blocks(&mut self, vote: &Vote) -> Vec<(Vote, BTreeSet<Name>)> {
        // Update valid blocks.
        let new_valid_votes = new_valid_blocks(&self.valid_blocks, &self.vote_counts, vote);
        self.valid_blocks
            .extend(new_valid_votes
                        .iter()
                        .map(|&(ref vote, _)| vote.to.clone()));

        // Update current blocks.
        self.update_current_blocks(&new_valid_votes);

        new_valid_votes
    }

    /// Update the set of current blocks.
    fn update_current_blocks(&mut self, new_votes: &[(Vote, BTreeSet<Name>)]) {
        // Any of the existing current blocks or the new valid blocks could be
        // in the next set of current blocks.
        let mut potentially_current = vec![];
        potentially_current.extend(mem::replace(&mut self.current_blocks, btreeset!{}));
        potentially_current.extend(new_votes.iter().map(|&(ref vote, _)| vote.to.clone()));

        mem::replace(&mut self.current_blocks,
                     compute_current_blocks(potentially_current));
        //println!("{}: we have {} current blocks", self, self.current_blocks.len());
    }

    /// Update peer states for changes to the set of current blocks.
    pub fn update_peer_states(&mut self, step: u64) {
        let in_all = in_all_current(&self.current_blocks);
        let in_any = in_any_current(&self.current_blocks);
        let in_some = &in_any - &in_all;

        for name in in_all {
            self.peer_states.in_all_current(name, step);
        }

        for name in in_some {
            self.peer_states.in_some_current(name, step);
        }
    }

    /// Add a block to our local cache, and update our current and valid blocks.
    pub fn add_vote<I>(&mut self, vote: Vote, voted_for: I) -> Vec<Message>
        where I: IntoIterator<Item = Name>
    {
        // Add vote to cache.
        self.add_vote_to_cache(vote.clone(), voted_for);

        // Update valid and current blocks.
        let new_valid_votes = self.update_valid_blocks(&vote);

        let vote_agreed_msgs = new_valid_votes.into_iter().map(VoteAgreedMsg).collect();
        self.broadcast(vote_agreed_msgs)
    }

    /// Return all neighbours we're connected to (or should be connected to).
    // TODO: will need adjusting once we have multiple sections.
    pub fn neighbouring_nodes(&self) -> BTreeSet<Name> {
        let mut res: BTreeSet<_> = self.current_blocks
            .iter()
            .flat_map(|block| block.members.clone())
            .collect();
        res.extend(self.peer_states.all_peers());
        res.remove(&self.our_name);
        res
    }

    /// Create messages for every relevant neighbour for every vote in the given vec.
    pub fn broadcast(&self, msgs: Vec<MessageContent>) -> Vec<Message> {
        self.neighbouring_nodes()
            .into_iter()
            .flat_map(|neighbour| {
                msgs.iter()
                    .map(move |content| {
                             Message {
                                 sender: self.our_name,
                                 recipient: neighbour,
                                 content: content.clone(),
                             }
                         })
            })
            .collect()
    }

    /// Blocks that we can legitimately vote on successors for, because we are part of them.
    pub fn our_current_blocks<'a>(&'a self) -> Box<Iterator<Item = &'a Block> + 'a> {
        our_blocks(&self.current_blocks, self.our_name)
    }

    /// True if the given node could be added to the given block
    fn could_be_added(node: Name, block: &Block) -> bool {
        !block.members.contains(&node) && block.prefix.matches(node)
    }

    /// Construct new successor blocks based on our view of the network.
    pub fn construct_new_votes(&self, step: u64) -> Vec<Vote> {
        let mut votes = vec![];

        for node in self.peer_states.nodes_to_add(step) {
            for block in self.our_current_blocks() {
                if Self::could_be_added(node, block) {
                    println!("{}: voting to add {} to: {:?}", self, node, block);
                    votes.push(Vote {
                                   from: block.clone(),
                                   to: block.add_node(node),
                               });
                }
            }
        }

        for node in self.peer_states.nodes_to_drop(step) {
            for block in self.our_current_blocks() {
                if block.members.contains(&node) {
                    println!("{}: voting to remove {} from: {:?}", self, node, block);
                    votes.push(Vote {
                                   from: block.clone(),
                                   to: block.remove_node(node),
                               });
                }
            }
        }

        for vote in split_blocks(&self.current_blocks, self.our_name, self.min_split_size()) {
            println!("{}: voting to split from: {:?} to: {:?}",
                     self,
                     vote.from,
                     vote.to);
            votes.push(vote);
        }

        votes
    }

    pub fn broadcast_new_votes(&mut self, step: u64) -> Vec<Message> {
        let votes = self.construct_new_votes(step);
        let our_name = self.our_name;

        let mut to_broadcast = vec![];

        for vote in &votes {
            let agreed_msgs = self.add_vote(vote.clone(), Some(our_name));
            to_broadcast.extend(agreed_msgs);
        }

        // Construct vote messages and broadcast.
        let vote_msgs: Vec<_> = votes.into_iter().map(VoteMsg).collect();
        to_broadcast.extend(self.broadcast(vote_msgs));

        // TODO: does this belong here..?
        self.update_peer_states(step);

        to_broadcast
    }

    /// Create a message with all our votes to send to a new node.
    fn construct_bootstrap_msg(&self, joining_node: Name) -> Message {
        Message {
            sender: self.our_name,
            recipient: joining_node,
            content: BootstrapMsg(self.vote_counts.clone()),
        }
    }

    /// Apply a bootstrap message received from another node.
    fn apply_bootstrap_msg(&mut self, vote_counts: VoteCounts) -> Vec<Message> {
        let mut to_send = vec![];
        for (vote, voters) in vote_counts {
            let our_votes = self.add_vote(vote, voters);
            to_send.extend(our_votes);
        }
        to_send
    }

    /// Handle a message intended for us and return messages we'd like to send.
    pub fn handle_message(&mut self, message: Message, step: u64) -> Vec<Message> {
        let mut to_send = match message.content {
            NodeJoined => {
                let joining_node = message.sender;
                println!("{}: received join message for: {}", self, joining_node);

                // Mark the peer as having joined so that we vote to keep adding it.
                self.peer_states.node_joined(joining_node, step);

                // Broadcast new votes.
                let mut messages = self.broadcast_new_votes(step);
                messages.push(self.construct_bootstrap_msg(joining_node));
                messages
            }
            VoteMsg(vote) => {
                println!("{}: received {:?} from {}", self, vote, message.sender);
                let mut msgs = self.add_vote(vote, Some(message.sender));
                msgs.extend(self.broadcast_new_votes(step));
                msgs
            }
            VoteAgreedMsg((vote, voters)) => {
                println!("{}: received agreement for {:?} from {}",
                         self,
                         vote,
                         message.sender);
                let mut msgs = self.add_vote(vote, voters);
                msgs.extend(self.broadcast_new_votes(step));
                msgs
            }
            BootstrapMsg(vote_counts) => {
                println!("{}: applying bootstrap message from {}",
                         self,
                         message.sender);
                self.apply_bootstrap_msg(vote_counts)
            }
            ConnectionLost => {
                println!("{}: lost our connection to {}", self, message.sender);
                self.peer_states.disconnected(message.sender, step);
                self.broadcast_new_votes(step)
            }
            ConnectionRegained => {
                println!("{}: regained our connection to {}", self, message.sender);
                self.peer_states.reconnected(message.sender, step);
                self.broadcast_new_votes(step)
            }
        };

        to_send.retain(|msg| !self.message_filter.contains(msg));
        self.message_filter.extend(to_send.clone());
        to_send
    }
}
