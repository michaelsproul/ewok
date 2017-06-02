extern crate ewok;
#[macro_use]
extern crate maplit;

use ewok::name::Prefix;
use ewok::event::Event::*;
use ewok::event_schedule::EventSchedule;
use ewok::logging::init_logging;
use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams};
use ewok::random::random;

// TODO: parameterise tests by their basic parameters like max_delay and num_steps
// so we can easily run all the tests with different values.
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
        min_section_size: 0,
        split_buffer: 1,
        join_timeout: 10,
        rmconv_timeout: 10,
        mergeconv_timeout: 10,
        self_shutdown_timeout: 30,
    }
}

fn p0() -> Prefix {
    Prefix::short(1, 0)
}
fn p1() -> Prefix {
    Prefix::short(1, 0b10000000)
}
fn p00() -> Prefix {
    Prefix::short(2, 0b00000000)
}
fn p01() -> Prefix {
    Prefix::short(2, 0b01000000)
}
fn p10() -> Prefix {
    Prefix::short(2, 0b10000000)
}
fn p11() -> Prefix {
    Prefix::short(2, 0b11000000)
}

#[test]
fn four_sections() {
    init_logging();

    let min_section_size = 10;
    let sections = btreemap! {
        p00() => min_section_size,
        p01() => min_section_size,
        p10() => min_section_size,
        p11() => min_section_size
    };

    let event_schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p00()),
            RemoveNodeFrom(p01()),
        ],
        3 => vec![RemoveNodeFrom(p01())]
    });

    let params = default_params();
    let node_params = NodeParams {
        min_section_size,
        ..default_node_params()
    };

    let mut simulation = Simulation::new_from(sections, event_schedule, params, node_params);

    simulation.run().unwrap();
}

// Fraser's example 1 from: https://github.com/Fraser999/Wookie/tree/master/Example%201
#[test]
fn two_drop_one_join() {
    init_logging();

    let num_initial = 6;
    let sections = btreemap! {
        Prefix::empty() => num_initial,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(Prefix::empty()),
            RemoveNodeFrom(Prefix::empty()),
            AddNode(random()),
        ],
    });

    let params = SimulationParams {
        max_delay: 100,
        max_num_nodes: num_initial + 1,
        ..default_params()
    };
    let node_params = NodeParams {
        min_section_size: 0,
        split_buffer: 1000,
        ..default_node_params()
    };

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);

    simulation.run().unwrap();
}

// Fraser's example 4 from: https://github.com/Fraser999/Wookie/tree/master/Example%204
#[test]
fn two_drop_merge() {
    init_logging();

    let min_section_size = 5;
    let sections = btreemap! {
        p0() => min_section_size,
        p1() => min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p0()),
            RemoveNodeFrom(p0()),
        ],
    });

    let params = default_params();
    let node_params = NodeParams {
        min_section_size,
        ..default_node_params()
    };

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}

#[test]
fn cascading_merge() {
    init_logging();

    let min_section_size = 5;
    let sections = btreemap! {
        p0() => min_section_size,
        p10() => min_section_size,
        p11() => min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p0()),
        ],
    });

    let params = default_params();
    let node_params = NodeParams {
        min_section_size,
        ..default_node_params()
    };

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}


// Fraser's example 5.
#[test]
fn one_join_one_drop() {
    init_logging();

    let min_section_size = 5;
    let sections = btreemap! {
        p0() => min_section_size,
        p1() => min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            AddNode(p0().substituted_in(random())),
            RemoveNodeFrom(p0()),
        ],
    });

    let params = SimulationParams {
        max_num_nodes: 2 * min_section_size + 1,
        ..default_params()
    };

    let node_params = NodeParams {
        min_section_size,
        ..default_node_params()
    };

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}

#[test]
fn triple_drop_merge() {
    init_logging();

    let min_section_size = 10;
    let sections = btreemap! {
        p0() => min_section_size,
        p1() => min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p0()),
            RemoveNodeFrom(p0()),
            RemoveNodeFrom(p0())
        ],
    });

    let params = default_params();
    let node_params = NodeParams {
        min_section_size,
        ..default_node_params()
    };

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}
