use simulation::Phase;
use simulation::Phase::*;

#[derive(Clone, Debug)]
pub struct SimulationParams {
    /// Maximum number of steps a message can be delayed by before it's delivered.
    pub max_delay: u64,
    /// Probability of a node joining on a given step during the network growth phase.
    pub grow_prob_join: f64,
    /// Probability of a node leaving on a given step during the network growth phase.
    pub grow_prob_drop: f64,
    /// Probability of a node joining or leaving on a given step.
    pub prob_churn: f64,
    /// Probability of a node joining on a given step during the network shrinking phase.
    pub shrink_prob_join: f64,
    /// Probability of a node leaving on a given step during the network shrinking phase.
    pub shrink_prob_drop: f64,
    /// Probability that a two-way connection will be lost on any given step.
    pub prob_disconnect: f64,
    /// Probability that a lost two-way connection will be re-established on any given step.
    pub prob_reconnect: f64,
    /// Network starting phase is complete once the size of network reaches this value.
    pub starting_complete: usize,
    /// Network growth phase is complete once the size of network reaches this value.
    pub grow_complete: usize,
    /// Network stable phase is run for this number of steps.
    pub stable_steps: u64,
}

impl SimulationParams {
    pub fn prob_join(&self, phase: Phase) -> f64 {
        match phase {
            Starting => self.grow_prob_join,
            Growth => self.grow_prob_join,
            Stable { .. } => self.prob_churn,
            Shrinking => self.shrink_prob_join,
            Finishing { .. } => 0.0,
        }
    }

    pub fn prob_drop(&self, phase: Phase) -> f64 {
        match phase {
            Starting | Finishing { .. } => 0.0,
            Growth => self.grow_prob_drop,
            Stable { .. } => self.prob_churn,
            Shrinking => self.shrink_prob_drop,
        }
    }

    pub fn prob_disconnect(&self, phase: Phase) -> f64 {
        match phase {
            Starting | Finishing { .. } => 0.0,
            Growth | Stable { .. } | Shrinking => self.prob_disconnect,
        }
    }

    pub fn prob_reconnect(&self, phase: Phase) -> f64 {
        match phase {
            Starting | Finishing { .. } => 0.0,
            Growth | Stable { .. } | Shrinking => self.prob_reconnect,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NodeParams {
    /// Minimum section size.
    pub min_section_size: usize,
    /// Number of nodes past the minimum that must be present in all sections when splitting.
    pub split_buffer: usize,
    /// Number of steps to wait before shutting down if we fail to join.
    pub self_shutdown_timeout: u64,
    /// The maximum number of permissible valid blocks for a single prefix and version pair.
    /// Exceeding this will cause the process to panic.
    pub max_conflicting_blocks: usize,
    /// Parameters related to candidates.
    pub candidate_params: CandidateParams,
}

impl Default for NodeParams {
    fn default() -> NodeParams {
        NodeParams {
            min_section_size: 8,
            split_buffer: 1,
            self_shutdown_timeout: 100,
            max_conflicting_blocks: 20,
            candidate_params: CandidateParams::default(),
        }
    }
}

impl NodeParams {
    pub fn with_resource_proof() -> Self {
        NodeParams {
            self_shutdown_timeout: 500,
            candidate_params: CandidateParams::with_resource_proof(),
            ..NodeParams::default()
        }
    }

    pub fn max_timeout(&self) -> u64 {
        vec![
            self.self_shutdown_timeout,
            self.candidate_params.approval_timeout,
            self.candidate_params.resource_proof_timeout,
            self.candidate_params.block_timeout,
        ].into_iter().max().unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct CandidateParams {
    /// Number of steps to wait for a candidate to become approved as a candidate.
    pub approval_timeout: u64,
    /// Number of steps to wait for a candidate to pass resource proof.
    pub resource_proof_timeout: u64,
    /// Number of steps to wait for a candidate to enter a block once it has passed resource proof.
    pub block_timeout: u64,
    /// Minimum number of steps it takes a node to do resource proof.
    pub resource_proof_min: u64,
    /// Maximum number of steps it takes a node to do resource proof.
    ///
    /// If this is greater than the resource proof timeout, then some nodes will fail.
    pub resource_proof_max: u64,
}

impl Default for CandidateParams {
    fn default() -> Self {
        CandidateParams {
            approval_timeout: 50,
            resource_proof_timeout: 50,
            block_timeout: 50,
            resource_proof_min: 0,
            resource_proof_max: 10,
        }
    }
}

impl CandidateParams {
    pub fn with_resource_proof() -> Self {
        CandidateParams {
            approval_timeout: 60,
            resource_proof_timeout: 300,
            block_timeout: 60,
            resource_proof_min: 120,
            resource_proof_max: 300,
        }
    }
}

/// Compute the number of nodes required to form a majority of `num_nodes`.
///
/// You should compare `num_votes >= quorum(num_nodes)`.
pub fn quorum(num_nodes: usize) -> usize {
    (num_nodes / 2) + 1
}

/// Compute the reconnect probability to use per step so that a connection is regained with
/// 95% probability after `average_wait` steps.
pub fn reconnect_prob(average_wait: u64) -> f64 {
    1.0 - 0.05_f64.powf(1.0 / average_wait as f64)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_quorum() {
        assert_eq!(501, quorum(1000));
        assert_eq!(6, quorum(10));
        assert_eq!(5, quorum(9));
        assert_eq!(3, quorum(4));
        assert_eq!(2, quorum(3));
        assert_eq!(2, quorum(2));
    }

    #[test]
    fn test_reconnect_prob() {
        let five_step = reconnect_prob(5);
        assert!(five_step > 0.45);
        assert!(five_step < 0.46);
        let twenty_step = reconnect_prob(20);
        assert!(twenty_step > 0.13);
        assert!(twenty_step < 0.14);
    }
}
