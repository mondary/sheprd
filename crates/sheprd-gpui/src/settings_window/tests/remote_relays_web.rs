//! Settings > Remote control > Relays, web access: the switch needs a
//! token file, turning it on asks first in the window's words, turning it
//! off saves at once, and while on the page shows what the daemon reports,
//! an alert first with a way to turn it on again.
use super::remote_control::{click, draw, page, status};
use super::*;
use crate::{
    config::{
        preferences::Preference,
        remote::{Edit, Relay},
    },
    remote::{Health, Supervisor},
};
use sheprd_remote_control::control::{
    self, RelayState, RelayStatus, Request, Response,
    web::{Transparency, WebStatus},
};
use std::path::{Path, PathBuf};

const HOME: &str = "https://relay.example.org";
// The token file must be absolute, which a Unix path is not on Windows.
#[cfg(not(windows))]
const TOKEN: &str = "/etc/relay/token";
#[cfg(windows)]
const TOKEN: &str = r"C:\sheprd\relay.token";

fn relay(token: bool, web: bool) -> Relay {
    Relay {
        token_file: token.then(|| PathBuf::from(TOKEN)),
        web,
        ..Relay::from(HOME)
    }
}

/// Shows the page with [`HOME`] listed as `relay`, and the daemon
/// reporting web access through it as `transparency` finds it, if at all.
fn with_relay(
    view: &Entity<SettingsWindow>,
    cx: &mut VisualTestContext,
    relay: Relay,
    transparency: Option<Transparency>,
) {
    let mut status = status();
    let mut reported: RelayStatus = RelayState::Listening { rtt_ms: None }.into();
    reported.web = transparency.map(|transparency| WebStatus {
        url: Some("https://k3x9.relay.example.org".into()),
        expires_ms: Some(crate::remote::now_ms() + 30 * 86_400_000),
        error: None,
        transparency,
    });
    status.relays = [(HOME.to_owned(), reported)].into();
    cx.update(|_, cx| Supervisor::set_health(cx, Health::Running(status), None));
    view.update(cx, |view, cx| {
        view.config.remote.relays = vec![relay];
        cx.notify();
    });
    draw(cx);
}

fn relays(edits: &remote_control::Edits) -> Vec<Vec<Relay>> {
    edits
        .lock()
        .unwrap()
        .iter()
        .filter_map(|edit| match edit {
            Preference::Remote(Edit::Relays(relays)) => Some(relays.clone()),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn the_switch_waits_for_a_token_file(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    with_relay(&view, cx, relay(false, false), None);
    for selector in [
        "remote-relay-0-token",
        "remote-relay-0-web",
        "remote-relay-0-web-toggle",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    click(cx, "remote-relay-0-web-toggle");
    view.read_with(cx, |view, _| assert!(view.remote.web.asking.is_none()));
    assert!(cx.debug_bounds("remote-relay-0-web-ask").is_none());
    assert!(edits.lock().unwrap().is_empty());
}

#[gpui::test]
fn a_token_file_must_be_an_absolute_path(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    with_relay(&view, cx, relay(false, false), None);
    click(cx, "remote-relay-0-token-edit");
    click(cx, "remote-relay-0-token-path");
    cx.simulate_input("relay-token");
    click(cx, "remote-relay-0-token-save");
    view.read_with(cx, |view, _| assert!(view.remote.web.token_invalid));
    assert!(edits.lock().unwrap().is_empty());
    click(cx, "remote-relay-0-token-path");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input(TOKEN);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(relays(&edits), [vec![relay(true, false)]]);
    view.read_with(cx, |view, _| assert!(view.remote.web.editing.is_none()));
}

#[gpui::test]
fn turning_on_asks_first_in_the_windows_words(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    with_relay(&view, cx, relay(true, false), None);
    click(cx, "remote-relay-0-web-toggle");
    assert!(cx.debug_bounds("remote-relay-0-web-ask").is_some());
    assert!(edits.lock().unwrap().is_empty(), "asking saves nothing");
    click(cx, "remote-relay-0-web-cancel");
    view.read_with(cx, |view, _| assert!(view.remote.web.asking.is_none()));
    click(cx, "remote-relay-0-web-toggle");
    click(cx, "remote-relay-0-web-confirm");
    assert_eq!(relays(&edits), [vec![relay(true, true)]]);
}

#[gpui::test]
fn turning_off_saves_at_once(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    with_relay(&view, cx, relay(true, true), Some(Transparency::Unchecked));
    click(cx, "remote-relay-0-web-toggle");
    assert!(cx.debug_bounds("remote-relay-0-web-ask").is_none());
    assert_eq!(relays(&edits), [vec![relay(true, false)]]);
}

static RESUMED: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn resuming(_: &Path, request: &Request) -> Result<Response, control::Error> {
    if matches!(request, Request::ResumeWeb { .. }) {
        RESUMED.lock().unwrap().push(request.clone());
    }
    Ok(Response::Done)
}

#[gpui::test]
fn the_page_shows_the_address_its_code_and_an_alert(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    with_relay(
        &view,
        cx,
        relay(true, true),
        Some(Transparency::Clean { checked_ms: 0 }),
    );
    for selector in [
        "remote-relay-0-web-url",
        "remote-relay-0-web-qr",
        "remote-relay-0-web-certificate",
        "remote-relay-0-web-transparency",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    assert!(cx.debug_bounds("remote-relay-0-web-alert").is_none());
    let alert = Transparency::Alert {
        certificate: "ab12".into(),
        found_ms: 0,
    };
    cx.update(|_, cx| {
        let config = crate::config::RemoteConfig {
            enabled: true,
            ..Default::default()
        };
        Supervisor::fixture(cx, config, resuming);
    });
    with_relay(&view, cx, relay(true, true), Some(alert));
    assert!(cx.debug_bounds("remote-relay-0-web-alert").is_some());
    // The address is off: no code or address invites a phone to it.
    assert!(cx.debug_bounds("remote-relay-0-web-qr").is_none());
    assert!(cx.debug_bounds("remote-relay-0-web-url").is_none());
    click(cx, "remote-relay-0-web-resume");
    assert!(cx.debug_bounds("remote-relay-0-web-ask").is_some());
    click(cx, "remote-relay-0-web-confirm");
    assert_eq!(
        *RESUMED.lock().unwrap(),
        [Request::ResumeWeb { relay: HOME.into() }]
    );
    assert!(relays(&edits).is_empty(), "on already in the config");
}
