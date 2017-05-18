//#![allow(unused)]

extern crate rand;

mod block;
mod message;
mod name;
mod network;
mod node;
mod random;
mod simulation;

use simulation::Simulation;

fn main() {
    let num_steps = 50;
    let max_delay = 10;
    let num_nodes = 5;
    let apc = num_steps; // effectively disabled
    let mut simulation = Simulation::new(num_steps, max_delay, num_nodes, apc);

    simulation.run();
}
