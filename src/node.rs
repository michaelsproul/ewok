use message::Message;
use message::MessageContent;
use message::MessageContent::*;
use name::Name;
use block::{Block, Vote, ValidBlocks, CurrentBlocks, VoteCounts, new_valid_blocks,
            compute_current_blocks, compute_current_candidate_blocks, our_blocks, section_blocks,
            chain_segment};
use params::NodeParams;
use split::split_blocks;
use merge::merge_blocks;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem;
use std::fmt;
use std::rc::Rc;
use itertools::Itertools;

const MESSAGE_FILTER_LEN: usize = 1024;

pub struct Node {
    /// Our node's name.
    pub our_name: Name,
    /// All valid blocks.
    pub valid_blocks: ValidBlocks,
    /// Our current candidates for current blocks.
    pub current_candidate_blocks: ValidBlocks,
    /// Our current blocks.
    pub current_blocks: CurrentBlocks,
    /// Our previous current blocks.
    pub prev_current_blocks: CurrentBlocks,
    /// Map from blocks to voters for that block.
    pub vote_counts: VoteCounts,
    /// Recently received votes that haven't yet been applied to the sets of valid and current
    /// blocks.
    pub recent_votes: BTreeSet<Vote>,
    /// Peers that we're currently connected to.
    pub connections: BTreeSet<Name>,
    /// Nodes that we've sent connection requests to.
    pub connect_requests: BTreeSet<Name>,
    /// Candidates who we are waiting to add to our current blocks.
    pub candidates: BTreeMap<Name, Candidate>,
    /// Filter for hashes of recent messages we've already sent and shouldn't resend.
    pub message_filter: VecDeque<u64>,
    /// Network configuration parameters.
    pub params: NodeParams,
    /// Step that this node was created.
    pub step_created: u64,
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Node({})", self.our_name)
    }
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f,
               "Node({}): {} valid blocks;   {} vote counts with max \"to\" blocks of {:?};   {} \
               current blocks: {:#?}",
               self.our_name,
               self.valid_blocks.len(),
               self.vote_counts.len(),
               self.vote_counts.values().map(BTreeMap::len).max(),
               self.current_blocks.len(),
               self.current_blocks)
    }
}

pub struct Candidate {
    step_added: u64
}

impl Candidate {
    fn is_recent(&self, join_timeout: u64, step: u64) -> bool {
        self.step_added + join_timeout >= step
    }
}

/// Compute the set of nodes that are in any current block.
pub fn nodes_in_any(blocks: &BTreeSet<Rc<Block>>) -> BTreeSet<Name> {
    blocks
        .iter()
        .fold(BTreeSet::new(), |acc, block| &acc | &block.members)
}

impl Node {
    /// Create a new node which starts from a given set of valid and current blocks.
    pub fn new(name: Name, current_blocks: CurrentBlocks, params: NodeParams, step: u64) -> Self {
        // FIXME: prune connections
        let connections = nodes_in_any(&current_blocks);

        Node {
            our_name: name,
            valid_blocks: current_blocks.clone(),
            current_blocks: current_blocks.clone(),
            prev_current_blocks: BTreeSet::new(),
            current_candidate_blocks: current_blocks,
            connections,
            connect_requests: BTreeSet::new(),
            candidates: BTreeMap::new(),
            vote_counts: BTreeMap::new(),
            recent_votes: BTreeSet::new(),
            message_filter: VecDeque::with_capacity(MESSAGE_FILTER_LEN),
            params,
            step_created: step,
        }
    }

    /// Minimum size that all sections must be before splitting.
    fn min_split_size(&self) -> usize {
        self.params.min_section_size + self.params.split_buffer
    }

    /// Insert a vote into our local cache of votes.
    fn add_vote<I>(&mut self, vote: Vote, voted_for: I)
        where I: IntoIterator<Item = Name>
    {
        self.recent_votes.insert(vote.clone());
        let voters = self.vote_counts
            .entry(vote.from)
            .or_insert_with(BTreeMap::new)
            .entry(vote.to)
            .or_insert_with(BTreeSet::new);
        voters.extend(voted_for);
    }

    /// Update valid and current block sets, return set of newly valid blocks to broadcast,
    /// and merge messages to broadcast.
    fn update_valid_blocks(&mut self) -> Vec<(Vote, BTreeSet<Name>)> {
        // Update valid blocks.
        let new_votes = mem::replace(&mut self.recent_votes, btreeset!{});
        let new_valid_votes = new_valid_blocks(&self.valid_blocks, &self.vote_counts, new_votes);
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
        let mut potentially_current = btreeset!{};
        potentially_current.extend(mem::replace(&mut self.current_candidate_blocks, btreeset!{}));
        potentially_current.extend(new_votes.iter().map(|&(ref vote, _)| vote.to.clone()));

        mem::replace(&mut self.current_candidate_blocks,
                     compute_current_candidate_blocks(potentially_current));

        self.prev_current_blocks = mem::replace(
            &mut self.current_blocks,
            compute_current_blocks(&self.current_candidate_blocks)
        );
    }

    /// Construct messages to send because of a merge in our own section, if one has occurred.
    ///
    /// Currently this sends the full histories of the _descendants of our sibling prefix_
    /// to any of our neighbours that they would previously have been disconnected from.
    /// E.g. we send history of 110 and 111 to 00 if we are 01 and merging with 00 into 0.
    // FIXME(michael): break this function into smaller readable + testable parts.
    fn merge_messages(&self) -> Vec<Message> {
        let mut messages = vec![];
        let new_current_blocks = &self.current_blocks - &self.prev_current_blocks;

        for our_new_block in section_blocks(&new_current_blocks, self.our_name) {
            for our_prev_block in section_blocks(&self.prev_current_blocks, self.our_name) {
                // Merge case.
                if let Some(sibling_pfx) = our_prev_block.prefix.sibling() {
                    if our_new_block.prefix == our_prev_block.prefix.popped() {
                        // In the case of a merge, send our sibling's history to all the
                        // sections we are connected to but they are not.
                        // We need to send history for all descendants of our sibling prefix,
                        // because our sibling may be split.
                        let sibling_blocks = self.prev_current_blocks
                            .iter()
                            .filter(|block| sibling_pfx.is_prefix_of(&block.prefix));

                        let disconn_from_sibling = self.prev_current_blocks
                            .iter()
                            .filter(|block| {
                                block.prefix != sibling_pfx &&
                                !block.prefix.is_neighbour(&sibling_pfx)
                            })
                            .inspect(|block| {
                                trace!("{}: updating {:?} on history of {:?} because of merge",
                                       self,
                                       block.prefix,
                                       sibling_pfx);
                            })
                            .flat_map(|block| block.members.iter().cloned())
                            .collect_vec();

                        // If there are no disconnected neighbour sections, don't try to compute
                        // the chain segment.
                        if disconn_from_sibling.is_empty() {
                            continue;
                        }

                        // Union of all chain segments for sibling block history.
                        let segments: BTreeSet<_> = sibling_blocks.flat_map(|sibling_block| {
                            chain_segment(sibling_block, &self.vote_counts)
                        }).collect();

                        let new_messages = disconn_from_sibling.into_iter()
                            .map(|neighbour| {
                                Message {
                                    sender: self.our_name,
                                    recipient: neighbour,
                                    content: VoteBundle(segments.clone()),
                                }
                            });
                        messages.extend(new_messages);
                    }
                }
            }
        }

        messages
    }

    /// Drop blocks for sections that we aren't neighbours of.
    fn prune_split_blocks(&mut self) {
        let all_current_blocks = mem::replace(&mut self.current_blocks, btreeset!{});

        let our_prefix = section_blocks(&all_current_blocks, self.our_name)
            .next()
            .unwrap()
            .prefix;

        for block in all_current_blocks {
            if block.prefix.is_neighbour(&our_prefix) || block.prefix == our_prefix {
                self.current_blocks.insert(block);
            }
        }
    }

    fn is_candidate(&self, name: &Name, step: u64) -> bool {
        self.candidates
            .get(name)
            .map(|candidate| {
                candidate.is_recent(self.params.join_timeout, step) &&
                self.connections.contains(name)
            })
            .unwrap_or(false)
    }

    /// Get connection and disconnection messages for peers.
    fn connects_and_disconnects(&mut self, step: u64) -> Vec<Message> {
        let neighbours = nodes_in_any(&self.current_blocks);
        let our_name = self.our_name;

        // FIXME: put this somewhere else?
        for node in &neighbours {
            self.candidates.remove(node);
        }

        let to_disconnect: BTreeSet<Name> = {
            self.connections
                .iter()
                .filter(|name| !neighbours.contains(&name) && !self.is_candidate(&name, step))
                .cloned()
                .collect()
        };

        for node in &to_disconnect {
            trace!("{}: disconnecting from {}", self, node);
            self.connections.remove(node);
        }

        let disconnects = to_disconnect.into_iter()
            .map(|neighbour| {
                Message {
                    sender: our_name,
                    recipient: neighbour,
                    content: MessageContent::Disconnect,
                }
            });

        let to_connect: BTreeSet<Name> = {
            neighbours.iter()
                .filter(|name| {
                    !self.connections.contains(&name) && !self.connect_requests.contains(&name)
                })
                .cloned()
                .collect()
        };

        for node in &to_connect {
            trace!("{}: connecting to {}", self, node);
            self.connect_requests.insert(*node);
        }

        let connects = to_connect.into_iter()
            .map(|neighbour| {
                Message {
                    sender: our_name,
                    recipient: neighbour,
                    content: MessageContent::Connect,
                }
            });

        connects.chain(disconnects).collect()
    }

    /// Called once per step.
    pub fn update_state(&mut self, step: u64) -> Vec<Message> {
        // Update valid and current blocks.
        let new_valid_votes = self.update_valid_blocks();

        // Broadcast vote agreement messages before pruning the current block set.
        let our_name = self.our_name;
        let mut messages = self.broadcast(
            new_valid_votes.into_iter()
                .filter(|&(ref vote, _)| vote.from.members.contains(&our_name))
                .map(VoteAgreedMsg)
                .collect(),
            step
        );

        // Prune blocks that are no longer relevant because of splitting.
        self.prune_split_blocks();

        // Generate connect and disconnect messages.
        messages.extend(self.connects_and_disconnects(step));

        // Generate messages related to merging.
        messages.extend(self.merge_messages());

        messages
    }

    /// Create messages for every relevant neighbour for every vote in the given vec.
    pub fn broadcast(&self, msgs: Vec<MessageContent>, step: u64) -> Vec<Message> {
        msgs.into_iter()
            .flat_map(move |content| {
                let mut recipients = content.recipients(&self.current_blocks);
                recipients.extend(self.nodes_to_add(step));
                recipients.remove(&self.our_name);

                recipients.into_iter().map(move |recipient| {
                    Message {
                        sender: self.our_name,
                        recipient,
                        content: content.clone()
                    }
                })
            })
            .collect()
    }

    /// Check we don't have excessive valid blocks for any given (prefix, version) pair.
    pub fn check_conflicting_block_count(&self) {
        let mut conflicting_counts = BTreeMap::new();
        for block in &self.valid_blocks {
            let count = conflicting_counts
                .entry((block.prefix, block.version))
                .or_insert(0);
            *count += 1;
            if *count == self.params.max_conflicting_blocks {
                panic!("{:?}\nhas {} valid blocks for {:?} with version {}.",
                       self,
                       count,
                       block.prefix,
                       block.version);
            }
        }
    }

    /// Blocks that we can legitimately vote on successors for, because we are part of them.
    pub fn our_current_blocks<'a>(&'a self) -> Box<Iterator<Item = &'a Rc<Block>> + 'a> {
        our_blocks(&self.current_blocks, self.our_name)
    }

    /// Get all blocks for our current section(s),
    ///
    /// i.e. all the blocks whose prefix matches `name`.
    pub fn our_current_section_blocks<'a>(&'a self) -> Box<Iterator<Item = &'a Rc<Block>> + 'a> {
        section_blocks(&self.current_blocks, self.our_name)
    }

    /// True if the given node could be added to the given block
    fn could_be_added(node: Name, block: &Block) -> bool {
        !block.members.contains(&node) && block.prefix.matches(node)
    }

    fn nodes_to_add(&self, step: u64) -> Vec<Name> {
        self.candidates
            .iter()
            .filter(|&(name, candidate)| {
                self.connections.contains(name) &&
                candidate.is_recent(self.params.join_timeout, step)
            })
            .map(|(name, _)| *name)
            .collect()
    }

    fn nodes_to_drop(&self, current_block: &Rc<Block>) -> Vec<Name> {
        current_block.members
            .iter()
            .filter(|peer| {
                **peer != self.our_name &&
                !self.connections.contains(peer) &&
                !self.candidates.contains_key(peer)
            })
            .cloned()
            .collect()
    }

    /// Construct new successor blocks based on our view of the network.
    pub fn construct_new_votes(&self, step: u64) -> Vec<Vote> {
        let mut votes = vec![];

        for block in self.our_current_blocks() {
            for node in self.nodes_to_add(step) {
                if Self::could_be_added(node, block) {
                    trace!("{}: voting to add {} to: {:?}", self, node, block);
                    votes.push(Vote {
                                   from: block.clone(),
                                   to: block.add_node(node),
                               });
                }
            }
        }

        for block in self.our_current_blocks() {
            for node in self.nodes_to_drop(&block) {
                trace!("{}: voting to remove {} from: {:?}", self, node, block);
                votes.push(Vote {
                               from: block.clone(),
                               to: block.remove_node(node),
                           });
            }
        }

        for vote in split_blocks(&self.current_blocks, self.our_name, self.min_split_size()) {
            trace!("{}: voting to split from: {:?} to: {:?}",
                   self,
                   vote.from,
                   vote.to);
            votes.push(vote);
        }

        for vote in merge_blocks(&self.current_blocks,
                                 self.our_name,
                                 self.params.min_section_size) {
            trace!("{}: voting to merge from: {:?} to: {:?}",
                   self,
                   vote.from,
                   vote.to);
            votes.push(vote);
        }

        votes
    }

    /// Returns new votes to be broadcast after filtering them.
    pub fn broadcast_new_votes(&mut self, step: u64) -> Vec<Message> {
        let votes = self.construct_new_votes(step);
        let our_name = self.our_name;

        let mut to_broadcast = vec![];

        for vote in &votes {
            self.add_vote(vote.clone(), Some(our_name));
        }

        // Construct vote messages and broadcast.
        let vote_msgs: Vec<_> = votes.into_iter().map(VoteMsg).collect();
        to_broadcast.extend(self.broadcast(vote_msgs, step));

        self.filter_messages(to_broadcast)
    }

    /// Remove messages that have already been sent from `messages`, and update the filter.
    fn filter_messages(&mut self, messages: Vec<Message>) -> Vec<Message> {
        let mut filtered = vec![];
        for message in messages {
            let mut hasher = DefaultHasher::new();
            message.hash(&mut hasher);
            let hash = hasher.finish();
            if !self.message_filter.contains(&hash) {
                filtered.push(message);
                if self.message_filter.len() == MESSAGE_FILTER_LEN {
                    let _ = self.message_filter.pop_front();
                }
                self.message_filter.push_back(hash);
            }
        }
        filtered
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
    fn apply_bootstrap_msg(&mut self, vote_counts: VoteCounts) {
        for (from, map) in vote_counts {
            for (to, voters) in map {
                let vote = Vote {
                    from: from.clone(),
                    to,
                };
                self.add_vote(vote, voters);
            }
        }
    }

    /// Returns true if the peer is known and its state is `Disconnected`.
    pub fn is_disconnected_from(&self, name: &Name) -> bool {
        !self.connections.contains(name)
    }

    /// Returns true if this node should shutdown because it has failed to join a section.
    pub fn should_shutdown(&self, step: u64) -> bool {
        let timeout_elapsed = step >= self.step_created + self.params.self_shutdown_timeout;
        let no_blocks = self.our_current_blocks().count() == 0;
        let insufficient_connections = self.connections.len() <= 2;

        timeout_elapsed && (no_blocks || insufficient_connections)
    }

    pub fn step_created(&self) -> u64 {
        self.step_created
    }

    /// Handle a message intended for us and return messages we'd like to send.
    pub fn handle_message(&mut self, message: Message, step: u64) -> Vec<Message> {
        let to_send = match message.content {
            NodeJoined => {
                let joining_node = message.sender;
                debug!("{}: received join message for: {}", self, joining_node);

                // Mark the peer as having joined so that we vote to keep adding it.
                self.candidates.insert(joining_node, Candidate { step_added: step });
                self.connections.insert(joining_node);

                let connect_msg = Message {
                    sender: self.our_name,
                    recipient: joining_node,
                    content: Connect,
                };

                // Send a bootstrap message to the joining node.
                vec![connect_msg, self.construct_bootstrap_msg(joining_node)]
            }
            VoteMsg(vote) => {
                debug!("{}: received {:?} from {}", self, vote, message.sender);
                self.add_vote(vote, Some(message.sender));
                vec![]
            }
            VoteAgreedMsg((vote, voters)) => {
                debug!("{}: received agreement for {:?} from {}",
                       self,
                       vote,
                       message.sender);
                self.add_vote(vote, voters);
                vec![]
            }
            VoteBundle(bundle) => {
                debug!("{}: received a vote bundle from {}", self, message.sender);
                for (vote, voters) in bundle {
                    self.add_vote(vote, voters);
                }
                vec![]
            }
            BootstrapMsg(vote_counts) => {
                debug!("{}: applying bootstrap message from {}",
                       self,
                       message.sender);
                self.apply_bootstrap_msg(vote_counts);
                vec![]
            }
            Disconnect => {
                debug!("{}: lost our connection to {}", self, message.sender);
                self.connections.remove(&message.sender);
                vec![]
            }
            Connect => {
                if self.connections.insert(message.sender) {
                    debug!("{}: obtained a connection to {}", self, message.sender);
                }
                if !self.connect_requests.contains(&message.sender) {
                    trace!("{}: connecting back to {}", self, message.sender);
                    self.connect_requests.insert(message.sender);
                    vec![Message {
                        sender: self.our_name,
                        recipient: message.sender,
                        content: MessageContent::Connect,
                    }]
                } else {
                    vec![]
                }
            }
        };

        self.filter_messages(to_send)
    }
}
