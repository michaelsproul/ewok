#[derive(Clone)]
pub struct NodeParams {
    /// Number of steps to wait for a node to appear in all current sections after it has
    /// first appeared in a single current section.
    pub join_stabilisation_timeout: u64
}
