extern crate rand;
extern crate itertools;
#[macro_use]
extern crate maplit;

mod block;
mod consistency;
mod message;
mod name;
mod network;
mod node;
mod params;
mod peer_state;
mod random;
mod simulation;
mod split;

use simulation::Simulation;
use params::{SimulationParams, NodeParams};

fn main() {
    let params = SimulationParams {
        num_nodes: 30,
        num_steps: 1150,
        max_delay: 5,
        prob_join: 0.1,
        prob_drop: 0.01,
        drop_step: 150,
        prob_disconnect: 0.05,
        // Gives ~95% chance that a pair will reconnect within 5 steps
        prob_reconnect: 0.45,
    };

    let node_params = NodeParams {
        min_section_size: 4,
        split_buffer: 0,
        join_timeout: 20,
    };

    let mut simulation = Simulation::new(params, node_params);

    simulation.run().unwrap();
}
