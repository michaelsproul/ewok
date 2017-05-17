use std::collections::BTreeMap;
use message::Message;

use random::random;

pub struct Network {
    /// Maximum delay in steps before a message is guaranteed to have been delivered.
    max_delay: u64,
    /// Probability that a message is delivered on a given step.
    prob_deliver: f64,
    /// Map from step # to messages inserted at that step.
    messages: BTreeMap<u64, Vec<Message>>
}

impl Network {
    pub fn new(max_delay: u64) -> Self {
        Network {
            max_delay,
            prob_deliver: Self::delivery_probability(max_delay),
            messages: BTreeMap::new()
        }
    }

    fn lower_bound(&self, step: u64) -> u64 {
        if step >= self.max_delay {
            step - self.max_delay
        } else {
            0
        }
    }

    fn delivery_probability(max_delay: u64) -> f64 {
        // Probability that a message won't be delivered by the randomised delivery
        // after `max_delay` tries.
        let p_drop = 0.05_f64;

        // Compute probability of success, p, to use for each trial by solving:
        // p_drop = (1 - p)^max_delay
        1.0 - p_drop.powf(1.0 / max_delay as f64)
    }

    /// Get messages delivered at the given step (randomised).
    pub fn receive(&mut self, step: u64) -> Vec<Message> {
        let start_step = self.lower_bound(step);

        let prob_deliver = self.prob_deliver;

        self.messages.range_mut(start_step..step)
            .flat_map(|(&step_sent, messages)| {
                // Partition randomly based on p, whilst also delivering any messages
                // which were sent at start step.
                // This means messages sent in step 0 are instantly delivered
                // but that's probably fine.
                let (deliver, leave) = messages.drain(..).partition(|_| {
                    let deliver_random = random::<f64>() <= prob_deliver;
                    let force_deliver = step_sent == start_step;
                    force_deliver || deliver_random
                });

                *messages = leave;
                deliver
            })
            .collect()
    }

    /// Send messages at the given step.
    pub fn send(&mut self, step: u64, messages: Vec<Message>) {
        let step_messages = self.messages.entry(step).or_insert_with(Vec::new);
        step_messages.extend(messages);
    }
}
