#[derive(Clone, Debug)]
pub struct SimulationParams {
    /// Maximum number of nodes.
    pub max_num_nodes: usize,
    /// Number of steps to run the simulation for.
    pub num_steps: u64,
    /// Maximum number of steps a message can be delayed by before it's delivered.
    pub max_delay: u64,
    /// Probability of a node joining on a given step.
    pub prob_join: f64,
    /// Probability of a node leaving on a given step.
    pub prob_drop: f64,
    /// Step at which to start dropping nodes (gives the network time to start up).
    pub drop_step: u64,
    /// Probability that a two-way connection will be lost on any given step.
    pub prob_disconnect: f64,
    /// Probability that a lost two-way connection will be re-established on any given step.
    pub prob_reconnect: f64,
}


#[derive(Clone, Debug)]
pub struct NodeParams {
    /// Minimum section size.
    pub min_section_size: u64,
    /// Number of nodes past the minimum that must be present in all sections when splitting.
    pub split_buffer: u64,
    /// Number of steps to wait for a candidate to appear in at least one current section.
    pub join_timeout: u64,
    /// Number of steps to wait before shutting down if we fail to join.
    pub self_shutdown_timeout: u64,
}
