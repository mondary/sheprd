use super::*;
use crate::control::protocol::remote::WebRelay;
use sheprd_remote_control::control::web::{Transparency, WebStatus};

fn home(status: Option<WebStatus>) -> WebRelay {
    WebRelay {
        enabled: true,
        endpoint: "https://relay.example.org".into(),
        name: Some("home".into()),
        status,
        token_file: true,
    }
}

fn reported(transparency: Transparency) -> WebStatus {
    WebStatus {
        url: Some("https://k3x9.relay.example.org".into()),
        // 2027-01-15 08:00 UTC.
        expires_ms: Some(1_800_000_000_000),
        error: None,
        transparency,
    }
}

#[test]
fn web_status_prints_the_address_certificate_and_watch() {
    let output = remote::output(&Reply::Web {
        relays: vec![
            home(Some(reported(Transparency::Unchecked))),
            WebRelay {
                enabled: false,
                endpoint: "https://eu.example.org".into(),
                name: None,
                status: None,
                token_file: true,
            },
        ],
        error: None,
    })
    .unwrap();
    assert_eq!(output.status, 0);
    assert_eq!(
        output.text,
        "Relay: home (https://relay.example.org) — web access on\n  \
         URL: https://k3x9.relay.example.org\n  \
         Certificate: expires 2027-01-15 08:00 UTC\n  \
         Certificate Transparency: not checked yet\n\
         Relay: https://eu.example.org — web access off"
    );
    assert_eq!(output.json["alert"], false);
    assert_eq!(output.json["relays"][0]["name"], "home");
    assert_eq!(
        output.json["relays"][0]["status"]["url"],
        "https://k3x9.relay.example.org"
    );
    assert_eq!(output.json["relays"][1]["enabled"], false);
}

#[test]
fn an_alert_leads_its_relay_and_says_how_to_turn_it_on_again() {
    let output = remote::output(&Reply::Web {
        relays: vec![home(Some(reported(Transparency::Alert {
            certificate: "ab12".into(),
            found_ms: 0,
        })))],
        error: None,
    })
    .unwrap();
    let mut lines = output.text.lines();
    assert_eq!(
        lines.next(),
        Some("Relay: home (https://relay.example.org) — web access off after an alert")
    );
    let alert = lines.next().unwrap();
    assert!(alert.starts_with("  ALERT: "), "{alert}");
    assert!(alert.contains("SHA-256 ab12"), "{alert}");
    assert!(
        alert.contains("sheprd remote web enable --relay home"),
        "{alert}"
    );
    assert_eq!(output.json["alert"], true);
    assert_eq!(
        output.json["relays"][0]["status"]["transparency"]["state"],
        "alert"
    );
}

#[test]
fn no_private_relay_exits_nothing_and_says_why_the_daemon_did_not_answer() {
    let output = remote::output(&Reply::Web {
        relays: Vec::new(),
        error: Some("could not reach the remote control daemon".into()),
    })
    .unwrap();
    assert_eq!(output.status, exit::NOTHING);
    assert!(output.text.starts_with(
        "The remote control daemon did not answer: could not reach the remote control daemon\n"
    ));
    assert!(output.text.contains("listen token file"));
    assert_eq!(
        output.json["error"],
        "could not reach the remote control daemon"
    );
}

#[test]
fn remote_status_adds_a_web_line_to_a_relay_serving_it() {
    let mut status = status();
    let mut relay: sheprd_remote_control::control::RelayStatus =
        sheprd_remote_control::control::RelayState::Listening { rtt_ms: None }.into();
    relay.web = Some(reported(Transparency::Unchecked));
    status
        .relays
        .insert("https://relay.example.org".into(), relay);
    let output = remote::output(&Reply::Status {
        enabled: true,
        status: Some(Box::new(status)),
        error: None,
    })
    .unwrap();
    assert!(
        output.text.contains(
            "Relay: https://relay.example.org — listening\n  Web: https://k3x9.relay.example.org · \
         certificate expires 2027-01-15 08:00 UTC · Certificate Transparency not checked yet"
        ),
        "{}",
        output.text
    );
    assert_eq!(
        output.json["status"]["relays"]["https://relay.example.org"]["web"]["url"],
        "https://k3x9.relay.example.org"
    );
}
