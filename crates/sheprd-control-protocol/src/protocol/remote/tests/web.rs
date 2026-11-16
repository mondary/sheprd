use super::*;
use sheprd_remote_control::control::web::{Transparency, WebStatus};

#[test]
fn web_ops_have_a_stable_wire_form() {
    for (text, op) in [
        (r#"{"op":"web_status"}"#, RemoteOp::WebStatus),
        (
            r#"{"op":"web_enable"}"#,
            RemoteOp::WebEnable { relay: None },
        ),
        (
            r#"{"op":"web_enable","relay":"fra3"}"#,
            RemoteOp::WebEnable {
                relay: Some("fra3".into()),
            },
        ),
        (
            r#"{"op":"web_disable","relay":"https://relay.example.org"}"#,
            RemoteOp::WebDisable {
                relay: Some("https://relay.example.org".into()),
            },
        ),
    ] {
        assert_eq!(serde_json::to_string(&op).unwrap(), text);
        assert_eq!(serde_json::from_str::<RemoteOp>(text).unwrap(), op);
    }
    for invalid in [
        r#"{"op":"web_enable","relay":"a","token":"t"}"#,
        r#"{"op":"web_disable","relay":7}"#,
    ] {
        assert!(
            serde_json::from_str::<RemoteOp>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn enabling_answers_at_once_and_status_waits_for_the_daemon() {
    // The user confirms in the window; the answer only says it asked.
    assert_eq!(RemoteOp::WebEnable { relay: None }.wait_seconds(), 0);
    assert!(
        RemoteOp::WebStatus.wait_seconds()
            > sheprd_remote_control::control::RESPONSE_TIMEOUT.as_secs()
    );
    assert!(RemoteOp::WebDisable { relay: None }.wait_seconds() > 0);
}

#[test]
fn web_replies_round_trip_and_leave_out_what_is_unknown() {
    let reply = Reply::Web {
        relays: vec![
            WebRelay {
                enabled: true,
                endpoint: "https://relay.example.org".into(),
                name: Some("home".into()),
                status: Some(WebStatus {
                    url: Some("https://k3x9.relay.example.org".into()),
                    expires_ms: Some(1_800_000_000_000),
                    error: None,
                    transparency: Transparency::Alert {
                        certificate: "ab12".into(),
                        found_ms: 1_700_000_000_000,
                    },
                }),
                token_file: true,
            },
            WebRelay {
                enabled: false,
                endpoint: "https://eu.example.org".into(),
                name: None,
                status: None,
                token_file: true,
            },
        ],
        error: None,
    };
    let response = Response::Remote { reply };
    let text = serde_json::to_string(&response).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let relays = &json["reply"]["relays"];
    assert_eq!(json["reply"]["kind"], "web");
    assert_eq!(relays[0]["status"]["transparency"]["state"], "alert");
    assert!(relays[1].get("name").is_none());
    assert!(relays[1].get("status").is_none());
    assert!(json["reply"].get("error").is_none());
}
