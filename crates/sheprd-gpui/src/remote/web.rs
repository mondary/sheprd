//! Web access through a private relay, Sheprd's side
//! (`docs/relay-web-plan.md`): browsers open `https://<label>.<relay host>`
//! and reach this computer's sign-in page there. Turning it on, or on again
//! after the Certificate Transparency watch saw a certificate with another
//! key, always asks first, as `sheprd remote web enable` and Settings do;
//! the question's words are [`question`] in both. Turning it off acts at
//! once.
// i18n: migrated

use super::{
    Health,
    dialog::{Tone, button},
    supervisor::Supervisor,
};
use crate::{
    SheprdWindow,
    config::{
        Config,
        preferences::Preference,
        remote::{Edit, Relay, relay_address},
    },
    i18n::{Localize, fl},
    menu::Page,
};
use gpui::{prelude::*, *};
use sheprd_remote_control::control::{
    self, RelayStatus, Request, Status,
    web::{Transparency, WebStatus},
};

/// Whether `a` and `b` name the same relay, however either is spelled.
pub(crate) fn same_relay(a: &str, b: &str) -> bool {
    a == b
        || matches!(
            (relay_address(a), relay_address(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// What the daemon reports for the relay at `endpoint`, with the name it
/// goes by there. The daemon names a relay as `relay_address` writes it; a
/// list edited by hand may spell it another way.
pub(crate) fn reported<'a>(
    status: &'a Status,
    endpoint: &str,
) -> Option<(&'a String, &'a RelayStatus)> {
    status.relays.get_key_value(endpoint).or_else(|| {
        relay_address(endpoint)
            .ok()
            .and_then(|address| status.relays.get_key_value(&address))
    })
}

/// Web access through `endpoint`, as the running daemon reports it.
pub(crate) fn web_status<'a>(health: &'a Health, endpoint: &str) -> Option<&'a WebStatus> {
    let Health::Running(status) = health else {
        return None;
    };
    reported(status, endpoint).and_then(|(_, relay)| relay.web.as_ref())
}

/// The daemon's name for the relay at `endpoint` when a Certificate
/// Transparency alert turned web access off there.
pub(crate) fn alerted(health: &Health, endpoint: &str) -> Option<String> {
    let Health::Running(status) = health else {
        return None;
    };
    let (name, relay) = reported(status, endpoint)?;
    matches!(
        relay.web.as_ref().map(|web| &web.transparency),
        Some(Transparency::Alert { .. })
    )
    .then(|| name.clone())
}

/// What turning web access on through a relay takes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TurnOn {
    /// The daemon's name for the relay, when an alert keeps web access off
    /// there until it is told to resume.
    pub(crate) resume: Option<String>,
    /// The list to save, when `[remote] relays` has web access off there.
    pub(crate) relays: Option<Vec<Relay>>,
}

impl TurnOn {
    /// Whether it is on already, with nothing to do.
    pub(crate) fn is_empty(&self) -> bool {
        self.resume.is_none() && self.relays.is_none()
    }
}

/// What turning web access on through `endpoint` takes, given the list and
/// what the daemon reports. A relay without a token file is never turned
/// on: web access needs a private relay.
pub(crate) fn turn_on(relays: &[Relay], endpoint: &str, health: &Health) -> TurnOn {
    let off = relays.iter().any(|relay| {
        same_relay(&relay.endpoint, endpoint) && relay.token_file.is_some() && !relay.web
    });
    TurnOn {
        resume: alerted(health, endpoint),
        relays: off.then(|| switched(relays, endpoint, true)),
    }
}

/// The list to save to turn web access off through `endpoint`; `None` when
/// it is off already.
pub(crate) fn turn_off(relays: &[Relay], endpoint: &str) -> Option<Vec<Relay>> {
    relays
        .iter()
        .any(|relay| same_relay(&relay.endpoint, endpoint) && relay.web)
        .then(|| switched(relays, endpoint, false))
}

/// `relays` with web access `on` through `endpoint`; the others unchanged.
fn switched(relays: &[Relay], endpoint: &str, on: bool) -> Vec<Relay> {
    relays
        .iter()
        .map(|relay| Relay {
            web: if same_relay(&relay.endpoint, endpoint) {
                on
            } else {
                relay.web
            },
            ..relay.clone()
        })
        .collect()
}

/// The question before turning web access on through `relay` (its name or
/// address, already made safe to show): a title, then what it means, and,
/// after an alert, what turning it on again trusts.
pub(crate) fn question(relay: &str, alert: bool) -> (String, Vec<String>) {
    let mut lines = vec![fl!("remote-web-detail", relay = relay)];
    if alert {
        lines.push(fl!("remote-web-alert"));
    }
    (fl!("remote-web-title", relay = relay), lines)
}

impl Supervisor {
    /// Tells the daemon to serve web access through `relay` again after an
    /// alert, then refreshes its status. `done` gets the outcome on the UI
    /// thread.
    pub(crate) fn resume_web(
        cx: &mut App,
        relay: String,
        done: impl FnOnce(Result<(), control::Error>, &mut App) + 'static,
    ) {
        let Some(client) = Self::client(cx) else {
            return;
        };
        let task = client.request(Request::ResumeWeb { relay });
        cx.spawn(async move |cx| {
            let result = task.await.map(drop);
            cx.update(|cx| {
                Self::refresh(cx);
                done(result, cx);
            });
        })
        .detach();
    }
}

/// The relay the web access question is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WebAsk {
    /// Its address, as `[remote] relays` lists it.
    pub(crate) endpoint: String,
    /// What the question calls it.
    pub(crate) name: String,
}

impl SheprdWindow {
    /// Asks to turn web access on through a relay, unless another page
    /// holds the window.
    pub(crate) fn ask_web_access(
        &mut self,
        ask: WebAsk,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return false;
        }
        self.menu.remote_web = Some(ask);
        self.menu.page = Some(Page::RemoteWeb);
        cx.notify();
        true
    }

    /// Turns web access on and closes the question: resumes it at the
    /// daemon after an alert, and saves `[remote] relays` with it on. A
    /// failure shows in the status bar.
    pub(crate) fn confirm_web_access(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ask = self.menu.remote_web.take();
        self.dismiss_menu(window, cx);
        let Some(WebAsk { endpoint, name }) = ask else {
            return;
        };
        let turn = turn_on(
            &self.config.remote.relays,
            &endpoint,
            &Supervisor::health(cx),
        );
        if let Some(relay) = turn.resume {
            let this = cx.entity().downgrade();
            Supervisor::resume_web(cx, relay, move |result, cx| {
                let Err(error) = result else {
                    return;
                };
                let _ = this.update(cx, |view, cx| {
                    view.local_error = Some(fl!(
                        "remote-web-failed",
                        relay = name,
                        error = error.localize()
                    ));
                    cx.notify();
                });
            });
        }
        if let Some(relays) = turn.relays {
            self.save_preference(
                move || Config::save_preference(Preference::Remote(Edit::Relays(relays))),
                cx,
            );
        }
    }

    pub(super) fn render_web_access(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(ask) = &self.menu.remote_web else {
            return div();
        };
        let alert = alerted(&Supervisor::health(cx), &ask.endpoint).is_some();
        let (title, lines) = question(&ask.name, alert);
        div()
            .debug_selector(|| "remote-web".into())
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(12.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .children(
                lines
                    .into_iter()
                    .map(|line| div().text_color(rgb(theme.subtext())).child(line)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(
                            theme,
                            "remote-web-cancel",
                            fl!("remote-web-cancel"),
                            Tone::Plain,
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_menu(window, cx);
                        })),
                    )
                    .child(
                        button(
                            theme,
                            "remote-web-submit",
                            fl!("remote-web-submit"),
                            Tone::Primary,
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.confirm_web_access(window, cx);
                        })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests;
