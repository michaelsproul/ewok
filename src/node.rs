use message::Message;
use message::MessageContent;
use message::MessageContent::*;
use name::Name;
use block::{Block, BlockId, Vote};
use blocks::{Blocks, VoteCounts, ValidBlocks, CurrentBlocks};
use params::NodeParams;
use split::split_blocks;
use merge::merge_blocks;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem;
use std::fmt;

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
    /// Reverse map from blocks to voters (to -> from -> voters)
    pub rev_vote_counts: VoteCounts,
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

pub struct Candidate {
    step_added: u64,
}

impl Candidate {
    fn is_recent(&self, join_timeout: u64, step: u64) -> bool {
        self.step_added + join_timeout >= step
    }
}

/// Compute the set of nodes that are in any current block.
pub fn nodes_in_any(all_blocks: &Blocks, blocks: &BTreeSet<BlockId>) -> BTreeSet<Name> {
    all_blocks
        .block_contents(blocks.into_iter().cloned())
        .into_iter()
        .fold(BTreeSet::new(), |acc, block| &acc | &block.members)
}

impl Node {
    /// Create a new node which starts from a given set of valid and current blocks.
    pub fn new(
        name: Name,
        blocks: &Blocks,
        current_blocks: CurrentBlocks,
        params: NodeParams,
        step: u64,
    ) -> Self {
        // FIXME: prune connections
        let connections = nodes_in_any(blocks, &current_blocks);

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
            rev_vote_counts: BTreeMap::new(),
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
    where
        I: IntoIterator<Item = Name>,
    {
        self.recent_votes.insert(vote.clone());
        let voters = self.vote_counts
            .entry(vote.from)
            .or_insert_with(BTreeMap::new)
            .entry(vote.to)
            .or_insert_with(BTreeSet::new);
        voters.extend(voted_for);
        let rev_voters = self.rev_vote_counts
            .entry(vote.to)
            .or_insert_with(BTreeMap::new)
            .entry(vote.from)
            .or_insert_with(BTreeSet::new);
        rev_voters.extend(voters.clone());
    }

    /// Update valid and current block sets, return set of newly valid blocks to broadcast,
    /// and merge messages to broadcast.
    fn update_valid_blocks(&mut self, blocks: &Blocks) -> BTreeSet<(Vote, BTreeSet<Name>)> {
        // Update valid blocks.
        let new_votes = mem::replace(&mut self.recent_votes, btreeset!{});
        let new_valid_votes =
            blocks.new_valid_blocks(&self.valid_blocks, &self.vote_counts, new_votes);
        self.valid_blocks.extend(new_valid_votes.iter().map(
            |&(ref vote, _)| {
                vote.to.clone()
            },
        ));

        // Update current blocks.
        self.update_current_blocks(blocks, &new_valid_votes);

        new_valid_votes
    }

    /// Update the set of current blocks.
    fn update_current_blocks(&mut self, blocks: &Blocks, new_votes: &BTreeSet<(Vote, BTreeSet<Name>)>) {
        // Any of the existing current blocks or the new valid blocks could be
        // in the next set of current blocks.
        let mut potentially_current = btreeset!{};
        potentially_current.extend(mem::replace(
            &mut self.current_candidate_blocks,
            btreeset!{},
        ));
        potentially_current.extend(new_votes.iter().map(|&(ref vote, _)| vote.to.clone()));

        mem::replace(
            &mut self.current_candidate_blocks,
            blocks.compute_current_candidate_blocks(potentially_current),
        );

        self.prev_current_blocks = mem::replace(
            &mut self.current_blocks,
            blocks.compute_current_blocks(&self.current_candidate_blocks),
        );
    }

    /// Drop blocks for sections that we aren't neighbours of.
    fn prune_split_blocks(&mut self, blocks: &Blocks) {
        let all_current_blocks = mem::replace(&mut self.current_blocks, btreeset!{});

        let our_prefix = blocks.section_blocks(&all_current_blocks, self.our_name)[0].prefix;

        for block in blocks.block_contents(all_current_blocks) {
            if block.prefix.is_neighbour(&our_prefix) || block.prefix == our_prefix {
                self.current_blocks.insert(block.get_id());
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
    fn connects_and_disconnects(&mut self, blocks: &Blocks, step: u64) -> Vec<Message> {
        let neighbours = nodes_in_any(blocks, &self.current_blocks);
        let our_name = self.our_name;

        // FIXME: put this somewhere else?
        for node in &neighbours {
            self.candidates.remove(node);
        }

        let to_disconnect: BTreeSet<Name> = {
            self.connections
                .iter()
                .filter(|name| {
                    !neighbours.contains(&name) && !self.is_candidate(&name, step)
                })
                .cloned()
                .collect()
        };

        for node in &to_disconnect {
            trace!("{}: disconnecting from {}", self, node);
            self.connections.remove(node);
            self.connect_requests.remove(node);
        }

        let disconnects = to_disconnect.into_iter().map(|neighbour| {
            Message {
                sender: our_name,
                recipient: neighbour,
                content: MessageContent::Disconnect,
            }
        });

        let to_connect: BTreeSet<Name> = {
            neighbours
                .iter()
                .filter(|name| {
                    !self.connections.contains(name) && !self.connect_requests.contains(name) &&
                    **name != our_name
                })
                .cloned()
                .collect()
        };

        for node in &to_connect {
            trace!("{}: connecting to {}", self, node);
            self.connect_requests.insert(*node);
        }

        let connects = to_connect.into_iter().map(|neighbour| {
            Message {
                sender: our_name,
                recipient: neighbour,
                content: MessageContent::Connect,
            }
        });

        connects.chain(disconnects).collect()
    }

    /// Called once per step.
    pub fn update_state(&mut self, blocks: &mut Blocks, step: u64) -> Vec<Message> {
        // Update valid and current blocks.
        let new_valid_votes = self.update_valid_blocks(blocks);

        // Broadcast vote agreement messages before pruning the current block set.
        let mut messages = self.broadcast(
            blocks,
            new_valid_votes
                .into_iter()
                .inspect(|&(ref vote, _)| {
                    debug!(
                        "{}: new valid vote: {:?}",
                        self,
                        vote.as_debug(blocks),
                    );
                })
                .filter(|&(ref vote, _)| !vote.is_witnessing(blocks))
                .map(VoteAgreedMsg)
                .collect(),
            step,
        );

        // Prune blocks that are no longer relevant because of splitting.
        self.prune_split_blocks(blocks);

        // Generate connect and disconnect messages.
        messages.extend(self.connects_and_disconnects(blocks, step));

        messages
    }

    /// Create messages for every relevant neighbour for every vote in the given vec.
    pub fn broadcast(&self, blocks: &Blocks, msgs: Vec<MessageContent>, step: u64) -> Vec<Message> {
        msgs.into_iter()
            .flat_map(move |content| {
                let mut recipients =
                    content.recipients(blocks, &self.current_blocks, self.our_name);
                recipients.extend(self.nodes_to_add(step));
                recipients.remove(&self.our_name);

                recipients.into_iter().map(move |recipient| {
                    Message {
                        sender: self.our_name,
                        recipient,
                        content: content.clone(),
                    }
                })
            })
            .collect()
    }

    /// Check we don't have excessive valid blocks for any given (prefix, version) pair.
    pub fn check_conflicting_block_count(&self, blocks: &Blocks) {
        let mut conflicting_counts = BTreeMap::new();
        for block in self.valid_blocks.iter().map(|b| blocks.get(b).unwrap()) {
            let count = conflicting_counts
                .entry((block.prefix, block.version))
                .or_insert(0);
            *count += 1;
            if *count == self.params.max_conflicting_blocks {
                panic!(
                    "{:?}\nhas {} valid blocks for {:?} with version {}.",
                    self.as_debug(blocks),
                    count,
                    block.prefix,
                    block.version
                );
            }
        }
    }

    /// Blocks that we can legitimately vote on successors for, because we are part of them.
    pub fn our_current_blocks<'a>(&self, blocks: &'a Blocks) -> Vec<&'a Block> {
        blocks.our_blocks(&self.current_blocks, self.our_name)
    }

    /// Get all blocks for our current section(s),
    ///
    /// i.e. all the blocks whose prefix matches `name`.
    pub fn our_current_section_blocks<'a>(&self, blocks: &'a Blocks) -> Vec<&'a Block> {
        blocks.section_blocks(&self.current_blocks, self.our_name)
    }

    /// True if the given node could be added to the given block
    fn could_be_added(&self, node: Name, block: &Block) -> bool {
        !block.members.contains(&node) && block.prefix.matches(node) &&
            !block.should_split(self.min_split_size())
    }

    /// Vote to add the oldest candidate (from our perspective) that hasn't timed out.
    fn nodes_to_add(&self, step: u64) -> Vec<Name> {
        self.candidates
            .iter()
            .filter(|&(name, candidate)| {
                self.connections.contains(name) &&
                    candidate.is_recent(self.params.join_timeout, step)
            })
            .min_by_key(|&(_, candidate)| candidate.step_added)
            .map(|(name, _)| *name)
            .into_iter()
            .collect()
    }

    fn nodes_to_drop(&self, current_block: &Block) -> Vec<Name> {
        current_block
            .members
            .iter()
            .filter(|peer| {
                **peer != self.our_name && !self.connections.contains(peer) &&
                    !self.candidates.contains_key(peer)
            })
            .cloned()
            .collect()
    }

    fn witness_votes(&self, blocks: &Blocks) -> Vec<Vote> {
        let new_current_blocks = self.current_blocks.difference(&self.prev_current_blocks);
        let mut votes = vec![];
        for block in blocks
            .block_contents(new_current_blocks)
            .into_iter()
            .filter(|b| !b.prefix.matches(self.our_name))
        {
            for our_block in self.our_current_blocks(blocks) {
                votes.push(Vote {
                    from: our_block.get_id(),
                    to: block.get_id(),
                });
            }
        }
        votes
    }

    /// Construct new successor blocks based on our view of the network.
    pub fn construct_new_votes(&self, blocks: &mut Blocks, step: u64) -> Vec<Vote> {
        let mut votes = vec![];

        let blocks_to_add = {
            let mut blocks_to_add = BTreeSet::new();
            for block in self.our_current_blocks(blocks) {
                for node in self.nodes_to_add(step) {
                    if self.could_be_added(node, block) {
                        trace!("{}: voting to add {} to: {:?}", self, node, block);
                        let added = block.add_node(node);
                        let added_id = added.get_id();
                        blocks_to_add.insert(added);
                        votes.push(Vote {
                            from: block.get_id(),
                            to: added_id,
                        });
                    }
                }
            }
            blocks_to_add
        };
        for block in blocks_to_add {
            blocks.insert(block);
        }

        let blocks_to_add = {
            let mut blocks_to_add = BTreeSet::new();
            for block in self.our_current_blocks(blocks) {
                for node in self.nodes_to_drop(&block) {
                    trace!("{}: voting to remove {} from: {:?}", self, node, block);
                    let removed = block.remove_node(node);
                    let removed_id = removed.get_id();
                    blocks_to_add.insert(removed);
                    votes.push(Vote {
                        from: block.get_id(),
                        to: removed_id,
                    });
                }
            }
            blocks_to_add
        };
        for block in blocks_to_add {
            blocks.insert(block);
        }

        for vote in split_blocks(
            blocks,
            &self.current_blocks,
            self.our_name,
            self.min_split_size(),
        )
        {
            trace!(
                "{}: voting to split from: {:?} to: {:?}",
                self,
                vote.from.into_block(blocks),
                vote.to.into_block(blocks)
            );
            votes.push(vote);
        }

        for vote in merge_blocks(
            blocks,
            &self.current_blocks,
            &self.connections,
            self.our_name,
            self.params.min_section_size,
        )
        {
            trace!(
                "{}: voting to merge from: {:?} to: {:?}",
                self,
                vote.from.into_block(blocks),
                vote.to.into_block(blocks)
            );
            votes.push(vote);
        }

        for vote in self.witness_votes(blocks) {
            trace!(
                "{}: witnessing from: {:?} to: {:?}",
                self,
                vote.from.into_block(blocks),
                vote.to.into_block(blocks)
            );
            votes.push(vote);
        }

        votes
    }

    /// Returns new votes to be broadcast after filtering them.
    pub fn broadcast_new_votes(&mut self, blocks: &mut Blocks, step: u64) -> Vec<Message> {
        let votes = self.construct_new_votes(blocks, step);
        let our_name = self.our_name;

        let mut to_broadcast = vec![];

        for vote in &votes {
            self.add_vote(vote.clone(), Some(our_name));
        }

        // Construct vote messages and broadcast.
        let vote_msgs: Vec<_> = votes.into_iter().map(VoteMsg).collect();
        to_broadcast.extend(self.broadcast(blocks, vote_msgs, step));

        self.filter_messages(to_broadcast)
    }

    /// Remove messages that have already been sent from `messages`, and update the filter.
    fn filter_messages(&mut self, messages: Vec<Message>) -> Vec<Message> {
        let mut filtered = vec![];
        for message in messages {
            let mut hasher = DefaultHasher::new();
            message.hash(&mut hasher);
            let hash = hasher.finish();
            if message.content == Connect || message.content == Disconnect ||
                !self.message_filter.contains(&hash) {
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

    /// Construct a RequestProof message
    fn request_proof(&self, blocks: &Blocks, block: BlockId, node: Name) -> Vec<Message> {
        let max_version = blocks
            .block_contents(&self.current_blocks)
            .into_iter()
            .map(|b| b.version)
            .max();
        // Request proof if the `from` block isn't valid and it's version is less than the
        // max version we have - 10
        // FIXME: this tries to prevent bootstrapping issues - find a less hacky way to do this
        if self.valid_blocks.contains(&block) ||
            max_version.map_or(true, |ver| block.into_block(blocks).version > ver + 10)
        {
            vec![]
        } else {
            vec![
                Message {
                    sender: self.our_name,
                    recipient: node,
                    content: RequestProof(block, self.current_blocks.clone()),
                },
            ]
        }
    }

    fn check_path(blocks: &Blocks, current_blocks: &CurrentBlocks, p: &[BlockId]) -> bool {
        let current_blocks_objs = blocks.block_contents(current_blocks);
        let plast = &p[p.len() - 1];
        let b0 = plast.into_block(blocks);
        current_blocks.contains(plast) ||
            current_blocks_objs.iter().any(|b| {
                b.prefix.is_compatible(&b0.prefix) && b.version > b0.version
            })
    }

    fn bundle_predecessors(&self, blocks: &Blocks, block: BlockId, node: Name) -> Message {
        let bundle = VoteBundle(
            blocks
                .predecessors(&block, &self.rev_vote_counts)
                .into_iter()
                .map(|(b, _, voters)| (Vote { from: b, to: block }, voters))
                .collect::<Vec<_>>(),
        );
        Message {
            sender: self.our_name,
            recipient: node,
            content: bundle,
        }
    }

    /// Constructs a message with a vote bundle proving the given block
    fn construct_proof(
        &self,
        blocks: &Blocks,
        block: BlockId,
        current_blocks: CurrentBlocks,
        node: Name,
    ) -> Message {
        if !self.valid_blocks.contains(&block) {
            return Message {
                sender: self.our_name,
                recipient: node,
                content: NoProof(block),
            };
        }
        if Self::check_path(blocks, &current_blocks, &[block]) {
            return self.bundle_predecessors(blocks, block, node);
        }

        let mut paths = BTreeSet::new();
        paths.insert(vec![block]);

        let mut had_predecessors = true;

        while had_predecessors &&
            !paths.iter().any(
                |p| Self::check_path(blocks, &current_blocks, p),
            )
        {
            had_predecessors = false;
            let mut new_paths = BTreeSet::new();
            for path in paths {
                let plast = path.last().unwrap();
                let predecessor_blocks = blocks
                    .predecessors(plast, &self.rev_vote_counts)
                    .into_iter()
                    .map(|(b, _, _)| b);

                for prev_block in predecessor_blocks.filter(|&b| !path.iter().any(|b2| *b2 == b)) {
                    had_predecessors = true;
                    let mut new_path = path.clone();
                    new_path.push(prev_block);
                    new_paths.insert(new_path);
                }
            }
            paths = new_paths;
        }

        if !had_predecessors {
            return Message {
                sender: self.our_name,
                recipient: node,
                content: NoProof(block),
            };
        }

        let mut path = paths
            .iter()
            .find(|&p| Self::check_path(blocks, &current_blocks, p))
            .unwrap()
            .clone();

        if path.len() < 2 {
            return Message {
                sender: self.our_name,
                recipient: node,
                content: NoProof(block),
            };
        }

        path.reverse();
        let mut bundle = Vec::new();

        trace!(
            "{}: found proof for {:?}: {:?}",
            self,
            block.into_block(blocks),
            path.iter()
                .map(|b| b.into_block(blocks))
                .collect::<Vec<_>>()
        );

        for vote in path.windows(2) {
            let names = self.vote_counts
                .get(&vote[0])
                .and_then(|map| map.get(&vote[1]))
                .unwrap()
                .clone();
            bundle.push((
                Vote {
                    from: vote[0],
                    to: vote[1],
                },
                names,
            ));
        }

        Message {
            sender: self.our_name,
            recipient: node,
            content: VoteBundle(bundle),
        }
    }

    /// Returns true if the peer is known and its state is `Disconnected`.
    pub fn is_disconnected_from(&self, name: &Name) -> bool {
        !self.connections.contains(name)
    }

    /// Returns true if this node should shutdown because it has failed to join a section.
    pub fn should_shutdown(&self, blocks: &Blocks, step: u64) -> bool {
        let timeout_elapsed = step >= self.step_created + self.params.self_shutdown_timeout;

        let (no_blocks, insufficient_connections) = match self.our_current_blocks(blocks).first() {
            Some(block) => (false, self.connections.len() * 2 < block.members.len()),
            None => (true, true),
        };

        timeout_elapsed && (no_blocks || insufficient_connections)
    }

    pub fn step_created(&self) -> u64 {
        self.step_created
    }

    fn bundle_base(&self, blocks: &Blocks, bundle: &[(Vote, BTreeSet<Name>)]) -> Vec<BlockId> {
        let mut block_ids = BTreeSet::new();
        for &(ref vote, _) in bundle {
            block_ids.insert(vote.from);
            block_ids.insert(vote.to);
        }
        block_ids
            .into_iter()
            .filter(|&b| {
                !bundle.into_iter().any(|&(ref vote, ref voters)| {
                    vote.to == b && vote.is_quorum(blocks, voters)
                })
            })
            .collect()
    }

    /// Handle a message intended for us and return messages we'd like to send.
    pub fn handle_message(&mut self, message: Message, blocks: &Blocks, step: u64) -> Vec<Message> {
        let to_send = match message.content {
            NodeJoined => {
                let joining_node = message.sender;
                debug!("{}: received join message for: {}", self, joining_node);

                // Mark the peer as having joined so that we vote to keep adding it.
                self.candidates.insert(
                    joining_node,
                    Candidate { step_added: step },
                );
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
                trace!(
                    "{}: received {:?} from {}",
                    self,
                    vote.as_debug(blocks),
                    message.sender
                );
                let messages = self.request_proof(blocks, vote.from, message.sender);
                self.add_vote(vote, Some(message.sender));
                messages
            }
            VoteAgreedMsg((vote, voters)) => {
                trace!(
                    "{}: received agreement msg for {:?} from {}",
                    self,
                    vote.as_debug(blocks),
                    message.sender
                );
                let messages = self.request_proof(blocks, vote.from, message.sender);
                self.add_vote(vote, voters);
                messages
            }
            VoteBundle(bundle) => {
                trace!("{}: received a vote bundle from {}", self, message.sender);
                let mut messages = Vec::new();
                for block in self.bundle_base(blocks, &bundle) {
                    messages.extend(self.request_proof(blocks, block, message.sender));
                }
                for (vote, voters) in bundle {
                    self.add_vote(vote, voters);
                }
                messages
            }
            BootstrapMsg(vote_counts) => {
                debug!(
                    "{}: applying bootstrap message from {}",
                    self,
                    message.sender
                );
                self.apply_bootstrap_msg(vote_counts);
                vec![]
            }
            Disconnect => {
                debug!("{}: lost our connection to {}", self, message.sender);
                self.connections.remove(&message.sender);
                self.connect_requests.remove(&message.sender);
                vec![]
            }
            Connect => {
                if self.connections.insert(message.sender) {
                    debug!("{}: obtained a connection to {}", self, message.sender);
                }
                if !self.connect_requests.contains(&message.sender) {
                    trace!("{}: connecting back to {}", self, message.sender);
                    self.connect_requests.insert(message.sender);
                    vec![
                        Message {
                            sender: self.our_name,
                            recipient: message.sender,
                            content: MessageContent::Connect,
                        },
                    ]
                } else {
                    vec![]
                }
            }
            RequestProof(block, current_blocks) => {
                trace!(
                    "{}: received a request for proof from {} for block {:?} with current blocks {:?}",
                    self,
                    message.sender,
                    block.into_block(blocks),
                    blocks.block_contents(&current_blocks)
                );
                vec![
                    self.construct_proof(blocks, block, current_blocks, message.sender),
                ]
            }
            NoProof(block) => {
                trace!(
                    "{}: {} couldn't prove block {:?}",
                    self,
                    message.sender,
                    block.into_block(blocks)
                );
                vec![]
            }
        };

        self.filter_messages(to_send)
    }

    pub fn as_debug<'a, 'b>(&'a self, blocks: &'b Blocks) -> DebugNode<'b, 'a> {
        DebugNode { blocks, node: self }
    }
}

pub struct DebugNode<'a, 'b> {
    blocks: &'a Blocks,
    node: &'b Node,
}

impl<'a, 'b> fmt::Debug for DebugNode<'a, 'b> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "Node({}): {} valid blocks;   {} vote counts with max \"to\" blocks of {:?};   {} \
               current blocks: {:#?}",
            self.node.our_name,
            self.node.valid_blocks.len(),
            self.node.vote_counts.len(),
            self.node.vote_counts.values().map(BTreeMap::len).max(),
            self.node.current_blocks.len(),
            self.blocks.block_contents(&self.node.current_blocks)
        )
    }
}
