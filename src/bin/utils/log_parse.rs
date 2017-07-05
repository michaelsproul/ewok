use regex::Regex;
use super::chain::{Block, Vote, Members};
use std::convert::AsRef;
use std::fs::File;
use std::io::{BufReader, BufRead};

lazy_static!{
    static ref AGREEMENT_RE: Regex = Regex::new(r"^Node\((?P<node>[0-9a-f]{6}\.\.)\): received agreement for DebugVote \{ from: Block \{ prefix: Prefix\((?P<pfrom>[01]*)\), version: (?P<vfrom>\d+), members: \{(?P<mfrom>[0-9a-f]{6}\.\.(, [0-9a-f]{6}\.\.)*)\} \}, to: Block \{ prefix: Prefix\((?P<pto>[01]*)\), version: (?P<vto>\d+), members: \{(?P<mto>[0-9a-f]{6}\.\.(, [0-9a-f]{6}\.\.)*)\} \} \}").unwrap();
    static ref STEP_RE: Regex = Regex::new(r"^-- step (?P<step>\d+) \(.+\) (?P<nodes>\d+) nodes --").unwrap();
    static ref SENT_RE: Regex = Regex::new(r"^Network: sent (?P<sent>\d+) messages from (?P<name>[0-9a-f]{6})\.\.").unwrap();
    static ref QUEUE_RE: Regex = Regex::new(r"^- (?P<count>\d+) messages still in queue").unwrap();
}

pub enum LogData {
    VoteAgreement(Vote, Block, Block),
    Step(u64, u64),
    SentMsgs(String, u64),
    MsgsInQueue(u64),
}

impl LogData {
    fn from_line<T: AsRef<str>>(arg: T) -> Option<LogData> {
        let line = arg.as_ref();
        if let Some(caps) = AGREEMENT_RE.captures(line) {
            let block_from = Block {
                prefix: caps["pfrom"].to_owned(),
                version: caps["vfrom"].parse().expect("invalid version number"),
                members: Members(caps["mfrom"].split(", ").map(|s| s.to_owned()).collect()),
            };
            let block_to = Block {
                prefix: caps["pto"].to_owned(),
                version: caps["vto"].parse().expect("invalid version number"),
                members: Members(caps["mto"].split(", ").map(|s| s.to_owned()).collect()),
            };
            let from_id = block_from.get_id();
            let to_id = block_to.get_id();
            let vote = Vote {
                from: from_id,
                to: to_id,
            };
            Some(LogData::VoteAgreement(vote, block_from, block_to))
        } else if let Some(caps) = STEP_RE.captures(line) {
            let step_num = caps["step"].parse().expect("invalid step number");
            let nodes = caps["nodes"].parse().expect("invalid number of nodes");
            Some(LogData::Step(step_num, nodes))
        } else if let Some(caps) = SENT_RE.captures(line) {
            let sent = caps["sent"].parse().expect("invalid message count");
            let name = caps["name"].to_owned();
            Some(LogData::SentMsgs(name, sent))
        } else if let Some(caps) = QUEUE_RE.captures(line) {
            let count = caps["count"].parse().expect("invalid message count");
            Some(LogData::MsgsInQueue(count))
        } else {
            None
        }
    }
}

pub struct LogIterator {
    file: BufReader<File>,
    line: String,
}

impl LogIterator {
    pub fn new(file: File) -> LogIterator {
        LogIterator {
            file: BufReader::new(file),
            line: String::new(),
        }
    }
}

impl Iterator for LogIterator {
    type Item = LogData;

    fn next(&mut self) -> Option<Self::Item> {
        self.line.clear();
        while self.file.read_line(&mut self.line).unwrap() > 0 {
            let result = LogData::from_line(&self.line);
            if result.is_none() {
                self.line.clear();
                continue;
            }
            return result;
        }
        None
    }
}
