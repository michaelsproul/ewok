//! Recommended usage:
//!
//! graph ewok_log_file -o output_file
//! dot -Tsvg -O output_file
//!
//! (The resulting images are large, so the SVG format is recommended for
//! quality-conserving zooming.)
//! The 'dot' utility can be found in the 'graphviz' package.

#![cfg_attr(feature="cargo-clippy", allow(doc_markdown))]

extern crate regex;
extern crate clap;
#[macro_use]
extern crate lazy_static;

mod utils;

use clap::{App, Arg};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Write, BufWriter};
use std::mem;
use utils::log_parse::{LogData, LogIterator};

struct StepData {
    pub msgs_sent: BTreeMap<String, u64>,
    pub msgs_queue: u64,
    pub network_size: u64,
//    pub blocks_per_prefix: BTreeMap<String, u64>,
}

/*fn calculate_blocks_per_prefix(blocks: &BTreeSet<Block>) -> BTreeMap<String, u64> {
    BTreeMap::new()
}*/

fn main() {
    let matches = App::new("ewok_graph_msgs")
        .about("This tool takes a log output from an Ewok simulation and generates a file \
            containing a description of evolution of the chain and the message queue. This \
            file can then be used to create graphs of the number of messages sent, messages \
            in queue and valid blocks for prefix for the latest version over time.")
        .arg(Arg::with_name("output")
                 .short("o")
                 .long("output")
                 .value_name("FILE")
                 .help("The name for the output file."))
        .arg(Arg::with_name("INPUT")
                 .help("Sets the input file to use")
                 .required(true)
                 .index(1))
        .get_matches();
    let input = matches.value_of("INPUT").unwrap();
    let output = matches.value_of("output").unwrap_or("output.dot");
    //let mut blocks = BTreeSet::new();
    let mut sent_msgs = BTreeMap::new();
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
    let file = File::create(output).unwrap();
    let mut writer = BufWriter::new(file);

    for (i, data) in result.into_iter().enumerate() {
        let msgs_sent: u64 = data.msgs_sent.iter().map(|(_, &v)| v).sum();
        let _ = write!(writer,
                       "{}\t{}\t{}\t{}\n",
                       i,
                       data.network_size,
                       msgs_sent,
                       data.msgs_queue);
    }
}
