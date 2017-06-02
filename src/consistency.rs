use name::Name;
use node::Node;
use std::collections::{BTreeMap, BTreeSet};
use itertools::Itertools;

/// Check that all the nodes have a consistent view of the network.
pub fn check_consistency(nodes: &BTreeMap<Name, Node>, min_section_size: usize) -> Result<(), ()> {
    let mut sections = btreemap!{};

    for node in nodes.values() {
        for block in &node.current_blocks {
            let section_versions = sections
                .entry(block.prefix)
                .or_insert_with(BTreeSet::new);
            section_versions.insert(block.clone());
        }
    }

    let mut failed = false;

    for (prefix, versions) in &sections {
        if versions.len() > 1 {
            failed = true;
            error!("multiple versions of {:?}, they are: {:#?}",
                   prefix,
                   versions);
        } else {
            let members = &versions.iter().next().unwrap().members;
            // Allow a quorum if we have only one section, otherwise require `min_section_size`.
            if (sections.len() == 1 && members.len() * 2 <= min_section_size) ||
               (sections.len() > 1 && members.len() < min_section_size) {
                failed = true;
                error!("section too small: {:?} with members {:?}", prefix, members);
            }
        }
    }

    for (p1, p2) in sections.keys().tuple_combinations() {
        if p1.is_compatible(p2) {
            failed = true;
            error!("prefixes {:?} and {:?} overlap", p1, p2);
        }
    }

    if failed {
        error!("network not consistent: see above");
        Err(())
    } else {
        info!("network is consistent!");
        Ok(())
    }
}
