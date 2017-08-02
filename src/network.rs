use std::collections::BTreeMap;
use std::mem;
use message::Message;
use name::Name;

use random::do_with_probability;

/// Network model with synchronous, in-order delivery.
pub struct Network {
    /// Maximum delay in steps before a message is guaranteed to have been delivered.
    max_delay: u64,
    /// Probability that a message is delivered on a given step.
    prob_deliver: f64,
    /// Map from a connection between two nodes and step # to messages inserted at that step.
    messages: BTreeMap<(Name, Name), BTreeMap<u64, Vec<Message>>>,
}

impl Network {
    pub fn new(max_delay: u64) -> Self {
        Network {
            max_delay,
            prob_deliver: Self::delivery_probability(max_delay),
            messages: BTreeMap::new(),
        }
    }

    fn delivery_probability(max_delay: u64) -> f64 {
        // Probability that a message won't be delivered by the randomised delivery
        // after `max_delay` tries.
        let p_drop = 0.05_f64;

        // Compute probability of success, p, to use for each trial by solving:
        // p_drop = (1 - p)^max_delay
        1.0 - p_drop.powf(1.0 / max_delay as f64)

        // NOTE: The above calculation is no longer valid with in-order delivery because
        // the delivery of previous messages effects the delivery of later messages.
        // It's probably a good-enough approximation for now however.
    }

    /// Get messages delivered at the given step (randomised).
    pub fn receive(&mut self, step: u64) -> Vec<Message> {
        let start_step = step.saturating_sub(self.max_delay);
        let prob_deliver = self.prob_deliver;
        let max_delay = self.max_delay;

        self.messages
            .values_mut()
            .flat_map(|messages| {
                Self::receive_from_conn(messages, prob_deliver, max_delay, start_step, step)
            })
            .collect()
    }

    /// Get messages delivered on a single connection at a given step.
    ///
    /// `conn_messages`: the messages for a single connection as contained in `self.messages`.
    fn receive_from_conn(
        conn_messages: &mut BTreeMap<u64, Vec<Message>>,
        prob_deliver: f64,
        max_delay: u64,
        start_step: u64,
        end_step: u64,
    ) -> Vec<Message> {
        let mut all_deliver = vec![];

        // Check that old messages which should have been delivered, have been.
        let num_undelivered: usize = conn_messages
            .range(..start_step)
            .map(|(_, m)| m.len())
            .sum();
        debug_assert_eq!(num_undelivered, 0);

        for (step_sent, messages) in conn_messages.range_mut(start_step..end_step) {
            // Deliver the messages if they were sent at the start step.
            if *step_sent == start_step && end_step >= max_delay {
                all_deliver.extend(messages.drain(..));
                continue;
            }

            let num_messages = messages.len();
            let num_delivered = (1..messages.len() + 1)
                .take_while(|_| do_with_probability(prob_deliver))
                .last()
                .unwrap_or(0);

            let leave = messages.split_off(num_messages - num_delivered);
            let deliver = mem::replace(messages, leave);

            all_deliver.extend(deliver);

            // If messages remain at this step, we can't deliver anything sent on a later step,
            // so return.
            if !messages.is_empty() {
                break;
            }
        }

        all_deliver
    }

    /// Send messages at the given step.
    pub fn send(&mut self, step: u64, messages: Vec<Message>) {
        let mut msg_counts = BTreeMap::new();
        for message in messages {
            let count = msg_counts.entry(message.sender).or_insert(0);
            *count += 1;
            let conn_messages = self.messages
                .entry((message.sender, message.recipient))
                .or_insert_with(BTreeMap::new);
            let step_messages = conn_messages.entry(step).or_insert_with(Vec::new);
            step_messages.push(message);
        }
        for (name, count) in msg_counts {
            trace!("Network: sent {} messages from {}", count, name);
        }
    }

    /// Whether the message/event queue is empty.
    pub fn queue_is_empty(&self) -> bool {
        self.messages.values().flat_map(BTreeMap::values).all(
            Vec::is_empty,
        )
    }

    /// Get the number of messages still in queue
    pub fn messages_in_queue(&self) -> usize {
        self.messages
            .values()
            .flat_map(BTreeMap::values)
            .map(Vec::len)
            .sum()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use message::MessageContent;
    use message::MessageContent::*;

    fn test_message(content: MessageContent) -> Message {
        Message {
            sender: Name(0),
            recipient: Name(1),
            content,
        }
    }

    #[test]
    fn in_order_delivery_diff_step() {
        let connect = test_message(Connect);
        let disconnect = test_message(Disconnect);

        let conn_messages = btreemap! {
            50 => vec![connect.clone()],
            51 => vec![disconnect.clone()],
        };
        let max_delay = 20;
        let start_step = 45;
        let end_step = start_step + max_delay;
        let prob_deliver = 0.5;

        for _ in 0..50 {
            let mut conn_messages = conn_messages.clone();

            let delivered = Network::receive_from_conn(
                &mut conn_messages,
                prob_deliver,
                max_delay,
                start_step,
                end_step,
            );

            // Check that the disconnect isn't delivered before the connect.
            if let Some(first_message) = delivered.first() {
                assert_ne!(first_message, &disconnect);
            }
        }
    }

    #[test]
    fn in_order_delivery_same_step() {
        let connect = test_message(Connect);
        let disconnect = test_message(Disconnect);

        let conn_messages = btreemap! {
            50 => vec![connect.clone(), disconnect.clone()],
        };
        let max_delay = 20;
        let start_step = 45;
        let end_step = start_step + max_delay;
        let prob_deliver = 0.5;

        for _ in 0..50 {
            let mut conn_messages = conn_messages.clone();

            let delivered = Network::receive_from_conn(
                &mut conn_messages,
                prob_deliver,
                max_delay,
                start_step,
                end_step,
            );

            // Check that the disconnect isn't delivered before the connect.
            if let Some(first_message) = delivered.first() {
                assert_ne!(first_message, &disconnect);
            }
        }
    }
}
