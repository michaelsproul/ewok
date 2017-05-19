extern crate rand;

mod block;
mod message;
mod name;
mod network;
mod node;
mod params;
mod peer_state;
mod random;
mod simulation;

use simulation::Simulation;
use params::NodeParams;

fn main() {
    let num_steps = 100;
    let max_delay = 5;
    let num_nodes = 5;
    let join_prob = 0.1;

    let node_params = NodeParams {
        join_stabilisation_timeout: 50
    };

    let mut simulation = Simulation::new(num_steps, max_delay, num_nodes, join_prob, node_params);

    simulation.run();
}
