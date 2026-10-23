//! Token counts of the flow engine over time (`TraceEvent::Tokens`).

/// Step functions of the token count per `(flow block, activity)`, built from the
/// `TraceEvent::Tokens` records and queried by binary search.
#[derive(Clone, Debug, Default)]
pub struct TokenTrack {
    steps: std::collections::HashMap<(usize, usize), Vec<(f64, u32)>>,
}

impl TokenTrack {
    /// Records that `(flow, activity)` holds `count` tokens from time `t` (call in time order).
    pub fn push(&mut self, t: f64, flow: usize, activity: usize, count: u32) {
        self.steps
            .entry((flow, activity))
            .or_default()
            .push((t, count));
    }

    /// Tokens in the activity at time `t`; 0 before the first record.
    pub fn at(&self, flow: usize, activity: usize, t: f64) -> u32 {
        let Some(steps) = self.steps.get(&(flow, activity)) else {
            return 0;
        };
        let k = steps.partition_point(|s| s.0 <= t);
        k.checked_sub(1).map_or(0, |k| steps[k].1)
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_track_is_a_step_function() {
        let mut t = TokenTrack::default();
        t.push(1.0, 5, 0, 1);
        t.push(2.0, 5, 0, 3);
        t.push(4.0, 5, 0, 0);
        assert_eq!(t.at(5, 0, 0.5), 0);
        assert_eq!(t.at(5, 0, 1.0), 1);
        assert_eq!(t.at(5, 0, 3.9), 3);
        assert_eq!(t.at(5, 0, 4.0), 0);
        assert_eq!(t.at(5, 1, 3.0), 0);
        assert_eq!(t.at(6, 0, 3.0), 0);
    }
}
