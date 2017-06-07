use std::collections::BTreeSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

fn hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Members(pub BTreeSet<String>);

impl fmt::Display for Members {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (count, name) in self.0.iter().enumerate() {
            write!(f, "<font color=\"#{}\">{}</font>, ", name, name)?;
            if count % 3 == 0 {
                write!(f, "<br/>")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block {
    pub prefix: String,
    pub version: u64,
    pub members: Members,
}

impl Block {
    pub fn get_id(&self) -> String {
        format!("prefix{}_v{}_{}",
                self.prefix,
                self.version,
                hash(&self.members))
    }

    pub fn get_label(&self) -> String {
        format!("<<font point-size=\"40\">p[{}] v{}</font><br/>Members: <br/>{}>",
                self.prefix,
                self.version,
                self.members)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vote {
    pub from: String,
    pub to: String,
}
