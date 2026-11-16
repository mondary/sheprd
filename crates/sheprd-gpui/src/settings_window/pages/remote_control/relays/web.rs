//! Web access through each listed relay (`docs/relay-web-plan.md`): the
//! relay's listen token file, which makes it private, and the switch,
//! which needs one. Turning web access on asks first, in the same words as
//! the window's question for `sheprd remote web enable`
//! ([`crate::remote::web::question`]); turning it off saves at once. While
//! on, the daemon's report shows under it: the web address and its QR, the
//! certificate, and the Certificate Transparency watch, with an alert first.
// i18n: migrated

use super::super::{Edit, SettingsWindow, ui};
use crate::{
    config::remote::Relay,
    direction::Logical,
    i18n::{Localize, fl},
    remote::{Health, Supervisor, qr::Qr, web},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use sheprd_remote_control::control::web::{Transparency, WebStatus};
use std::{cell::RefCell, path::PathBuf, sync::Arc};

/// An example token file, the same in every language.
const TOKEN_PLACEHOLDER: &str = "/path/to/relay-token";
/// The QR's side.
const QR_SIDE: f32 = 128.;
/// How many web addresses keep their QR encoded.
const QR_KEPT: usize = 8;
const DAY_MS: u64 = 86_400_000;

/// Web access on the page, between draws.
#[derive(Default)]
pub(in crate::settings_window) struct WebSettings {
    /// Where a relay's token file is typed.
    token: Option<Entity<SearchInput>>,
    /// The relay whose token file is being typed, by its address.
    pub(in crate::settings_window) editing: Option<String>,
    /// What was typed is not an absolute path, so it was not saved.
    pub(in crate::settings_window) token_invalid: bool,
    /// The relay whose web access waits for confirmation, by its address.
    pub(in crate::settings_window) asking: Option<String>,
    /// Each web address's QR, encoded once rather than on every draw.
    qr: RefCell<Vec<(String, Arc<Qr>)>>,
}

impl WebSettings {
    /// The QR for `url`, encoded the first time it is asked for.
    fn qr(&self, url: &str) -> Option<Arc<Qr>> {
        let mut kept = self.qr.borrow_mut();
        if let Some((_, qr)) = kept.iter().find(|(kept, _)| kept == url) {
            return Some(qr.clone());
        }
        let qr = Arc::new(Qr::encode(url).ok()?);
        if kept.len() >= QR_KEPT {
            kept.remove(0);
        }
        kept.push((url.to_owned(), qr.clone()));
        Some(qr)
    }
}

/// What the page calls a relay: its name, else its address.
fn display(relay: &Relay) -> String {
    match &relay.label.name {
        Some(name) => crate::notifications::safe_text(name, 64),
        None => crate::notifications::safe_text(&relay.endpoint, 200),
    }
}

/// The certificate's line: when it expires, else why there is none.
pub(in crate::settings_window) fn certificate_line(status: &WebStatus, now_ms: u64) -> String {
    match (status.expires_ms, &status.error) {
        (Some(expires), _) if expires <= now_ms => fl!("settings-remote-web-expired"),
        (Some(expires), _) => fl!(
            "settings-remote-web-expires",
            days = (expires - now_ms).div_ceil(DAY_MS)
        ),
        (None, Some(error)) => fl!(
            "settings-remote-web-certificate-failed",
            error = crate::notifications::safe_text(error, 200)
        ),
        (None, None) => fl!("settings-remote-web-certificate-pending"),
    }
}

/// The Certificate Transparency watch's line.
pub(in crate::settings_window) fn transparency_line(
    transparency: &Transparency,
    now_ms: u64,
) -> String {
    let ago = |ms: &u64| crate::ago::text(now_ms.saturating_sub(*ms) / 1000);
    match transparency {
        Transparency::Unchecked => fl!("settings-remote-web-ct-unchecked"),
        Transparency::Clean { checked_ms } => {
            fl!("settings-remote-web-ct-clean", ago = ago(checked_ms))
        }
        Transparency::Failed { error, checked_ms } => fl!(
            "settings-remote-web-ct-failed",
            ago = ago(checked_ms),
            error = crate::notifications::safe_text(error, 200)
        ),
        Transparency::Alert { found_ms, .. } => {
            fl!("settings-remote-web-alert", ago = ago(found_ms))
        }
    }
}

impl SettingsWindow {
    /// The page shows: make the token file field.
    pub(in crate::settings_window) fn open_relay_web(&mut self, cx: &mut Context<Self>) {
        if self.remote.web.token.is_none() {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| input.set_placeholder(TOKEN_PLACEHOLDER, cx));
            self.remote.web.token = Some(input);
        }
    }

    /// The page went away: an edit or a question left open does not stay.
    pub(in crate::settings_window) fn close_relay_web(&mut self) {
        self.remote.web.editing = None;
        self.remote.web.token_invalid = false;
        self.remote.web.asking = None;
    }

    pub(in crate::settings_window) fn relay_web_input(&self) -> Option<&Entity<SearchInput>> {
        self.remote.web.token.as_ref()
    }

    /// The token file's Set or Change: its field opens with the path saved.
    pub(in crate::settings_window) fn edit_relay_token(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(relay) = self.config.remote.relays.get(index) else {
            return;
        };
        let path = relay
            .token_file
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        self.remote.web.editing = Some(relay.endpoint.clone());
        self.remote.web.token_invalid = false;
        if let Some(input) = &self.remote.web.token {
            input.update(cx, |input, cx| input.set_text_selected(&path, cx));
        }
        cx.notify();
    }

    pub(in crate::settings_window) fn cancel_relay_token(&mut self, cx: &mut Context<Self>) {
        self.remote.web.editing = None;
        self.remote.web.token_invalid = false;
        cx.notify();
    }

    /// Save or Enter: the path typed becomes the relay's token file, and an
    /// empty field takes it off, with web access, which needs one. What is
    /// not an absolute path is not saved.
    pub(in crate::settings_window) fn save_relay_token(&mut self, cx: &mut Context<Self>) {
        let (Some(endpoint), Some(input)) = (
            self.remote.web.editing.clone(),
            self.remote.web.token.clone(),
        ) else {
            return;
        };
        let text = input.read(cx).text().trim().to_owned();
        let token_file = (!text.is_empty()).then(|| PathBuf::from(&text));
        if token_file
            .as_ref()
            .is_some_and(|path| !path.is_absolute() || path.to_str().is_none())
        {
            self.remote.web.token_invalid = true;
            cx.notify();
            return;
        }
        self.remote.web.editing = None;
        self.remote.web.token_invalid = false;
        let relays = self
            .config
            .remote
            .relays
            .iter()
            .map(|relay| {
                if relay.endpoint != endpoint {
                    return relay.clone();
                }
                Relay {
                    web: relay.web && token_file.is_some(),
                    token_file: token_file.clone(),
                    ..relay.clone()
                }
            })
            .collect();
        self.save_remote(Edit::Relays(relays), cx);
        cx.notify();
    }

    /// The web access switch: on asks first, off saves at once.
    pub(in crate::settings_window) fn toggle_relay_web(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(relay) = self.config.remote.relays.get(index) else {
            return;
        };
        if !relay.web {
            return self.ask_relay_web(index, cx);
        }
        if let Some(relays) = web::turn_off(&self.config.remote.relays, &relay.endpoint) {
            self.save_remote(Edit::Relays(relays), cx);
        }
        cx.notify();
    }

    /// Opens the question for turning web access on, or on again after an
    /// alert, through the relay at `index`.
    pub(in crate::settings_window) fn ask_relay_web(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(relay) = self.config.remote.relays.get(index) else {
            return;
        };
        if relay.token_file.is_none() {
            return;
        }
        self.remote.web.asking = Some(relay.endpoint.clone());
        cx.notify();
    }

    pub(in crate::settings_window) fn cancel_relay_web(&mut self, cx: &mut Context<Self>) {
        self.remote.web.asking = None;
        cx.notify();
    }

    /// The question's Turn On: resumes web access at the daemon after an
    /// alert, and saves `[remote] relays` with it on.
    pub(in crate::settings_window) fn confirm_relay_web(&mut self, cx: &mut Context<Self>) {
        let Some(endpoint) = self.remote.web.asking.take() else {
            return;
        };
        let name = self
            .config
            .remote
            .relays
            .iter()
            .find(|relay| relay.endpoint == endpoint)
            .map_or_else(|| endpoint.clone(), display);
        let turn = web::turn_on(
            &self.config.remote.relays,
            &endpoint,
            &Supervisor::health(cx),
        );
        if let Some(relay) = turn.resume {
            let view = cx.entity().downgrade();
            Supervisor::resume_web(cx, relay, move |result, cx| {
                let Err(error) = result else {
                    return;
                };
                let _ = view.update(cx, |view, cx| {
                    view.error = Some(fl!(
                        "remote-web-failed",
                        relay = name,
                        error = error.localize()
                    ));
                    cx.notify();
                });
            });
        }
        if let Some(relays) = turn.relays {
            self.save_remote(Edit::Relays(relays), cx);
        }
        cx.notify();
    }

    /// The relay at `index`'s token file and web access, under its row.
    pub(in crate::settings_window) fn relay_web(
        &self,
        index: usize,
        relay: &Relay,
        health: &Health,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let idle = !self.busy();
        let private = relay.token_file.is_some();
        let status = web::web_status(health, &relay.endpoint);
        let alert =
            status.is_some_and(|status| matches!(status.transparency, Transparency::Alert { .. }));
        let toggle = ui::toggle(
            theme,
            format!("remote-relay-{index}-web-toggle"),
            relay.web,
            idle && private,
        )
        .when(idle && private, |toggle| {
            toggle.on_click(cx.listener(move |this, _, _, cx| this.toggle_relay_web(index, cx)))
        });
        let description = match (private, relay.web, status) {
            (false, _, _) => fl!("settings-remote-web-needs-token"),
            (true, false, _) => fl!("settings-remote-web-off"),
            // On in the config, but the daemon keeps it off until resumed.
            (true, true, _) if alert => fl!("settings-remote-web-alerted"),
            (true, true, Some(WebStatus { url: Some(_), .. })) => {
                fl!("settings-remote-web-on")
            }
            (true, true, _) => fl!("settings-remote-web-starting"),
        };
        let asking = self.remote.web.asking.as_deref() == Some(relay.endpoint.as_str());
        div()
            .flex()
            .flex_col()
            .pl(px(16.))
            .child(self.relay_token(index, relay, cx))
            .child(
                ui::row(
                    theme,
                    format!("remote-relay-{index}-web"),
                    fl!("settings-remote-web"),
                )
                .description(description)
                .switch(toggle),
            )
            .when(asking, |block| {
                block.child(self.relay_web_question(index, &display(relay), alert, cx))
            })
            .when_some(status.filter(|_| relay.web && !asking), |block, status| {
                block.child(self.relay_web_details(index, status, alert, cx))
            })
    }

    /// The token file's row: the path saved, or its field while typed.
    fn relay_token(&self, index: usize, relay: &Relay, cx: &mut Context<Self>) -> ui::Row {
        let theme = &self.theme;
        let idle = !self.busy();
        let row = ui::row(
            theme,
            format!("remote-relay-{index}-token"),
            fl!("settings-remote-web-token"),
        );
        let editing = self.remote.web.editing.as_deref() == Some(relay.endpoint.as_str());
        if !editing {
            let (description, label) = match &relay.token_file {
                Some(path) => (
                    crate::notifications::safe_text(&path.display().to_string(), 200),
                    fl!("settings-remote-web-token-change"),
                ),
                None => (
                    fl!("settings-remote-web-token-none"),
                    fl!("settings-remote-web-token-set"),
                ),
            };
            return row.description(description).control(
                ui::button(
                    theme,
                    format!("remote-relay-{index}-token-edit"),
                    label,
                    idle,
                )
                .when(idle, |button| {
                    button.on_click(
                        cx.listener(move |this, _, _, cx| this.edit_relay_token(index, cx)),
                    )
                }),
            );
        }
        let field = div()
            .debug_selector(move || format!("remote-relay-{index}-token-path"))
            .w(px(260.))
            .flex_none()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "enter" => this.save_relay_token(cx),
                    "escape" => this.cancel_relay_token(cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .children(self.remote.web.token.clone());
        row.description(if self.remote.web.token_invalid {
            fl!(
                "settings-remote-web-token-invalid",
                example = TOKEN_PLACEHOLDER
            )
        } else {
            fl!(
                "settings-remote-web-token-note",
                example = TOKEN_PLACEHOLDER
            )
        })
        .control(crate::motion::pop_in(
            field,
            ElementId::Name(format!("remote-relay-{index}-token-field").into()),
        ))
        .control(
            ui::button(
                theme,
                format!("remote-relay-{index}-token-cancel"),
                fl!("settings-remote-web-token-cancel"),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| this.cancel_relay_token(cx))),
        )
        .control(
            ui::button(
                theme,
                format!("remote-relay-{index}-token-save"),
                fl!("settings-remote-web-token-save"),
                idle,
            )
            .when(idle, |button| {
                button.on_click(cx.listener(|this, _, _, cx| this.save_relay_token(cx)))
            }),
        )
    }

    /// The question before web access goes on, in the window's words.
    fn relay_web_question(
        &self,
        index: usize,
        name: &str,
        alert: bool,
        cx: &mut Context<Self>,
    ) -> AnimationElement<Div> {
        let theme = &self.theme;
        let (title, lines) = web::question(name, alert);
        let card = div()
            .debug_selector(move || format!("remote-relay-{index}-web-ask"))
            .flex()
            .flex_col()
            .gap(px(8.))
            .my(px(6.))
            .p(px(12.))
            .rounded(px(crate::config::corners::PANEL))
            .bg(rgb(theme.primary_wash()))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .children(
                lines
                    .into_iter()
                    .map(|line| div().text_color(rgb(theme.subtext())).child(line)),
            )
            .child(
                div()
                    .row()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ui::button(
                            theme,
                            format!("remote-relay-{index}-web-cancel"),
                            fl!("remote-web-cancel"),
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_relay_web(cx))),
                    )
                    .child(
                        ui::button(
                            theme,
                            format!("remote-relay-{index}-web-confirm"),
                            fl!("remote-web-submit"),
                            !self.busy(),
                        )
                        .when(!self.busy(), |button| {
                            button
                                .on_click(cx.listener(|this, _, _, cx| this.confirm_relay_web(cx)))
                        }),
                    ),
            );
        crate::motion::pop_in(
            card,
            ElementId::Name(format!("remote-relay-{index}-web-ask").into()),
        )
    }

    /// What the daemon reports while web access is on: an alert first, then
    /// the address and its QR, the certificate, and the watch.
    fn relay_web_details(
        &self,
        index: usize,
        status: &WebStatus,
        alert: bool,
        cx: &mut Context<Self>,
    ) -> AnimationElement<Div> {
        let theme = &self.theme;
        let now = crate::remote::now_ms();
        let idle = !self.busy();
        let line = |suffix: &'static str, text: String| {
            div()
                .debug_selector(move || format!("remote-relay-{index}-web-{suffix}"))
                .text_color(rgb(theme.subtext()))
                .child(text)
        };
        let url = status
            .url
            .as_deref()
            .map(|url| crate::notifications::safe_text(url, 200));
        let qr = status
            .url
            .as_deref()
            .and_then(|url| self.remote.web.qr(url));
        let alert_block = alert.then(|| {
            div()
                .debug_selector(move || format!("remote-relay-{index}-web-alert"))
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(8.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .border_color(crate::menu::danger(theme))
                .text_color(crate::menu::danger(theme))
                .font_weight(FontWeight::MEDIUM)
                .child(transparency_line(&status.transparency, now))
                .child(
                    div().row().child(
                        ui::button(
                            theme,
                            format!("remote-relay-{index}-web-resume"),
                            fl!("settings-remote-web-resume"),
                            idle,
                        )
                        .when(idle, |button| {
                            button.on_click(
                                cx.listener(move |this, _, _, cx| this.ask_relay_web(index, cx)),
                            )
                        }),
                    ),
                )
        });
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .children(url.map(|url| {
                div()
                    .debug_selector(move || format!("remote-relay-{index}-web-url"))
                    .font_weight(FontWeight::MEDIUM)
                    .child(fl!("settings-remote-web-url", url = url))
            }))
            .child(line("certificate", certificate_line(status, now)))
            .when(!alert, |text| {
                text.child(line(
                    "transparency",
                    transparency_line(&status.transparency, now),
                ))
            });
        // After an alert, the address is off: no code invites a phone to it.
        let card = div()
            .debug_selector(move || format!("remote-relay-{index}-web-details"))
            .flex()
            .flex_col()
            .gap(px(8.))
            .my(px(6.))
            .children(alert_block)
            .when(!alert, |card| {
                card.child(
                    div()
                        .row()
                        .items_start()
                        .gap(px(12.))
                        .children(qr.map(|qr| {
                            div()
                                .debug_selector(move || format!("remote-relay-{index}-web-qr"))
                                .flex_none()
                                .child(crate::remote::qr_square(qr, QR_SIDE))
                        }))
                        .child(text),
                )
            });
        crate::motion::pop_in(
            card,
            ElementId::Name(format!("remote-relay-{index}-web-details").into()),
        )
    }
}
