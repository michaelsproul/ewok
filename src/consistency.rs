use name::Name;
use node::Node::{self, Active};
use std::collections::{BTreeMap, BTreeSet};
use itertools::Itertools;

/// Check that all the nodes have a consistent view of the network.
pub fn check_consistency(nodes: &BTreeMap<Name, Node>) -> Result<(), ()> {
    let mut sections = btreemap!{};

    for node in nodes.values() {
        if let Active(ref active_node) = *node {
            for block in &active_node.current_blocks {
                let section_versions = sections
                    .entry(block.prefix)
                    .or_insert_with(BTreeSet::new);
                section_versions.insert(block.clone());
            }
        }
    }

    let mut failed = false;

    for (prefix, versions) in &sections {
        if versions.len() > 1 {
            failed = true;
            println!("multiple versions of {:?}, they are: {:#?}",
                     prefix,
                     versions);
        }
    }

    for (p1, p2) in sections.keys().tuple_combinations() {
        if p1.is_compatible(*p2) {
            failed = true;
            println!("prefixes {:?} and {:?} overlap", p1, p2);
        }
    }

    if failed {
        println!("network not consistent: see above");
        Err(())
    } else {
        println!("network is consistent!");
        Ok(())
    }
}
