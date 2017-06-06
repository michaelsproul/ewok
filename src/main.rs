extern crate ewok;

use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams};
use ewok::logging::init_logging;

fn main() {
    init_logging();

    let params = SimulationParams {
        max_delay: 5,
        max_conflicting_blocks: 20,
        grow_prob_join: 0.1,
        grow_prob_drop: 0.02,
        prob_churn: 0.05,
        shrink_prob_join: 0.02,
        shrink_prob_drop: 0.1,
        prob_disconnect: 0.05,
        // Gives ~95% chance that a pair will reconnect within 5 steps
        prob_reconnect: 0.45,
        starting_complete: 16,
        grow_complete: 64,
        stable_steps: 100,
    };

    let mut simulation = Simulation::new(params, NodeParams::default());

    simulation.run().unwrap();
}
