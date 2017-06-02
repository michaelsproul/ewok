extern crate ewok;

use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams};
use ewok::logging::init_logging;

fn main() {
    init_logging();

    let params = SimulationParams {
        max_num_nodes: 30,
        num_steps: 1150,
        max_delay: 5,
        prob_join: 0.1,
        prob_drop: 0.01,
        drop_step: 150,
        prob_disconnect: 0.05,
        // Gives ~95% chance that a pair will reconnect within 5 steps
        prob_reconnect: 0.45,
        max_conflicting_blocks: 20,
    };

    let node_params = NodeParams {
        min_section_size: 8,
        split_buffer: 1,
        join_timeout: 20,
        self_shutdown_timeout: 100,
    };

    let mut simulation = Simulation::new(params, node_params);

    simulation.run().unwrap();
}
