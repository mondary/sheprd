//! Web access (`docs/relay-web-plan.md`): browsers reaching a computer at
//! its web name, `<label>.<relay host>`, through the TCP front. A listener
//! turns it on; each browser connection is then a dial with no key, held
//! to the route's web limit beside every limit a dial has.

use super::{Caller, DialRefusal, Hub, Waiting};
use crate::{RouteId, SessionId, WebLabel};
use std::{net::IpAddr, sync::Arc};

impl Hub {
    /// Turns web access on or off for `route`'s listener `generation`.
    /// Off ends the route's web sessions. `false` when the relay offers no
    /// web access, the listener is gone, or another route already holds
    /// the same label.
    pub(super) fn set_web(&self, route: RouteId, generation: u64, on: bool) -> bool {
        if on && !self.web {
            return false;
        }
        let label = WebLabel::of(&route);
        let mut state = self.state();
        let taken = state.web.get(&label).is_some_and(|held| *held != route);
        let Some(listener) = state.listeners.get_mut(&route) else {
            return false;
        };
        if listener.generation != generation {
            return false;
        }
        if listener.web == on {
            return true;
        }
        if on {
            if taken {
                return false;
            }
            listener.web = true;
            state.web.insert(label, route);
            return true;
        }
        listener.web = false;
        state.web.remove(&label);
        state.end_web_sessions(&route);
        true
    }

    /// Whether `route` has a listener with web access on.
    #[must_use]
    pub fn web_on(&self, route: &RouteId) -> bool {
        self.web
            && self
                .state()
                .listeners
                .get(route)
                .is_some_and(|listener| listener.web)
    }

    /// Tells the listener at `label` that a browser from `dialer` connects,
    /// holding a slot for it, if that listener has web access on. Refused
    /// as `Offline` otherwise, exactly as a name no one holds.
    pub fn dial_web(
        self: &Arc<Self>,
        label: WebLabel,
        dialer: IpAddr,
    ) -> crate::Result<Result<Waiting, DialRefusal>> {
        if !self.web {
            return Ok(Err(DialRefusal::Offline));
        }
        let session = SessionId::random()?;
        let mut state = self.state();
        let Some(route) = state.web.get(&label).copied() else {
            return Ok(Err(DialRefusal::Offline));
        };
        if !state
            .listeners
            .get(&route)
            .is_some_and(|listener| listener.web)
        {
            return Ok(Err(DialRefusal::Offline));
        }
        if state.web_routes.get(&route) >= self.limits.max_web_sessions_per_route {
            return Ok(Err(DialRefusal::Busy));
        }
        let waiting = self.wait(&mut state, route, Caller::Web, dialer, session)?;
        if waiting.is_ok() {
            state.web_routes.add(route);
        }
        drop(state);
        Ok(waiting)
    }
}
