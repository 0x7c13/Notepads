// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One operation's monotonic window; counters only schedule real clock reads.

use super::{CHECKPOINT_BYTES, CHECKPOINT_EVENTS};
use crate::error::CoreError;
use std::time::{Duration, Instant};

pub(super) struct ElapsedState {
    start: Instant,
    last_elapsed_ns: u64,
    limit_ns: u64,
    bytes_since_read: u64,
    events_since_read: u64,
    #[cfg(test)]
    samples: Option<TestClock>,
}

#[cfg(test)]
struct TestClock {
    values: Vec<u64>,
    next: usize,
    reads: usize,
    shared: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
}

impl ElapsedState {
    pub(super) fn new(limit_ns: u64) -> Self {
        Self {
            start: Instant::now(),
            last_elapsed_ns: 0,
            limit_ns,
            bytes_since_read: 0,
            events_since_read: 0,
            #[cfg(test)]
            samples: None,
        }
    }

    fn actual_elapsed_ns(&mut self) -> Result<u64, CoreError> {
        #[cfg(test)]
        if let Some(samples) = &mut self.samples {
            samples.reads += 1;
            let value = samples.shared.as_ref().map_or_else(
                || samples.values[samples.next],
                |value| value.load(std::sync::atomic::Ordering::Relaxed),
            );
            if samples.next + 1 < samples.values.len() {
                samples.next += 1;
            }
            return Ok(value);
        }
        let duration = Instant::now()
            .checked_duration_since(self.start)
            .ok_or(CoreError::ClockFailure)?;
        elapsed_nanos(duration)
    }

    pub(super) fn read_clock(&mut self) -> Result<(), CoreError> {
        let elapsed_ns = self.actual_elapsed_ns()?;
        // Compare with the previous actual read, not merely the origin.
        if elapsed_ns < self.last_elapsed_ns {
            return Err(CoreError::ClockFailure);
        }
        self.last_elapsed_ns = elapsed_ns;
        if elapsed_ns >= self.limit_ns {
            return Err(CoreError::DeadlineExceeded {
                elapsed_ns,
                limit_ns: self.limit_ns,
            });
        }
        self.bytes_since_read = 0;
        self.events_since_read = 0;
        Ok(())
    }

    pub(super) fn reaches_boundary(&self, bytes: u64, events: u64) -> Result<bool, CoreError> {
        let bytes_left = CHECKPOINT_BYTES
            .checked_sub(self.bytes_since_read)
            .ok_or(CoreError::InternalInvariant)?;
        let events_left = CHECKPOINT_EVENTS
            .checked_sub(self.events_since_read)
            .ok_or(CoreError::InternalInvariant)?;
        let nonzero = bytes != 0 || events != 0;
        Ok(nonzero
            && (bytes_left == 0
                || events_left == 0
                || (bytes != 0 && bytes >= bytes_left)
                || (events != 0 && events >= events_left)))
    }

    pub(super) fn record_charge(&mut self, bytes: u64, events: u64) -> Result<(), CoreError> {
        let next_bytes = self
            .bytes_since_read
            .checked_add(bytes)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let next_events = self
            .events_since_read
            .checked_add(events)
            .ok_or(CoreError::ArithmeticOverflow)?;
        if next_bytes > CHECKPOINT_BYTES || next_events > CHECKPOINT_EVENTS {
            return Err(CoreError::InternalInvariant);
        }
        self.bytes_since_read = next_bytes;
        self.events_since_read = next_events;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn read_count_for_test(&self) -> usize {
        self.samples.as_ref().map_or(0, |samples| samples.reads)
    }

    #[cfg(test)]
    pub(super) fn set_samples(&mut self, values: &[u64]) {
        self.samples = if values.is_empty() {
            None
        } else {
            Some(TestClock {
                values: values.to_vec(),
                next: 0,
                reads: 0,
                shared: None,
            })
        };
    }

    #[cfg(test)]
    pub(super) fn set_shared_sample(
        &mut self,
        value: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) {
        self.set_samples(&[value.load(std::sync::atomic::Ordering::Relaxed)]);
        self.samples.as_mut().unwrap().shared = Some(value);
    }
}

fn elapsed_nanos(duration: Duration) -> Result<u64, CoreError> {
    u64::try_from(duration.as_nanos()).map_err(|_| CoreError::ClockFailure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrepresentable_elapsed_is_a_clock_failure() {
        assert_eq!(elapsed_nanos(Duration::from_nanos(u64::MAX)), Ok(u64::MAX));
        assert_eq!(
            elapsed_nanos(Duration::new(u64::MAX, 0)),
            Err(CoreError::ClockFailure)
        );
    }
}
