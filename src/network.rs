use std::collections::BTreeMap;
use message::Event;
use name::Name;

use random::do_with_probability;

/// Network model with synchronous, in-order delivery.
pub struct Network {
    /// Maximum delay in steps before a message is guaranteed to have been delivered.
    max_delay: u64,
    /// Probability that a message is delivered on a given step.
    prob_deliver: f64,
    /// Map from a connection between two nodes and step # to messages inserted at that step.
    messages: BTreeMap<(Name, Name), BTreeMap<u64, Vec<Event>>>,
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

        // TODO: the above calculation is no longer valid with in-order delivery because
        // the delivery of previous messages effects the delivery of later messages.
        // It's probably a good-enough approximation for now however.
    }

    /// Get messages delivered at the given step (randomised).
    pub fn receive(&mut self, step: u64) -> Vec<Event> {
        let start_step = step.saturating_sub(self.max_delay);
        let prob_deliver = self.prob_deliver;

        self.messages
            .values_mut()
            .flat_map(|messages| Self::receive_from_conn(messages, prob_deliver, start_step, step))
            .collect()
    }

    /// Get messages delivered on a single connection at a given step.
    ///
    /// `conn_messages`: the messages for a single connection as contained in `self.messages`.
    fn receive_from_conn(conn_messages: &mut BTreeMap<u64, Vec<Event>>,
                         prob_deliver: f64,
                         start_step: u64,
                         end_step: u64)
                         -> Vec<Event> {
        let mut all_deliver = vec![];

        // Check that old messages which should have been delivered, have been.
        let num_undelivered: usize = conn_messages
            .range(..start_step)
            .map(|(_, m)| m.len())
            .sum();
        debug_assert_eq!(num_undelivered, 0);

        for (step_sent, messages) in conn_messages.range_mut(start_step..end_step) {
            // Partition randomly based on p, whilst also delivering any messages
            // which were sent at start step.
            let (deliver, leave) = messages
                .drain(..)
                .partition(|_| {
                               let deliver_random = do_with_probability(prob_deliver);
                               let force_deliver = *step_sent == start_step;
                               force_deliver || deliver_random
                           });

            all_deliver.extend(deliver);
            *messages = leave;

            // If messages remain at this step, we can't deliver anything sent on a later step,
            // so return.
            if !messages.is_empty() {
                break;
            }
        }

        all_deliver
    }

    /// Send messages at the given step.
    pub fn send(&mut self, step: u64, events: Vec<Event>) {
        for event in events {
            let conn_messages = self.messages
                .entry((event.src, event.dst))
                .or_insert_with(BTreeMap::new);
            let step_messages = conn_messages.entry(step).or_insert_with(Vec::new);
            step_messages.push(event);
        }
    }
}
