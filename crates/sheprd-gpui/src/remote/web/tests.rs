#![allow(clippy::unwrap_used)]
use super::*;
use crate::{config::RemoteConfig, sidebar::layout_tests::fixture_window};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};
use sheprd_remote_control::control::{RelayState, Response};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

const HOME: &str = "https://relay.example.org";
const AWAY: &str = "https://eu.example.org";

fn private(endpoint: &str, web: bool) -> Relay {
    Relay {
        token_file: Some(PathBuf::from("/etc/relay/token")),
        web,
        ..Relay::from(endpoint)
    }
}

/// The daemon running, reporting web access through [`HOME`] as `transparency` finds it.
fn health(transparency: Transparency) -> Health {
    let mut relay: RelayStatus = RelayState::Listening { rtt_ms: None }.into();
    relay.web = Some(WebStatus {
        url: Some("https://k3x9.relay.example.org".into()),
        expires_ms: Some(1_800_000_000_000),
        error: None,
        transparency,
    });
    Health::Running(Status {
        boot_id: String::new(),
        address_details: Default::default(),
        version: "dev".into(),
        port: 7443,
        addresses: Vec::new(),
        fingerprint: String::new(),
        computer: String::new(),
        herdr: true,
        chats: 0,
        devices: 0,
        connected: 0,
        relays: [(HOME.to_owned(), relay)].into(),
    })
}

fn alert() -> Transparency {
    Transparency::Alert {
        certificate: "ab12".into(),
        found_ms: 1_700_000_000_000,
    }
}

#[test]
fn a_relay_is_the_same_however_it_is_spelled() {
    assert!(same_relay(HOME, HOME));
    assert!(same_relay("https://relay.example.org/", HOME));
    assert!(!same_relay(HOME, AWAY));
    let health = health(alert());
    assert!(web_status(&health, "https://relay.example.org/").is_some());
    assert!(web_status(&health, AWAY).is_none());
    assert!(web_status(&Health::Off, HOME).is_none());
}

#[test]
fn turning_on_saves_the_switch_and_resumes_after_an_alert() {
    let relays = [private(HOME, false), private(AWAY, true)];
    let clean = health(Transparency::Unchecked);
    let turn = turn_on(&relays, HOME, &clean);
    assert_eq!(turn.resume, None);
    assert_eq!(
        turn.relays,
        Some(vec![private(HOME, true), private(AWAY, true)]),
        "the other relay keeps its own switch"
    );
    // On already, but an alert turned it off at the daemon.
    let on = [private(HOME, true)];
    let turn = turn_on(&on, HOME, &health(alert()));
    assert_eq!(turn.resume.as_deref(), Some(HOME));
    assert_eq!(turn.relays, None);
    assert!(turn_on(&on, HOME, &clean).is_empty());
    // A public relay never gets web access.
    let public = [Relay::from(HOME)];
    assert!(turn_on(&public, HOME, &clean).is_empty());
}

#[test]
fn turning_off_saves_only_a_change() {
    let relays = [private(HOME, true), private(AWAY, true)];
    assert_eq!(
        turn_off(&relays, "https://relay.example.org/"),
        Some(vec![private(HOME, false), private(AWAY, true)])
    );
    assert_eq!(turn_off(&[private(HOME, false)], HOME), None);
}

#[test]
fn the_question_warns_after_an_alert() {
    let (title, lines) = question("home", false);
    assert!(title.contains("home"), "{title}");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("passkey"), "{}", lines[0]);
    let (_, lines) = question("home", true);
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("operator"), "{}", lines[1]);
}

static RESUMED: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    if matches!(request, Request::ResumeWeb { .. }) {
        RESUMED.lock().unwrap().push(request.clone());
    }
    Ok(Response::Done)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn ask(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    let ask = WebAsk {
        endpoint: HOME.into(),
        name: "home".into(),
    };
    cx.update(|window, cx| view.update(cx, |view, cx| view.ask_web_access(ask, window, cx)))
}

#[gpui::test]
fn the_window_asks_first_and_resumes_only_on_confirm(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let config = RemoteConfig {
        enabled: true,
        // On in the config, so confirming only resumes and writes nothing.
        relays: vec![private(HOME, true)],
        ..RemoteConfig::default()
    };
    cx.update(|_, cx| {
        Supervisor::fixture(cx, config.clone(), daemon);
        Supervisor::set_health(cx, health(alert()), None);
    });
    view.update(cx, |view, _| view.config.remote = config);
    assert!(ask(&view, cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteWeb));
    });
    draw(cx);
    assert!(cx.debug_bounds("remote-web").is_some());
    // Another question waits until this one closes.
    assert!(!ask(&view, cx));
    let cancel = cx.debug_bounds("remote-web-cancel").unwrap().center();
    cx.simulate_click(cancel, Modifiers::default());
    cx.run_until_parked();
    assert!(RESUMED.lock().unwrap().is_empty(), "cancel sends nothing");
    view.read_with(cx, |view, _| assert!(view.menu.remote_web.is_none()));
    assert!(ask(&view, cx));
    draw(cx);
    let submit = cx.debug_bounds("remote-web-submit").unwrap().center();
    cx.simulate_click(submit, Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    assert_eq!(
        *RESUMED.lock().unwrap(),
        [Request::ResumeWeb { relay: HOME.into() }]
    );
}
