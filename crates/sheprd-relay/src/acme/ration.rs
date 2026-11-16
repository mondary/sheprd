//! How often the relay orders certificates: Let's Encrypt allows 50 new
//! ones per registered domain a week, across every relay on it, and a
//! computer that loops must not spend them. In memory, so a restart resets
//! it; Let's Encrypt's own limits stay the hard stop.

use super::error::Error;
use crate::RouteId;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

/// The least time between two certificates for one computer.
pub const PER_COMPUTER: Duration = Duration::from_secs(3 * 24 * 60 * 60);
/// The least time between two attempts for one computer, so a failing
/// order cannot be retried into Let's Encrypt's failed-validation limit.
pub const RETRY: Duration = Duration::from_secs(10 * 60);
/// The window the weekly allowance counts over.
const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Computers remembered at once; the oldest are forgotten past it.
const MAX_REMEMBERED: usize = 10_000;

/// The relay's certificate allowance.
#[derive(Debug)]
pub struct Ration {
    /// Each computer's last attempt, and whether it got a certificate.
    computers: HashMap<RouteId, (Instant, bool)>,
    /// When each certificate this week was issued.
    issued: VecDeque<Instant>,
    weekly: usize,
}

impl Ration {
    /// At most `weekly` certificates a week from this relay.
    #[must_use]
    pub fn new(weekly: usize) -> Self {
        Self {
            computers: HashMap::new(),
            issued: VecDeque::new(),
            weekly,
        }
    }

    /// Takes an attempt for `route` at `now`, or says why not.
    pub fn attempt(&mut self, route: RouteId, now: Instant) -> Result<(), Error> {
        while self
            .issued
            .front()
            .is_some_and(|issued| now.duration_since(*issued) >= WEEK)
        {
            self.issued.pop_front();
        }
        if let Some((last, issued)) = self.computers.get(&route) {
            let wait = if *issued { PER_COMPUTER } else { RETRY };
            if now.duration_since(*last) < wait {
                return Err(Error::Rationed);
            }
        }
        if self.issued.len() >= self.weekly {
            return Err(Error::Weekly);
        }
        if self.computers.len() >= MAX_REMEMBERED && !self.computers.contains_key(&route) {
            self.forget_oldest();
        }
        self.computers.insert(route, (now, false));
        Ok(())
    }

    /// Records that `route`'s attempt at `now` got a certificate.
    pub fn issued(&mut self, route: RouteId, now: Instant) {
        self.computers.insert(route, (now, true));
        self.issued.push_back(now);
    }

    fn forget_oldest(&mut self) {
        let oldest = self
            .computers
            .iter()
            .min_by_key(|(_, (last, _))| *last)
            .map(|(route, _)| *route);
        if let Some(route) = oldest {
            self.computers.remove(&route);
        }
    }
}
