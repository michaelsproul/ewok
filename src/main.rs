extern crate rand;

mod block;
mod message;
mod name;
mod network;
mod node;
mod peer_state;
mod random;
mod simulation;

use simulation::Simulation;

fn main() {
    let num_steps = 100;
    let max_delay = 5;
    let num_nodes = 5;
    let apc = num_steps; // effectively disabled
    let join_prob = 0.1;
    let mut simulation = Simulation::new(num_steps, max_delay, num_nodes, apc, join_prob);

    simulation.run();
}
