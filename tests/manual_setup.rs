extern crate ewok;
#[macro_use]
extern crate maplit;
#[macro_use]
extern crate unwrap;

use ewok::name::Prefix;
use ewok::event::Event;
use ewok::event::Event::*;
use ewok::event_schedule::EventSchedule;
use ewok::logging::init_logging;
use ewok::simulation::Simulation;
use ewok::params::{SimulationParams, NodeParams};
use ewok::random::random;
use std::iter;

// TODO: parameterise tests by their basic parameters like max_delay and num_steps
// so we can easily run all the tests with different values.
fn default_params() -> SimulationParams {
    SimulationParams {
        max_delay: 5,
        grow_prob_join: 0.0,
        grow_prob_drop: 0.0,
        prob_churn: 0.0,
        shrink_prob_join: 0.0,
        shrink_prob_drop: 0.0,
        prob_disconnect: 0.0,
        prob_reconnect: 0.0,
        starting_complete: 0,
        grow_complete: 0,
        stable_steps: 1000,
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
fn p010() -> Prefix {
    Prefix::short(3, 0b01000000)
}
fn p100() -> Prefix {
    Prefix::short(3, 0b10000000)
}
fn p110() -> Prefix {
    Prefix::short(3, 0b11000000)
}
fn p111() -> Prefix {
    Prefix::short(3, 0b11100000)
}
fn p0110() -> Prefix {
    Prefix::short(4, 0b01100000)
}
fn p0111() -> Prefix {
    Prefix::short(4, 0b01110000)
}
fn p1010() -> Prefix {
    Prefix::short(4, 0b10100000)
}
fn p1011() -> Prefix {
    Prefix::short(4, 0b10110000)
}

#[test]
fn four_sections() {
    init_logging();

    let params = default_params();
    let node_params = NodeParams::default();

    let sections =
        btreemap! {
        p00() => node_params.min_section_size,
        p01() => node_params.min_section_size,
        p10() => node_params.min_section_size,
        p11() => node_params.min_section_size
    };

    let event_schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p00()),
            RemoveNodeFrom(p01()),
        ],
        3 => vec![RemoveNodeFrom(p01())]
    });

    let mut simulation = Simulation::new_from(sections, event_schedule, params, node_params);

    simulation.run().unwrap();
}

// 00 and 01 merge into 0 at the same time that 10 and 11 merge into 1.
#[test]
fn parallel_merge() {
    init_logging();

    let params = SimulationParams {
        max_delay: 20,
        ..default_params()
    };
    let node_params = NodeParams::default();

    let sections =
        btreemap! {
        p00() => node_params.min_section_size,
        p01() => node_params.min_section_size,
        p10() => node_params.min_section_size,
        p11() => node_params.min_section_size
    };

    let event_schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p00()),
            RemoveNodeFrom(p11()),
        ],
    });

    let mut simulation = Simulation::new_from(sections, event_schedule, params, node_params);

    simulation.run().unwrap();
}

#[test]
#[ignore] //FIXME
fn parallel_merge_with_adds() {
    init_logging();

    let params = SimulationParams {
        max_delay: 20,
        ..default_params()
    };
    let node_params = NodeParams::default();

    let sections =
        btreemap! {
        p00() => node_params.min_section_size,
        p01() => node_params.min_section_size,
        p10() => node_params.min_section_size,
        p11() => node_params.min_section_size
    };

    let event_schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p00()),
            RemoveNodeFrom(p11()),
            AddNode(p00().substituted_in(random())),
            AddNode(p11().substituted_in(random())),
        ],
    });

    let mut simulation = Simulation::new_from(sections, event_schedule, params, node_params);

    simulation.run().unwrap();
}

#[test]
fn parallel_cascading_merges() {
    init_logging();

    let params = SimulationParams {
        max_delay: 20,
        ..default_params()
    };
    let node_params = NodeParams::default();
    let min_section_size = node_params.min_section_size;

    let sections =
        btreemap! {
        p00() => node_params.min_section_size,
        p010() => node_params.min_section_size,
        p0110() => node_params.min_section_size,
        p0111() => node_params.min_section_size,
        p100() => node_params.min_section_size,
        p1010() => node_params.min_section_size,
        p1011() => node_params.min_section_size,
        p11() => node_params.min_section_size
    };

    let event_schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p00()),
            RemoveNodeFrom(p010()),
            RemoveNodeFrom(p100()),
            RemoveNodeFrom(p11()),
        ],
    });

    let mut simulation = Simulation::new_from(sections, event_schedule, params, node_params);

    let final_blocks = simulation.run().unwrap();

    // Check that only 4 nodes were lost.
    let total_nodes: usize = final_blocks.values().map(|b| b.members.len()).sum();
    assert_eq!(total_nodes, 8 * min_section_size - 4);
}

// Fraser's example 1 from: https://github.com/Fraser999/Wookie/tree/master/Example%201
#[test]
fn two_drop_one_join() {
    init_logging();

    let num_initial = 6;
    let params = SimulationParams {
        max_delay: 100,
        ..default_params()
    };
    let node_params = NodeParams {
        min_section_size: 0,
        split_buffer: 1000,
        ..NodeParams::default()
    };

    let sections =
        btreemap! {
        Prefix::empty() => num_initial,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(Prefix::empty()),
            RemoveNodeFrom(Prefix::empty()),
            AddNode(random()),
        ],
    });

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);

    simulation.run().unwrap();
}

// Fraser's example 4 from: https://github.com/Fraser999/Wookie/tree/master/Example%204
#[test]
fn two_drop_merge() {
    init_logging();

    let params = default_params();
    let node_params = NodeParams::default();

    let sections =
        btreemap! {
        p0() => node_params.min_section_size,
        p1() => node_params.min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p0()),
            RemoveNodeFrom(p0()),
        ],
    });

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}

#[test]
fn cascading_merge() {
    init_logging();

    let params = default_params();
    let node_params = NodeParams::default();

    let sections =
        btreemap! {
        p0() => node_params.min_section_size,
        p10() => node_params.min_section_size,
        p11() => node_params.min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p0()),
        ],
    });

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params.clone());
    let final_blocks = simulation.run().unwrap();

    let final_block = unwrap!(final_blocks.get(&Prefix::empty()));

    assert_eq!(
        final_block.members.len(),
        3 * node_params.min_section_size - 1
    );
}


// Fraser's example 5.
#[test]
fn one_join_one_drop() {
    init_logging();

    let node_params = NodeParams::default();
    let params = default_params();

    let sections =
        btreemap! {
        p0() => node_params.min_section_size,
        p1() => node_params.min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![RemoveNodeFrom(p0())],
        1 => vec![AddNode(p0().substituted_in(random()))],
    });

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}

#[test]
fn triple_drop_merge() {
    init_logging();

    let node_params = NodeParams::default();
    let params = default_params();

    let sections =
        btreemap! {
        p0() => node_params.min_section_size,
        p1() => node_params.min_section_size,
    };

    let schedule = EventSchedule::new(btreemap! {
        0 => vec![
            RemoveNodeFrom(p0()),
            RemoveNodeFrom(p0()),
            RemoveNodeFrom(p0())
        ],
    });

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}

fn step_num(schedule: &EventSchedule, offset: u64) -> u64 {
    schedule.schedule.keys().next_back().cloned().unwrap_or(0) + offset
}

fn add_events(schedule: &mut EventSchedule, offset: u64, spacing: u64, events: Vec<Event>) {
    let start_step = step_num(schedule, offset);

    let timed_events = events.into_iter().enumerate().map(|(i, ev)| {
        (start_step + (i as u64 + 1) * spacing, vec![ev])
    });

    schedule.schedule.extend(timed_events);
}

#[test]
fn growth_then_cascade() {
    init_logging();

    let node_params = NodeParams::default();
    let params = default_params();

    let sections =
        btreemap! {
        p0() => node_params.min_section_size,
        p1() => node_params.min_section_size,
    };

    let step_size = 20;
    let mut schedule = EventSchedule::empty();

    let add_to = |prefix: Prefix| AddNode(prefix.substituted_in(random()));

    // 8 nodes in 10.
    add_events(
        &mut schedule,
        0,
        step_size,
        (0..9).map(|_| add_to(p10())).collect(),
    );

    // 8 nodes in 111 (should cause a split into 10 and 11).
    add_events(
        &mut schedule,
        50,
        step_size,
        (0..9).map(|_| add_to(p111())).collect(),
    );

    // 8 nodes in 110 (should cause a split into 110 and 111).
    add_events(
        &mut schedule,
        50,
        step_size,
        (0..9).map(|_| add_to(p110())).collect(),
    );

    // Remove 8 nodes from 10, should cause a cascading merge!
    add_events(
        &mut schedule,
        50,
        2 * step_size,
        (0..8).map(|_| RemoveNodeFrom(p10())).collect(),
    );

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}

#[test]
#[ignore]
fn growth_then_force_merge() {
    init_logging();

    let node_params = NodeParams::default();
    let params = default_params();

    let sections =
        btreemap! {
        Prefix::short(0, 0) => 1
    };

    let step_size = 20;
    let mut schedule = EventSchedule::empty();

    let add_to = |prefix: Prefix| AddNode(prefix.substituted_in(random()));

    // 9 nodes in 00.
    add_events(
        &mut schedule,
        0,
        step_size,
        (0..9).map(|_| add_to(p00())).collect(),
    );

    // 9 nodes in 01.
    add_events(
        &mut schedule,
        0,
        step_size,
        (0..9).map(|_| add_to(p01())).collect(),
    );

    // 9 nodes in 10.
    add_events(
        &mut schedule,
        0,
        step_size,
        (0..9).map(|_| add_to(p10())).collect(),
    );

    // 9 nodes in 11.
    add_events(
        &mut schedule,
        0,
        step_size,
        (0..9).map(|_| add_to(p11())).collect(),
    );

    // add some blocks to 10.
    add_events(
        &mut schedule,
        0,
        step_size,
        (0..2).map(|_| add_to(p10())).collect(),
    );

    // Drop 5 nodes from 11 simultaneously - should trigger a force merge
    let step = step_num(&schedule, 150);
    schedule.schedule.extend(iter::once(
        (step, (0..5).map(|_| RemoveNodeFrom(p11())).collect()),
    ));

    let mut simulation = Simulation::new_from(sections, schedule, params, node_params);
    simulation.run().unwrap();
}
