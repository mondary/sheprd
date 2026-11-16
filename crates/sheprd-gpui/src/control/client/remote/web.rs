//! `remote web status`: each private relay's web access, as text and JSON.
//! A Certificate Transparency alert leads its relay's lines, since web
//! access stays off there until the user turns it on again.
use super::Output;
use crate::control::protocol::remote::WebRelay;
use sheprd_remote_control::control::web::{Transparency, WebStatus};

/// What `remote web status` prints.
pub(super) fn output(relays: &[WebRelay], error: Option<&str>) -> crate::Result<Output> {
    let alert = relays.iter().any(|relay| {
        relay
            .status
            .as_ref()
            .is_some_and(|status| matches!(status.transparency, Transparency::Alert { .. }))
    });
    let json = serde_json::json!({ "relays": relays, "alert": alert, "error": error });
    let mut lines = Vec::new();
    if let Some(error) = error {
        lines.push(format!("The remote control daemon did not answer: {error}"));
    }
    if relays.is_empty() {
        lines.push(
            "No relay offers web access; give a private relay its listen token file in \
             Settings > Remote control > Relays"
                .to_owned(),
        );
        return Ok(Output::data(lines.join("\n"), &json)?.nothing());
    }
    let now = crate::remote::now_ms();
    lines.extend(relays.iter().map(|relay| relay_lines(relay, now)));
    Output::data(lines.join("\n"), &json)
}

fn relay_lines(relay: &WebRelay, now_ms: u64) -> String {
    let state = match (&relay.status, relay.enabled, relay.token_file) {
        (Some(status), _, _) if matches!(status.transparency, Transparency::Alert { .. }) => {
            "web access off after an alert"
        }
        (_, true, false) => "web access on, but no listen token file",
        (Some(_), true, true) => "web access on",
        (None, true, true) => "web access on, not reported yet",
        (_, false, _) => "web access off",
    };
    let mut text = match &relay.name {
        Some(name) => format!("Relay: {name} ({}) — {state}", relay.endpoint),
        None => format!("Relay: {} — {state}", relay.endpoint),
    };
    let Some(status) = &relay.status else {
        return text;
    };
    if let Transparency::Alert {
        certificate,
        found_ms,
    } = &status.transparency
    {
        let named = relay.name.as_deref().unwrap_or(&relay.endpoint);
        text.push_str(&format!(
            "\n  ALERT: a Certificate Transparency log showed a certificate for this web \
             address with another key ({}, SHA-256 {certificate}). Turning web access on \
             again trusts the relay's operator: sheprd remote web enable --relay {named}",
            ago(*found_ms, now_ms)
        ));
    }
    if let Some(url) = &status.url {
        text.push_str(&format!("\n  URL: {url}"));
    }
    text.push_str(&format!("\n  Certificate: {}", certificate(status)));
    text.push_str(&format!(
        "\n  Certificate Transparency: {}",
        transparency(&status.transparency, now_ms)
    ));
    text
}

/// One line for `remote status`: the address, the certificate, and the
/// Certificate Transparency watch.
pub(super) fn summary(status: &WebStatus, now_ms: u64) -> String {
    let mut parts = Vec::new();
    if let Some(url) = &status.url {
        parts.push(url.clone());
    }
    parts.push(format!("certificate {}", certificate(status)));
    parts.push(format!(
        "Certificate Transparency {}",
        transparency(&status.transparency, now_ms)
    ));
    parts.join(" · ")
}

fn certificate(status: &WebStatus) -> String {
    match (status.expires_ms, &status.error) {
        (Some(expires), _) => format!("expires {}", date(expires)),
        (None, Some(error)) => format!("failed: {error}"),
        (None, None) => "not issued yet".to_owned(),
    }
}

fn transparency(transparency: &Transparency, now_ms: u64) -> String {
    match transparency {
        Transparency::Unchecked => "not checked yet".to_owned(),
        Transparency::Clean { checked_ms } => {
            format!("clean, checked {}", ago(*checked_ms, now_ms))
        }
        Transparency::Failed { error, checked_ms } => {
            format!("check failed {}: {error}", ago(*checked_ms, now_ms))
        }
        Transparency::Alert { found_ms, .. } => {
            format!("alert, found {}", ago(*found_ms, now_ms))
        }
    }
}

fn ago(then_ms: u64, now_ms: u64) -> String {
    crate::ago::english(now_ms.saturating_sub(then_ms) / 1000)
}

/// A UTC date and minute, the same wherever the command runs.
fn date(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map_or_else(
            || ms.to_string(),
            |time| time.format("%Y-%m-%d %H:%M UTC").to_string(),
        )
}
