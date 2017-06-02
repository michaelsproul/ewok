use name::Name;
use std::collections::{BTreeMap, BTreeSet};
use std::mem;
use params::NodeParams;
use block::Block;
use self::PeerState::*;
use itertools::Itertools;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum PeerState {
    /// Appeared in all current blocks at some point.
    Confirmed,
    /// Appeared in at least one current block at some point.
    PartiallyConfirmed {
        // Step that the node first appeared in a current block.
        since: u64,
    },
    /// Waiting to join, has not yet been included in a current block.
    Unconfirmed { join_step: u64 },
    /// Was confirmed, but dropped from some current blocks.
    PartiallyLost { since: u64 },
    /// Currently disconnected from us.
    Disconnected {
        since: u64,
        previous_state: Box<PeerState>,
    },
}

#[derive(Debug)]
pub struct PeerStates {
    /// States of known peers.
    states: BTreeMap<Name, PeerState>,
    /// Parameters like timeouts, etc.
    params: NodeParams,
}

impl PeerStates {
    pub fn new(params: NodeParams) -> Self {
        PeerStates {
            states: BTreeMap::new(),
            params,
        }
    }

    /// Names of all known peers.
    pub fn all_peers<'a>(&'a self) -> Box<Iterator<Item = &'a Name> + 'a> {
        Box::new(self.states.keys())
    }

    /// Called when we see a NodeJoined message.
    pub fn node_joined(&mut self, name: Name, step: u64) {
        // FIXME: limit to one candidate at a time.
        self.states
            .entry(name)
            .or_insert(Unconfirmed { join_step: step });
    }

    /// Called when a node becomes current in a single block, but is not valid in all current
    /// blocks.
    pub fn in_some_current(&mut self, name: Name, step: u64) {
        let state = self.states
            .entry(name)
            .or_insert(PartiallyConfirmed { since: step });

        match *state {
            // Previously unconfirmed, upgrade to partially confirmed.
            // Previously confirmed, downgrade to partially confirmed.
            Unconfirmed { .. } => {
                *state = PartiallyConfirmed { since: step };
            }
            Confirmed => {
                *state = PartiallyLost { since: step };
            }
            PartiallyConfirmed { since } => {
                // if added more than some period ago, we mark it as a candidate to be dropped
                if since < step.saturating_sub(self.params.join_timeout) {
                    *state = PartiallyLost { since: step };
                }
            }
            _ => (),
        }
    }

    /// Called when a node appears in all current blocks.
    pub fn in_all_current(&mut self, name: Name, _step: u64) {
        let state = self.states.entry(name).or_insert(Confirmed);

        match *state {
            PartiallyConfirmed { .. } |
            PartiallyLost { .. } |
            Unconfirmed { .. } => {
                *state = Confirmed;
            }
            _ => (),
        }
    }

    /// Update a node's state in light of a disconnection.
    pub fn disconnected(&mut self, name: Name, step: u64) {
        let state = match self.states.get_mut(&name) {
            Some(s) => s,
            None => return,
        };

        match *state {
            // Already disconnected, do nothing.
            Disconnected { .. } => {}
            // Anything else, update state to disconnected.
            _ => {
                *state = Disconnected {
                    since: step,
                    previous_state: Box::new(state.clone()),
                };
            }
        };
    }

    /// Update a node's state in light of a reconnection.
    pub fn reconnected(&mut self, name: Name, _step: u64) {
        let state_ptr = match self.states.get_mut(&name) {
            Some(s) => s,
            None => return,
        };

        // FIXME: WARNING: nasty borrowck-appeasing hacks.
        let state = mem::replace(state_ptr, Confirmed);

        if let Disconnected { previous_state, .. } = state {
            *state_ptr = *previous_state;
        } else {
            warn!("warning: out-of-order reconnect");
            *state_ptr = state;
        }
    }

    /// Return all unconfirmed or partially confirmed nodes who we should keep trying to add.
    pub fn nodes_to_add(&self, step: u64) -> Vec<Name> {
        self.states
            .iter()
            .filter(|&(_, state)| {
                match *state {
                    // rule:Add
                    // candidate passed our resource proof in the last `join_timeout` steps.
                    Unconfirmed { join_step } => {
                        join_step >= step.saturating_sub(self.params.join_timeout)
                    }
                    // anything else
                    _ => false,
                }
            })
            .map(|(name, _)| *name)
            .collect()
    }

    /// Return all nodes that we should vote to remove.
    pub fn nodes_to_drop(&self, step: u64) -> Vec<Name> {
        self.states
            .iter()
            .filter(|&(_, state)| {
                match *state {
                    // rule:RmDc
                    Disconnected { .. } => true,
                    // rule:RmConv
                    PartiallyLost { since } => {
                        since < step.saturating_sub(self.params.rmconv_timeout)
                    }
                    _ => false,
                }
            })
            .map(|(name, _)| *name)
            .collect()
    }

    /// Returns true if the peer is known and its state is `Disconnected`.
    pub fn is_disconnected_from(&self, name: &Name) -> bool {
        self.states
            .get(name)
            .map_or(false,
                    |state| if let PeerState::Disconnected { .. } = *state {
                        true
                    } else {
                        false
                    })
    }
}

/// Compute the set of nodes that are in all the given blocks.
pub fn nodes_in_all(blocks: &BTreeSet<Block>) -> BTreeSet<Name> {
    if blocks.is_empty() {
        return BTreeSet::new();
    }

    blocks
        .iter()
        .map(|block| block.members.clone())
        .fold1(|members1, members2| &members1 & &members2)
        .unwrap()
}

/// Compute the set of nodes that are in any current block.
pub fn nodes_in_any(blocks: &BTreeSet<Block>) -> BTreeSet<Name> {
    blocks
        .iter()
        .fold(BTreeSet::new(), |acc, block| &acc | &block.members)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn current_blocks() {
        let block = Block::genesis(Name(0));

        let block1 = Block {
            members: btreeset!{Name(1), Name(2), Name(3), Name(4), Name(5)},
            ..block.clone()
        };
        let block2 = Block {
            members: btreeset!{Name(1), Name(3), Name(5)},
            ..block.clone()
        };

        let blocks = btreeset!{block1, block2};

        assert_eq!(nodes_in_all(&blocks), btreeset!{Name(1), Name(3), Name(5)});
        assert_eq!(nodes_in_any(&blocks),
                   btreeset!{Name(1), Name(2), Name(3), Name(4), Name(5)});
    }
}
