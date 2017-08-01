use block::{Block, is_quorum_of};
use name::Name;
use params::CandidateParams;
use random::rand_int;

use self::Candidate::*;

use std::mem;
use std::collections::BTreeSet;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub enum Candidate {
    /// State that candidates are in before we vote to accept them as a candidate.
    UnapprovedCandidate {
        /// Step at which we first became aware of this candidate.
        since: u64,
        /// Votes we've received from our section to approve this candidate.
        voters: BTreeSet<Name>,
        /// Number of steps it will take this node to complete resource proof (sim. only).
        resource_proof_steps: u64,
    },
    /// State that candidates are in once we've voted to accept them as a candidate, but
    /// they have not yet passed our resource proof challenge.
    ApprovedCandidate {
        /// Step at which we began resource proof.
        since: u64,
        /// Number of steps it will take this node to complete resource proof (sim. only).
        resource_proof_steps: u64,
    },
    /// State that candidates are in once they've passed our resource proof challenge and we're
    /// voting to add them to a block.
    UnapprovedNode {
        /// Step at which the candidate passed our resource proof.
        since: u64,
    }
}

impl Candidate {
    pub fn new(params: &CandidateParams, step: u64) -> Self {
        UnapprovedCandidate {
            since: step,
            voters: btreeset!{},
            resource_proof_steps: rand_int(params.resource_proof_min, params.resource_proof_max),
        }
    }

    pub fn check_resource_proof(&mut self, params: &CandidateParams, step: u64) -> bool {
        if let ApprovedCandidate { resource_proof_steps, since } = *self {
            if step >= since + resource_proof_steps && !self.has_timed_out(params, step) {
                *self = UnapprovedNode {
                    since: step,
                };
                return true;
            }
        }
        false
    }

    pub fn should_add_to_block(&self, params: &CandidateParams, step: u64) -> bool {
        match *self {
            UnapprovedNode { .. } => !self.has_timed_out(params, step),
            _ => false,
        }
    }

    pub fn add_approval_vote(&mut self, current_block: &Block, voter: Name, step: u64) -> Option<BTreeSet<Name>> {
        let new_state = if let UnapprovedCandidate { ref mut voters, resource_proof_steps, .. } =
            *self
        {
            voters.insert(voter);

            if is_quorum_of(voters, &current_block.members) {
                let voters = mem::replace(voters, btreeset!{});
                let new_state = ApprovedCandidate {
                    since: step,
                    resource_proof_steps,
                };

                Some((new_state, voters))
            } else {
                None
            }
        } else {
            None
        };

        if let Some((new_state, voters)) = new_state {
            *self = new_state;
            Some(voters)
        } else {
            None
        }
    }

    pub fn has_timed_out(&self, params: &CandidateParams, step: u64) -> bool {
        match *self {
            UnapprovedCandidate { since, .. } => step > since + params.approval_timeout,
            ApprovedCandidate { since, .. } => step > since + params.resource_proof_timeout,
            UnapprovedNode { since } => step > since + params.block_timeout,
        }
    }
}
