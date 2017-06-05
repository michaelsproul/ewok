//! Recommended usage:
//!
//! graph ewok_log_file -o output_file
//! dot -Tsvg -O output_file
//!
//! (The resulting images are large, so the SVG format is recommended for
//! quality-conserving zooming.)
//! The 'dot' utility can be found in the 'graphviz' package.
extern crate regex;
extern crate clap;
use regex::Regex;
use clap::{App, Arg};
use std::collections::{BTreeSet, BTreeMap};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::io::{BufReader, BufRead, Write, BufWriter};
use std::fmt;

fn hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct Members(pub BTreeSet<String>);

impl fmt::Display for Members {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut count = 0;
        for name in &self.0 {
            count += 1;
            write!(f, "<font color=\"#{}\">{}</font>, ", name, name)?;
            if count % 3 == 0 {
                write!(f, "<br/>")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Block {
    pub prefix: String,
    pub version: u64,
    pub members: Members,
}

impl Block {
    fn get_id(&self) -> String {
        format!("prefix{}_v{}_{}",
                self.prefix,
                self.version,
                hash(&self.members))
    }

    fn get_label(&self) -> String {
        format!("<Prefix: ({})<br/>Version: {}<br/>Members: <br/>{}>",
                self.prefix,
                self.version,
                self.members)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Vote {
    pub from: String,
    pub to: String,
}

fn main() {
    let agreement_re = Regex::new(r"^Node\((?P<node>[0-9a-f]{6}\.\.)\): received agreement for Vote \{ from: Block \{ prefix: Prefix\((?P<pfrom>[01]*)\), version: (?P<vfrom>\d+), members: \{(?P<mfrom>[0-9a-f]{6}\.\.(, [0-9a-f]{6}\.\.)*)\} \}, to: Block \{ prefix: Prefix\((?P<pto>[01]*)\), version: (?P<vto>\d+), members: \{(?P<mto>[0-9a-f]{6}\.\.(, [0-9a-f]{6}\.\.)*)\} \} \}").unwrap();

    let matches = App::new("ewok_graph")
        .about("This tool takes a log output from an Ewok simulation and generates a file describing a graph of blocks in the DOT language. The resulting file can then be converted into a graphics file using the 'dot' utility from the 'graphviz' toolset.")
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
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    println!("Reading log...");
    while reader.read_line(&mut line).unwrap() > 0 {
        if let Some(caps) = agreement_re.captures(&line) {
            let block_from = Block {
                prefix: caps["pfrom"].to_owned(),
                version: caps["vfrom"]
                    .parse()
                    .ok()
                    .expect("invalid version number"),
                members: Members(caps["mfrom"]
                                     .split(", ")
                                     .map(|s| s.to_owned())
                                     .collect()),
            };
            let block_to = Block {
                prefix: caps["pto"].to_owned(),
                version: caps["vto"]
                    .parse()
                    .ok()
                    .expect("invalid version number"),
                members: Members(caps["mto"].split(", ").map(|s| s.to_owned()).collect()),
            };
            let from_id = block_from.get_id();
            let to_id = block_to.get_id();
            blocks.insert(from_id.clone(), block_from);
            blocks.insert(to_id.clone(), block_to);
            let vote = Vote {
                from: from_id,
                to: to_id,
            };
            votes.insert(vote);
        }
        line.clear();
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
