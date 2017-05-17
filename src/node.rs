use message::Message;
use message::MessageContent::*;
use name::Name;
use block::{Block, Vote};

use std::iter::FromIterator;
use std::collections::{HashMap, HashSet, BTreeSet};

use self::Node::*;

pub type ValidBlocks = HashSet<Block>;
pub type CurrentBlocks = HashSet<Block>;
pub type VoteCounts = HashMap<Vote, BTreeSet<Name>>;

pub enum Node {
    WaitingToJoin,
    //Dead,
    Active(ActiveNode)
}

impl Node {
    pub fn first(name: Name) -> Self {
        Active(ActiveNode::first(name))
    }

    pub fn joining() -> Self {
        WaitingToJoin
    }

    pub fn is_joining(&self) -> bool {
        match *self {
            WaitingToJoin => true,
            _ => false
        }
    }

    pub fn is_active(&self) -> bool {
        match *self {
            Active(..) => true,
            _ => false
        }
    }

    pub fn make_active(&mut self,
                       name: Name,
                       valid_blocks: ValidBlocks,
                       current_blocks: CurrentBlocks,
                       vote_counts: VoteCounts)
    {
        *self = Active(ActiveNode::new(
            name,
            valid_blocks,
            current_blocks,
            vote_counts
        ));
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
    // votes: successor -> (predecessor -> set of voters)
    pub vote_counts: VoteCounts,
}

use std::fmt;
impl fmt::Display for ActiveNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Node({})", self.our_name)
    }
}

impl ActiveNode {
    pub fn first(name: Name) -> Self {
        let genesis = Block::genesis(name);
        ActiveNode {
            our_name: name,
            valid_blocks: HashSet::from_iter(vec![genesis.clone()]),
            current_blocks: HashSet::from_iter(vec![genesis]),
            vote_counts: HashMap::new()
        }
    }

    pub fn new(our_name: Name,
               valid_blocks: ValidBlocks,
               current_blocks: CurrentBlocks,
               vote_counts: VoteCounts) -> Self {
        ActiveNode {
            our_name,
            valid_blocks,
            current_blocks,
            vote_counts
        }
    }

    fn add_vote_to_cache(&mut self, vote: Vote, voted_for: Name) {
        let voters = self.vote_counts.entry(vote).or_insert_with(BTreeSet::new);
        voters.insert(voted_for);
    }

    /// Update valid blocks, return true if a new block became valid.
    fn update_valid_blocks(&mut self, vote: &Vote) -> bool {
        // Check if the "to" block is now valid.
        // 1. Check if predecessor is valid.
        if !self.valid_blocks.contains(&vote.from) {
            return false;
        }
        // 2. Check that votes for edge are quorum of predecessor.members.
        let votes = &self.vote_counts[vote];
        if !is_quorum_of(votes, &vote.from.members) {
            return false;
        }

        // We're good! Add to valid blocks.
        println!("{}: new valid block {:?}", self, vote.to);
        self.valid_blocks.insert(vote.to.clone());

        true
    }

    /// If a new block has just become valid, update the current blocks.
    fn update_current_blocks(&mut self, vote: &Vote) {
        // Re-compute current blocks by checking if our new block is admissable after
        // any current block, and if it is, removing all those blocks that it is admissable after.
        let mut add = false;
        let mut new_current_blocks = HashSet::new();

        for block in self.current_blocks.drain() {
            if vote.to.is_admissable_after(&block) {
                add = true;
                println!("evicting old block: {:?}", block);
                drop(block);
            } else {
                new_current_blocks.insert(block);
            }
        }

        if add {
            println!("{}: new current block: {:?}", self, vote.to);
            new_current_blocks.insert(vote.to.clone());
        }

        self.current_blocks = new_current_blocks;
    }

    /// Add a block to our local cache, and update our current and valid blocks.
    pub fn add_vote(&mut self, vote: Vote, voted_for: Name) {
        // Add vote to cache.
        println!("{}: received {:?} from {}", self, vote, voted_for);
        self.add_vote_to_cache(vote.clone(), voted_for);

        // Update valid blocks.
        if self.update_valid_blocks(&vote) {
            self.update_current_blocks(&vote);
        }

        // TODO: Send new votes?
    }

    /// Return all neighbours we're connected to (or should be connected to).
    pub fn neighbouring_nodes(&self) -> HashSet<Name> {
        // TODO: filter this better
        self.current_blocks.iter().flat_map(|block| block.members.clone()).collect()
    }

    pub fn handle_message(&mut self, message: Message) -> Vec<Message> {
        let our_name = message.recipient;

        match message.content {
            NodeJoined => {
                println!("{}: received join message for: {}", self, message.sender);

                let joining_node = message.sender;

                // Construct votes for successor blocks.
                let votes: Vec<_> = self.current_blocks.iter()
                    .map(|block| {
                        Vote { from: block.clone(), to: block.add_node(joining_node) }
                    })
                    .collect();

                // Add votes to our local cache.
                for vote in &votes {
                    self.add_vote(vote.clone(), our_name);
                }

                // Construct votes for this new block.
                let vote_msgs: Vec<_> = votes.into_iter().map(VoteMsg).collect();

                // Send messages to all relevant neighbours.
                // TODO: make this a broadcast function.
                let mut messages = vec![];
                for neighbour in self.neighbouring_nodes() {
                    for content in &vote_msgs {
                        messages.push(Message {
                            sender: our_name,
                            recipient: neighbour,
                            content: content.clone()
                        });
                    }
                }

                messages
            },
            VoteMsg(vote) => {
                self.add_vote(vote, message.sender);
                // TODO: propose changes based on our local view of the network.
                vec![]
            },
            _ => unreachable!()
        }
    }
}

fn is_quorum_of(voters: &BTreeSet<Name>, members: &BTreeSet<Name>) -> bool {
    let valid_voters: HashSet<_> = voters.intersection(members).collect();
    //assert_eq!(voters.len(), valid_voters.len());

    valid_voters.len() * 2 > members.len()
}
