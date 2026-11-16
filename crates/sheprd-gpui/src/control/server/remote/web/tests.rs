#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    config::RemoteConfig, menu::Page, remote::Health, sidebar::layout_tests::fixture_window,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use sheprd_remote_control::{
    control::{
        self, RelayState, RelayStatus,
        web::{Transparency, WebStatus},
    },
    relay_label::RelayLabel,
};
use std::{path::PathBuf, sync::Mutex};

const HOME: &str = "https://relay.example.org";
const AWAY: &str = "https://eu.example.org";
const OPEN: &str = "https://open.example.org";

fn relay(endpoint: &str, name: Option<&str>, token: bool, web: bool) -> Relay {
    Relay {
        endpoint: endpoint.into(),
        label: RelayLabel {
            name: name.map(str::to_owned),
            ..RelayLabel::default()
        },
        token_file: token.then(|| PathBuf::from("/etc/relay/token")),
        web,
    }
}

fn code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

fn message(response: &Response) -> &str {
    match response {
        Response::Done { message } | Response::Error { message, .. } => message,
        response => panic!("{response:?}"),
    }
}

fn status(transparency: Transparency) -> Status {
    let mut home: RelayStatus = RelayState::Listening { rtt_ms: None }.into();
    home.web = Some(WebStatus {
        url: Some("https://k3x9.relay.example.org".into()),
        expires_ms: Some(1_800_000_000_000),
        error: None,
        transparency,
    });
    Status {
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
        relays: [
            (HOME.to_owned(), home),
            (OPEN.to_owned(), RelayState::Connecting.into()),
        ]
        .into(),
    }
}

fn alert() -> Transparency {
    Transparency::Alert {
        certificate: "ab12".into(),
        found_ms: 1,
    }
}

#[test]
fn a_relay_is_named_by_its_address_or_its_name() {
    let relays = [
        relay(HOME, Some("home"), true, false),
        relay(AWAY, Some("eu"), true, false),
        relay(OPEN, Some("eu"), false, false),
    ];
    assert_eq!(
        pick(&relays, Some("https://relay.example.org/"))
            .unwrap()
            .endpoint,
        HOME
    );
    assert_eq!(pick(&relays, Some("home")).unwrap().endpoint, HOME);
    assert_eq!(
        code(&pick(&relays, Some("eu")).unwrap_err()),
        Some(ErrorCode::InvalidRequest),
        "a name two relays share is refused"
    );
    assert_eq!(
        code(&pick(&relays, Some("lab")).unwrap_err()),
        Some(ErrorCode::NotFound)
    );
}

#[test]
fn without_a_name_the_only_private_relay_is_meant() {
    let one = [
        relay(HOME, None, true, false),
        relay(OPEN, None, false, false),
    ];
    assert_eq!(pick(&one, None).unwrap().endpoint, HOME);
    let two = [
        relay(HOME, None, true, false),
        relay(AWAY, None, true, false),
    ];
    let several = pick(&two, None).unwrap_err();
    assert_eq!(code(&several), Some(ErrorCode::InvalidRequest));
    assert!(message(&several).contains("--relay"));
    let public = [relay(OPEN, None, false, false)];
    let refused = pick(&public, None).unwrap_err();
    assert_eq!(code(&refused), Some(ErrorCode::InvalidRequest));
    assert!(message(&refused).contains("token file"));
    assert_eq!(
        code(&pick(&[], None).unwrap_err()),
        Some(ErrorCode::NotFound)
    );
}

#[test]
fn status_lists_private_relays_with_what_the_daemon_reports() {
    let relays = [
        relay("https://relay.example.org/", Some("home"), true, true),
        relay(AWAY, None, true, false),
        relay(OPEN, None, false, false),
    ];
    let listed = listed(&relays, Some(&status(alert())));
    assert_eq!(listed.len(), 2, "a public relay offers no web access");
    assert_eq!(listed[0].endpoint, HOME, "as the daemon names it");
    assert_eq!(listed[0].name.as_deref(), Some("home"));
    assert!(listed[0].enabled && listed[0].token_file);
    assert!(matches!(
        listed[0].status.as_ref().unwrap().transparency,
        Transparency::Alert { .. }
    ));
    assert_eq!(listed[1].endpoint, AWAY);
    assert!(!listed[1].enabled);
    assert!(listed[1].status.is_none());
    // Without the daemon, the config alone.
    assert!(
        super::listed(&relays, None)
            .iter()
            .all(|relay| relay.status.is_none())
    );
}

static RESUMED: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn daemon(_: &std::path::Path, request: &Request) -> Result<control::Response, control::Error> {
    RESUMED.lock().unwrap().push(request.clone());
    Ok(control::Response::Done)
}

fn window(
    cx: &mut TestAppContext,
    enabled: bool,
    relays: Vec<Relay>,
    health: Health,
) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let config = RemoteConfig {
        enabled,
        relays,
        ..RemoteConfig::default()
    };
    cx.update(|_, cx| {
        Supervisor::fixture(cx, config.clone(), daemon);
        Supervisor::set_health(cx, health, None);
    });
    view.update(cx, |view, _| view.config.remote = config);
    (view, cx)
}

fn run(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, needle: Option<&str>) -> Response {
    cx.update(|window, cx| view.update(cx, |view, cx| ask(view, needle, window, cx)))
}

#[gpui::test]
fn enabling_opens_the_question_and_changes_nothing(cx: &mut TestAppContext) {
    let relays = vec![relay(HOME, Some("home"), true, false)];
    let (view, cx) = window(
        cx,
        true,
        relays.clone(),
        Health::Running(status(Transparency::Unchecked)),
    );
    let response = run(&view, cx, None);
    assert!(
        message(&response).starts_with("Asking in Sheprd to turn on web access through home"),
        "{response:?}"
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteWeb));
        assert_eq!(view.menu.remote_web.as_ref().unwrap().endpoint, HOME);
        assert_eq!(
            view.config.remote.relays, relays,
            "nothing is saved until the user confirms"
        );
    });
    cx.run_until_parked();
    assert!(RESUMED.lock().unwrap().is_empty());
    // A second ask waits for the open question.
    assert_eq!(code(&run(&view, cx, None)), Some(ErrorCode::WindowBusy));
}

#[gpui::test]
fn enabling_refuses_a_public_relay_and_remote_control_off(cx: &mut TestAppContext) {
    let relays = vec![relay(OPEN, Some("open"), false, false)];
    let (view, cx) = window(cx, true, relays.clone(), Health::Off);
    let refused = run(&view, cx, Some("open"));
    assert_eq!(code(&refused), Some(ErrorCode::InvalidRequest));
    assert!(message(&refused).contains("private relay"), "{refused:?}");
    view.update(cx, |view, _| view.config.remote.enabled = false);
    assert_eq!(code(&run(&view, cx, Some("open"))), Some(ErrorCode::Failed));
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
}

#[gpui::test]
fn enabling_what_is_on_says_so_unless_an_alert_turned_it_off(cx: &mut TestAppContext) {
    let relays = vec![relay(HOME, None, true, true)];
    let (view, cx) = window(
        cx,
        true,
        relays,
        Health::Running(status(Transparency::Unchecked)),
    );
    let on = run(&view, cx, None);
    assert!(message(&on).contains("already on"), "{on:?}");
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
    cx.update(|_, cx| Supervisor::set_health(cx, Health::Running(status(alert())), None));
    let asked = run(&view, cx, Some(HOME));
    assert!(message(&asked).starts_with("Asking"), "{asked:?}");
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::RemoteWeb))
    });
}
