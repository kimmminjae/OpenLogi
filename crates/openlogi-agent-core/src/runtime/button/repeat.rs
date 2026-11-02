//! Steady repeat schedule owned by one active physical press.

use openlogi_core::config::ButtonRepeatRate;
use std::time::{Duration, Instant};

const INITIAL_DELAY: Duration = Duration::from_millis(400);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RepeatSchedule {
    deadline: Instant,
    interval: Duration,
}

impl RepeatSchedule {
    pub(super) fn new(pressed_at: Instant, rate: ButtonRepeatRate) -> Self {
        Self {
            deadline: pressed_at + INITIAL_DELAY,
            interval: rate.interval(),
        }
    }

    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn fire_due(&mut self, now: Instant) -> bool {
        if now < self.deadline {
            return false;
        }
        // Never catch up with a burst after a blocked handler or system sleep.
        self.deadline = now + self.interval;
        true
    }
}
