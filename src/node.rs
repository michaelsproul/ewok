use message::Message;
use message::MessageContent;
use message::MessageContent::*;
use name::Name;
use block::{Block, Vote};

use std::iter::FromIterator;
use std::collections::{BTreeMap, BTreeSet};

use self::Node::*;

pub type ValidBlocks = BTreeSet<Block>;
pub type CurrentBlocks = BTreeSet<Block>;
pub type VoteCounts = BTreeMap<Vote, BTreeSet<Name>>;

pub enum Node {
    WaitingToJoin,
    //Dead,
    Active(ActiveNode)
}

impl Node {
    pub fn first(name: Name, genesis: Block, active_peer_cutoff: u64) -> Self {
        Active(ActiveNode::new(name, genesis, active_peer_cutoff))
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
                       genesis: Block,
                       active_peer_cutoff: u64)
    {
        println!("Node({}): starting up!", name);
        *self = Active(ActiveNode::new(name, genesis, active_peer_cutoff));
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
    pub active_peers: BTreeMap<Name, u64>,
    /// Time after which to give up on a peer.
    pub active_peer_cutoff: u64,
    /// Filter for messages we've already sent and shouldn't resend.
    pub message_filter: BTreeSet<Message>
}

use std::fmt;
impl fmt::Display for ActiveNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Node({})", self.our_name)
    }
}

impl ActiveNode {
    pub fn new(name: Name, genesis: Block, active_peer_cutoff: u64) -> Self {
        ActiveNode {
            our_name: name,
            valid_blocks: BTreeSet::from_iter(vec![genesis.clone()]),
            current_blocks: BTreeSet::from_iter(vec![genesis]),
            vote_counts: BTreeMap::new(),
            active_peers: BTreeMap::new(),
            active_peer_cutoff,
            message_filter: BTreeSet::new()
        }
    }

    fn add_vote_to_cache<I>(&mut self, vote: Vote, voted_for: I)
        where I: IntoIterator<Item=Name>
    {
        let voters = self.vote_counts.entry(vote).or_insert_with(BTreeSet::new);
        voters.extend(voted_for);
    }

    /// Return all votes for blocks that succeed the given block.
    fn successors<'a>(&'a self, from: &'a Block) -> Box<Iterator<Item=(Vote, BTreeSet<Name>)> + 'a> {
        // TODO: could be more efficient with look-up by `from` block.
        let iter = self.vote_counts.iter()
            .filter(move |&(vote, _)| {
                &vote.from == from
            })
            .filter(|&(vote, voters)| {
                is_quorum_of(voters, &vote.from.members)
            })
            .map(|(vote, voters)| {
                (vote.clone(), voters.clone())
            });

        Box::new(iter)
    }

    /// Update valid blocks, return set of newly valid blocks to broadcast.
    fn update_valid_blocks(&mut self, vote: &Vote) -> Vec<(Vote, BTreeSet<Name>)> {
        // Set of valid blocks to branch out from.
        // Stored as a set of votes where the frontier blocks are the "to" component,
        // and the nodes that voted for them are held alongside (a little hacky).
        let mut frontier: BTreeSet<(Vote, BTreeSet<Name>)> = BTreeSet::new();

        if self.valid_blocks.contains(&vote.from) {
            // This is a nasty hack...
            // Should maybe use a Graph type from `petgraph`?
            let init_vote = Vote { from: vote.from.clone(), to: vote.from.clone() };
            frontier.insert((init_vote, BTreeSet::new()));
        } else {
            return vec![];
        }

        // Set of new valid votes to broadcast.
        let mut new_valid_votes = vec![];

        while !frontier.is_empty() {
            let mut new_frontier = BTreeSet::new();

            for (vote, voters) in frontier {

                if !self.valid_blocks.contains(&vote.to) {
                    println!("{}: new valid block: {:?}", self, vote.to);
                    self.valid_blocks.insert(vote.to.clone());
                    new_valid_votes.push((vote.clone(), voters));

                    // Update current blocks.
                    // FIXME: need to also remove superseded sibling blocks.
                    if self.current_blocks.remove(&vote.from) {
                        println!("{}: block no longer current: {:?}", self, vote.from);
                    }
                    println!("{}: new current block: {:?}", self, vote.to);
                    self.current_blocks.insert(vote.to.clone());
                }

                new_frontier.extend(self.successors(&vote.to));
            }

            frontier = new_frontier;
        }

        new_valid_votes
    }

    /// Add a block to our local cache, and update our current and valid blocks.
    pub fn add_vote<I>(&mut self, vote: Vote, voted_for: I) -> Vec<Message>
        where I: IntoIterator<Item=Name>
    {
        // Add vote to cache.
        self.add_vote_to_cache(vote.clone(), voted_for);

        // Update valid and current blocks.
        let new_valid_votes = self.update_valid_blocks(&vote);

        let vote_agreed_msgs = new_valid_votes.into_iter().map(VoteAgreedMsg).collect();
        self.broadcast(vote_agreed_msgs)
    }

    /// Return all neighbours we're connected to (or should be connected to).
    pub fn neighbouring_nodes(&self) -> BTreeSet<Name> {
        // TODO: filter this better
        let mut res: BTreeSet<_> = self.current_blocks.iter().flat_map(|block| block.members.clone()).collect();
        res.remove(&self.our_name);
        res.extend(self.active_peers.keys().map(|&name| name));
        res
    }

    /// Create messages for every relevant neighbour for every vote in the given vec.
    pub fn broadcast(&self, msgs: Vec<MessageContent>) -> Vec<Message> {
        self.neighbouring_nodes().into_iter().flat_map(|neighbour| {
            msgs.iter().map(move |content| {
                Message {
                    sender: self.our_name,
                    recipient: neighbour,
                    content: content.clone()
                }
            })
        }).collect()
    }

    /// Construct new successor blocks based on our view of the network.
    pub fn construct_new_votes(&self, step: u64) -> Vec<Vote> {
        let mut votes = vec![];
        for current_block in &self.current_blocks {
            if !current_block.members.contains(&self.our_name) {
                continue;
            }

            for (peer, last_active) in &self.active_peers {
                // FIXME: >= or >?
                if *last_active >= step.saturating_sub(self.active_peer_cutoff) &&
                   !current_block.members.contains(peer) {
                    println!("{}: peer {} is missing from current_block: {:?}", self, peer, current_block);
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

        // FIXME: should probably pass a "message sender" around for this...
        let mut to_broadcast = vec![];

        for vote in &votes {
            println!("{}: voting for {:?} based on our view", self, vote);
            let agreed_msgs = self.add_vote(vote.clone(), Some(our_name));
            to_broadcast.extend(agreed_msgs);
        }

        // Construct vote messages and broadcast.
        let vote_msgs: Vec<_> = votes.into_iter().map(VoteMsg).collect();
        to_broadcast.extend(self.broadcast(vote_msgs));

        to_broadcast
    }

    /// Create a message with all our votes to send to a new node.
    fn construct_bootstrap_msg(&self, joining_node: Name) -> Message {
        Message {
            sender: self.our_name,
            recipient: joining_node,
            content: BootstrapMsg(self.vote_counts.clone())
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

    pub fn handle_message(&mut self, message: Message, step: u64) -> Vec<Message> {
        let mut to_send = match message.content {
            NodeJoined => {
                let joining_node = message.sender;
                println!("{}: received join message for: {}", self, joining_node);

                // Add joining node to active peers so we keep voting to add them.
                self.active_peers.insert(joining_node, step);

                // Broadcast new votes.
                let mut messages = self.broadcast_new_votes(step);
                messages.push(self.construct_bootstrap_msg(joining_node));
                messages
            },
            VoteMsg(vote) => {
                println!("{}: received {:?} from {}", self, vote, message.sender);
                let mut msgs = self.add_vote(vote, Some(message.sender));
                msgs.extend(self.broadcast_new_votes(step));
                msgs
            },
            VoteAgreedMsg((vote, voters)) => {
                println!("{}: received agreement for {:?} from {}", self, vote, message.sender);
                let mut msgs = self.add_vote(vote, voters);
                msgs.extend(self.broadcast_new_votes(step));
                msgs
            },
            BootstrapMsg(vote_counts) => {
                println!("{}: applying bootstrap message from {}", self, message.sender);
                self.apply_bootstrap_msg(vote_counts)
            }
        };

        to_send.retain(|msg| !self.message_filter.contains(msg));
        self.message_filter.extend(to_send.clone());
        to_send
    }
}

fn is_quorum_of(voters: &BTreeSet<Name>, members: &BTreeSet<Name>) -> bool {
    let valid_voters: BTreeSet<_> = voters.intersection(members).collect();
    //assert_eq!(voters.len(), valid_voters.len());
    valid_voters.len() * 2 > members.len()
}
