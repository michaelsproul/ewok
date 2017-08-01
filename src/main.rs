extern crate ewok;

use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams, reconnect_prob};
use ewok::logging::init_logging;

fn main() {
    init_logging();

    let params = SimulationParams {
        max_delay: 30,
        grow_prob_join: 1.0 / 200.0,
        grow_prob_drop: 1.0 / 2000.0,
        prob_churn: 1.0 / 200.0,
        shrink_prob_join: 1.0 / 4000.0,
        shrink_prob_drop: 1.0 / 30.0,
        prob_disconnect: 1.0 / 600.0,
        prob_reconnect: reconnect_prob(20),
        starting_complete: 16,
        grow_complete: 30,
        stable_steps: 1000,
    };

    let mut simulation = Simulation::new(params, NodeParams::with_resource_proof());

    simulation.run().unwrap();
}
