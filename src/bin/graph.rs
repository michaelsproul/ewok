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
use std::collections::{BTreeSet, BTreeMap};
use std::fs::File;
use std::io::{Write, BufWriter};
use utils::log_parse::{LogData, LogIterator};

fn main() {
    let matches = App::new("ewok_graph")
        .about("This tool takes a log output from an Ewok simulation and generates a file \
               describing a graph of blocks in the DOT language. The resulting file can then be \
               converted into a graphics file using the 'dot' utility from the 'graphviz' toolset.")
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
    let mut blocks = BTreeMap::new();
    let mut votes = BTreeSet::new();

    let file = File::open(input).unwrap();
    let log_iter = LogIterator::new(file);

    println!("Reading log...");
    for data in log_iter {
        if let LogData::VoteAgreement(vote, block_from, block_to) = data {
            blocks.insert(block_from.get_id(), block_from);
            blocks.insert(block_to.get_id(), block_to);
            votes.insert(vote);
        }
    }

    println!("Reading finished. Outputting the dot file...");
    let file = File::create(output).unwrap();
    let mut writer = BufWriter::new(file);
    let _ = write!(writer, "digraph {{\n");
    for (b, block) in blocks {
        let _ = write!(writer,
                       "{} [label = {}; shape=box];\n",
                       b,
                       block.get_label());
    }
    for vote in votes {
        let _ = write!(writer, "{}->{}\n", vote.from, vote.to);
    }
    let _ = write!(writer, "}}\n");
}
