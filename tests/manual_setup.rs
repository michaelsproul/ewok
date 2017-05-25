extern crate ewok;
#[macro_use]
extern crate maplit;

use ewok::name::Prefix;
use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams};

fn default_params() -> SimulationParams {
    SimulationParams {
        num_nodes: 0,
        prob_join: 0.0,
        num_steps: 1000,
        max_delay: 5,
        prob_drop: 0.01,
        drop_step: 0,
        prob_disconnect: 0.01,
        prob_reconnect: 0.45
    }
}

fn default_node_params() -> NodeParams {
    NodeParams {
        min_section_size: 10,
        split_buffer: 1,
        join_timeout: 10
    }
}

#[test]
fn four_sections() {
    let sections = btreemap! {
        Prefix::short(2, 0b00000000) => 10,
        Prefix::short(2, 0b01000000) => 10,
        Prefix::short(2, 0b10000000) => 10,
        Prefix::short(2, 0b11000000) => 10
    };

    let params = default_params();
    let node_params = default_node_params();

    let mut simulation = Simulation::new_from(sections, params, node_params);

    simulation.run().expect("eventual consistency");
}
