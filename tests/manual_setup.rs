extern crate ewok;
#[macro_use]
extern crate maplit;

use ewok::name::Prefix;
use ewok::event::Event::*;
use ewok::event_schedule::EventSchedule;
use ewok::logging::init_logging;
use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams};

fn default_params() -> SimulationParams {
    SimulationParams {
        max_num_nodes: 0,
        prob_join: 0.0,
        num_steps: 1000,
        max_delay: 5,
        prob_drop: 0.00,
        drop_step: 0,
        prob_disconnect: 0.00,
        prob_reconnect: 0.00,
    }
}

fn default_node_params() -> NodeParams {
    NodeParams {
        min_section_size: 10,
        split_buffer: 1,
        join_timeout: 10,
    }
}

#[test]
fn four_sections() {
    init_logging();

    let p00 = Prefix::short(2, 0b00000000);
    let p01 = Prefix::short(2, 0b01000000);
    let p10 = Prefix::short(2, 0b10000000);
    let p11 = Prefix::short(2, 0b11000000);

    let sections = btreemap! {
        p00 => 10,
        p01 => 10,
        p10 => 10,
        p11 => 10
    };

    let event_schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p00),
            RemoveNodeFrom(p01),
        ],
        3 => vec![RemoveNodeFrom(p01)]
    });

    let params = default_params();
    let node_params = default_node_params();

    let mut simulation = Simulation::new_from(sections, event_schedule, params, node_params);

    simulation.run().expect("eventual consistency");
}
