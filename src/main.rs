extern crate ewok;

use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams};
use ewok::logging::init_logging;

fn main() {
    init_logging();

    let params = SimulationParams {
        max_delay: 60,
        grow_prob_join: 1.0 / 30.0,
        grow_prob_drop: 1.0 / 150.0,
        prob_churn: 1.0 / 60.0,
        shrink_prob_join: 1.0 / 150.0,
        shrink_prob_drop: 1.0 / 30.0,
        prob_disconnect: 1.0 / 60.0,
        // Gives ~95% chance that a pair will reconnect within 5 steps
        prob_reconnect: 1.0 / 30.0,
        starting_complete: 16,
        grow_complete: 30,
        stable_steps: 600,
    };

    let mut simulation = Simulation::new(params, NodeParams::default());

    simulation.run().unwrap();
}
