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
    pub fn first(name: Name, active_peer_cutoff: u64) -> Self {
        Active(ActiveNode::first(name, active_peer_cutoff))
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
                       vote_counts: VoteCounts,
                       active_peer_cutoff: u64)
    {
        *self = Active(ActiveNode::new(
            name,
            valid_blocks,
            current_blocks,
            vote_counts,
            active_peer_cutoff
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
    /// Map from node name to step # when that node was last considered "active".
    /// For joining nodes, we consider them active from the point where they pass resource proof.
    pub active_peers: HashMap<Name, u64>,
    /// Time after which to give up on a peer.
    pub active_peer_cutoff: u64,
}

use std::fmt;
impl fmt::Display for ActiveNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Node({})", self.our_name)
    }
}

impl ActiveNode {
    pub fn first(name: Name, active_peer_cutoff: u64) -> Self {
        let genesis = Block::genesis(name);
        ActiveNode {
            our_name: name,
            valid_blocks: HashSet::from_iter(vec![genesis.clone()]),
            current_blocks: HashSet::from_iter(vec![genesis]),
            vote_counts: HashMap::new(),
            active_peers: HashMap::new(),
            active_peer_cutoff
        }
    }

    // TODO: need to pass in active peers I wonder?
    pub fn new(our_name: Name,
               valid_blocks: ValidBlocks,
               current_blocks: CurrentBlocks,
               vote_counts: VoteCounts,
               active_peer_cutoff: u64) -> Self {
        ActiveNode {
            our_name,
            valid_blocks,
            current_blocks,
            vote_counts,
            active_peers: HashMap::new(),
            active_peer_cutoff
        }
    }

    fn add_vote_to_cache(&mut self, vote: Vote, voted_for: Name) {
        let voters = self.vote_counts.entry(vote).or_insert_with(BTreeSet::new);
        voters.insert(voted_for);
    }

    /// Return all blocks that succeed the given block.
    fn successors<'a>(&'a self, from: &'a Block) -> Box<Iterator<Item=Block> + 'a> {
        // TODO: could be more efficient with look-up by `from` block.
        let iter = self.vote_counts.iter()
            .filter(move |&(vote, _)| {
                &vote.from == from
            })
            .filter(|&(vote, voters)| {
                is_quorum_of(voters, &vote.from.members)
            })
            .map(|(vote, _)| {
                vote.to.clone()
            });

        Box::new(iter)
    }

    /// Update valid blocks, return true if a new block became valid.
    fn update_valid_blocks(&mut self, vote: &Vote) -> bool {
        // Set of valid blocks to branch out from.
        let mut frontier = HashSet::new();

        if self.valid_blocks.contains(&vote.from) {
            frontier.insert(vote.from.clone());
        } else {
            return false;
        }

        let mut added_blocks = false;

        while !frontier.is_empty() {
            let mut new_frontier = HashSet::new();
            for block in frontier {
                if !self.valid_blocks.contains(&block) {
                    added_blocks = true;
                    println!("{}: new valid block: {:?}", self, block);
                    self.valid_blocks.insert(block.clone());
                }
                new_frontier.extend(self.successors(&block));
            }
            frontier = new_frontier;
        }

        added_blocks
    }

    /// If a new block has just become valid, update the current blocks.
    // FIXME: need to do transitive closure.
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
        let mut res: HashSet<_> = self.current_blocks.iter().flat_map(|block| block.members.clone()).collect();
        res.remove(&self.our_name);
        res
    }

    /// Create messages for every relevant neighbour for every vote in the given vec.
    pub fn broadcast_votes(&self, votes: Vec<Vote>) -> Vec<Message> {
        // Construct vote messages.
        let vote_msgs: Vec<_> = votes.into_iter().map(VoteMsg).collect();

        self.neighbouring_nodes().into_iter().flat_map(|neighbour| {
            vote_msgs.iter().map(move |vote_msg| {
                Message {
                    sender: self.our_name,
                    recipient: neighbour,
                    content: vote_msg.clone()
                }
            })
        }).collect()
    }

    /// Construct new successor blocks based on our view of the network.
    pub fn construct_new_votes(&self, step: u64) -> Vec<Vote> {
        let mut votes = vec![];
        for current_block in &self.current_blocks {
            for (peer, last_active) in &self.active_peers {
                // FIXME: >= or >?
                if *last_active >= step.saturating_sub(self.active_peer_cutoff) &&
                   !current_block.members.contains(peer) {
                    votes.push(Vote {
                        from: current_block.clone(),
                        to: current_block.add_node(*peer)
                    });
                }
            }
        }

        votes
    }

    pub fn broadcast_new_votes(&mut self, step: u64) -> Vec<Message> {
        // FIXME: Do we need to iterate to hit a fixed point here...?
        let votes = self.construct_new_votes(step);
        let our_name = self.our_name;

        for vote in &votes {
            self.add_vote(vote.clone(), our_name);
        }

        self.broadcast_votes(votes)
    }

    pub fn handle_message(&mut self, message: Message, step: u64) -> Vec<Message> {
        match message.content {
            NodeJoined => {
                let joining_node = message.sender;
                println!("{}: received join message for: {}", self, joining_node);

                // Add joining node to active peers so we keep voting to add them.
                self.active_peers.insert(joining_node, step);

                // Broadcast new votes.
                self.broadcast_new_votes(step)
            },
            VoteMsg(vote) => {
                self.add_vote(vote, message.sender);
                self.broadcast_new_votes(step)
            },
            //_ => unreachable!()
        }
    }
}

fn is_quorum_of(voters: &BTreeSet<Name>, members: &BTreeSet<Name>) -> bool {
    let valid_voters: HashSet<_> = voters.intersection(members).collect();
    //assert_eq!(voters.len(), valid_voters.len());

    valid_voters.len() * 2 > members.len()
}
