#![cfg_attr(feature="cargo-clippy", allow(doc_markdown))]

extern crate regex;
extern crate clap;
#[macro_use]
extern crate lazy_static;

mod utils;

use clap::{App, Arg};
use std::collections::{BTreeSet, BTreeMap};
use std::fs::File;
use std::io::{Write, BufWriter};
use std::mem;
use utils::log_parse::{LogData, LogIterator};
use std::process::Command;

struct StepData {
    pub msgs_sent: BTreeMap<String, u64>,
    pub msgs_queue: u64,
    pub network_size: u64,
//    pub blocks_per_prefix: BTreeMap<String, u64>,
}

/*fn calculate_blocks_per_prefix(blocks: &BTreeSet<Block>) -> BTreeMap<String, u64> {
    BTreeMap::new()
}*/

fn gnuplot_command(input: &str,
                   output: &str,
                   network_size: bool,
                   queue_size: bool,
                   total_sent: bool,
                   avg_sent: bool,
                   max_sent: bool)
                   -> String {
    let mut column_counter = 2;
    let mut params = vec![];

    if network_size {
        params.push(format!("'{}' u 1:{} title 'Nodes' lt rgb '#8000FF'",
                            input,
                            column_counter));
        column_counter += 1;
    }
    if queue_size {
        params.push(format!("'{}' u 1:(${} / 100) title 'Queue size / 100' lt rgb '#FF0000'",
                            input,
                            column_counter));
        column_counter += 1;
    }
    if total_sent {
        params.push(format!("'{}' u 1:(${} / 100) title 'Messages sent / 100' lt rgb '#00FF00'",
                            input,
                            column_counter));
        column_counter += 1;
    }
    if avg_sent {
        params.push(format!("'{}' u 1:{} title 'Avg messages sent' lt rgb '#000080'",
                            input,
                            column_counter));
        column_counter += 1;
    }
    if max_sent {
        params.push(format!("'{}' u 1:{} title 'Max messages sent' lt rgb '#222222'",
                            input,
                            column_counter));
    }

    format!("set terminal png size 1920,1080; set output '{}';\
            set style data lines; set xlabel 'Step'; plot {}",
            output,
            params.join(","))
}

fn main() {
    let matches = App::new("ewok_graph_msgs")
        .about("This tool takes a log output from an Ewok simulation and generates a file \
            containing a description of evolution of the chain and the message queue. This \
            file can then be used to create graphs of the number of messages sent, messages \
            in queue and valid blocks for prefix for the latest version over time.\n\n\
            Output file row format:\n\n\
            step_number [network_size] [queue_size] [total_messages_sent] [avg_messages_sent] \
            [max_messages_sent_per_node]")
        .arg(Arg::with_name("output")
                 .short("o")
                 .long("output")
                 .value_name("FILE")
                 .help("The name for the output file."))
        .arg(Arg::with_name("include_network_size")
                 .short("n")
                 .long("network-size")
                 .takes_value(false)
                 .help("Include the network size in the output"))
        .arg(Arg::with_name("include_queue_size")
                 .short("q")
                 .long("queue-size")
                 .takes_value(false)
                 .help("Include the queue size in the output"))
        .arg(Arg::with_name("include_total_sent")
                 .short("t")
                 .long("total-sent")
                 .takes_value(false)
                 .help("Include the total number of messages sent in the output"))
        .arg(Arg::with_name("include_avg_sent")
                 .short("a")
                 .long("average-sent")
                 .takes_value(false)
                 .help("Include the average number of messages sent per node in the output"))
        .arg(Arg::with_name("include_max_sent")
                 .short("m")
                 .long("max-sent")
                 .takes_value(false)
                 .help("Include the maximum number of messages sent per node in the output"))
        .arg(Arg::with_name("plot")
                 .short("p")
                 .long("plot")
                 .value_name("PLOT")
                 .help("Plot the graph using gnuplot to the given file"))
        .arg(Arg::with_name("INPUT")
                 .help("Sets the input file to use")
                 .required(true)
                 .index(1))
        .get_matches();
    let input = matches.value_of("INPUT").unwrap();
    let output = matches.value_of("output").unwrap_or("output.dot");
    let network_size = matches.is_present("include_network_size");
    let queue_size = matches.is_present("include_queue_size");
    let total_sent = matches.is_present("include_total_sent");
    let avg_sent = matches.is_present("include_avg_sent");
    let max_sent = matches.is_present("include_max_sent");
    let plot = matches.value_of("plot");
    //let mut blocks = BTreeSet::new();
    let mut sent_msgs = BTreeMap::new();
    let mut node_names = BTreeSet::new();
    let mut result = Vec::new();
    let mut msgs_in_queue = 0;

    let file = File::open(input).unwrap();
    let log_iter = LogIterator::new(file);

    println!("Reading log...");
    for data in log_iter {
        match data {
            /*LogData::VoteAgreement(_, block_from, block_to) => {
                blocks.insert(block_from);
                blocks.insert(block_to);
            }*/
            LogData::SentMsgs(name, sent) => {
                node_names.insert(name.clone());
                let count = sent_msgs.entry(name).or_insert(0);
                *count += sent;
            }
            LogData::MsgsInQueue(count) => {
                msgs_in_queue = count;
            }
            LogData::Step(s, n) if s > 0 => {
                let data = StepData {
                    msgs_sent: mem::replace(&mut sent_msgs, BTreeMap::new()),
                    msgs_queue: msgs_in_queue,
                    network_size: n,
                    //blocks_per_prefix: calculate_blocks_per_prefix(&blocks),
                };
                result.push(data);
            }
            _ => (),
        }
    }

    println!("Reading finished. Generating output...");
    let file = File::create(&output).unwrap();
    let mut writer = BufWriter::new(file);

    for (i, data) in result.into_iter().enumerate() {
        write!(writer, "{}", i).unwrap();
        if network_size {
            write!(writer, "\t{}", data.network_size).unwrap();
        }
        if queue_size {
            write!(writer, "\t{}", data.msgs_queue).unwrap();
        }
        if total_sent {
            write!(writer,
                   "\t{}",
                   data.msgs_sent.iter().map(|(_, n)| *n).sum::<u64>())
                    .unwrap();
        }
        if avg_sent {
            write!(writer,
                   "\t{}",
                   data.msgs_sent.iter().map(|(_, n)| *n).sum::<u64>() as f64 /
                   data.network_size as f64)
                    .unwrap();
        }
        if max_sent {
            write!(writer,
                   "\t{}",
                   data.msgs_sent
                       .iter()
                       .map(|(_, n)| *n)
                       .max()
                       .unwrap_or(0))
                    .unwrap();
        }
        write!(writer, "\n").unwrap();
    }

    if let Some(plot_output) = plot {
        let command = gnuplot_command(&output,
                                      &plot_output,
                                      network_size,
                                      queue_size,
                                      total_sent,
                                      avg_sent,
                                      max_sent);
        let mut child = Command::new("gnuplot")
            .args(&["-e", &command])
            .spawn()
            .expect("failed to execute gnuplot");
        let _ = child.wait().expect("failed to wait on child");
    }

    println!("Done!");
}
