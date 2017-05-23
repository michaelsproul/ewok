extern crate rand;
extern crate itertools;
#[macro_use]
extern crate maplit;

mod block;
mod message;
mod name;
mod network;
mod node;
mod params;
mod peer_state;
mod random;
mod simulation;
mod split;
mod util;

use simulation::Simulation;
use params::{SimulationParams, NodeParams};

fn main() {
    let params = SimulationParams {
        num_nodes: 30,
        num_steps: 5000,
        max_delay: 5,
        prob_join: 0.1,
        prob_drop: 0.00,
        drop_step: 100
    };

    let node_params = NodeParams {
        min_section_size: 4,
        split_buffer: 0,
        join_timeout: 50
    };

    let mut simulation = Simulation::new(params, node_params);

    simulation.run();
}
