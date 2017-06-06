use name::{Name, Prefix};
use node::Node;
use message::Message;
use message::MessageContent::*;
use std::collections::BTreeMap;
use self::Event::*;

#[derive(Clone, Debug)]
pub enum Event {
    AddNode(Name),
    RemoveNode(Name),
    RemoveNodeFrom(Prefix),
    //Reconnect(Name, Name)
    //Disconnect(Name, Name)
}

impl Event {
    /// Convert the event into a vec of notifications for all the nodes it should be sent to.
    pub fn broadcast(&self, nodes: &BTreeMap<Name, Node>) -> Vec<Message> {
        match *self {
            AddNode(name) => add_node(name, nodes),
            RemoveNode(name) => remove_node(name, nodes),
            RemoveNodeFrom(_) => panic!("you need to normalise events before broadcasting"),
        }
    }

    /// If this is an event about a prefix, transform it into an event about a specific node.
    pub fn normalise(self, nodes: &BTreeMap<Name, Node>) -> Option<Self> {
        if let RemoveNodeFrom(prefix) = self {
            select_node_to_remove(prefix, nodes).map(RemoveNode)
        } else {
            Some(self)
        }
    }
}

fn add_node(joining_node: Name, nodes: &BTreeMap<Name, Node>) -> Vec<Message> {
    // TODO: send only to this node's section(s).
    nodes
        .iter()
        .map(|(&neighbour, _)| {
                 Message {
                     sender: joining_node,
                     recipient: neighbour,
                     content: NodeJoined,
                 }
             })
        .collect()
}

fn select_node_to_remove(prefix: Prefix, nodes: &BTreeMap<Name, Node>) -> Option<Name> {
    nodes
        .iter()
        .find(move |&(name, _)| prefix.matches(*name))
        .map(|(name, _)| *name)
}

fn remove_node(to_remove: Name, nodes: &BTreeMap<Name, Node>) -> Vec<Message> {
    // TODO: only send to this node's connected peers.
    // TODO: consider connections again?
    nodes
        .iter()
        .map(|(&neighbour, _)| {
                 Message {
                     sender: to_remove,
                     recipient: neighbour,
                     content: ConnectionLost,
                 }
             })
        .collect()
}
