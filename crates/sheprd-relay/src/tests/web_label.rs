//! Web labels: the first label of a computer's web address on a relay.

use super::signing_key;
use crate::{RouteId, WebLabel, route::WEB_LABEL_LEN};

fn route_of(bytes: [u8; 32]) -> RouteId {
    RouteId::from_bytes(bytes)
}

#[test]
fn a_label_is_base32_of_the_routes_first_twenty_bytes() {
    // "fooba" is RFC 4648's five-byte vector: "MZXW6YTB".
    let mut bytes = [0; 32];
    bytes[..20].copy_from_slice(b"foobafoobafoobafooba");
    let label = WebLabel::of(&route_of(bytes));
    assert_eq!(label.to_string(), "mzxw6ytb".repeat(4));
    assert_eq!(WebLabel::of(&route_of([0; 32])).to_string(), "a".repeat(32));
    assert_eq!(
        WebLabel::of(&route_of([0xff; 32])).to_string(),
        "7".repeat(32)
    );
}

#[test]
fn a_label_round_trips_and_names_one_dns_label() {
    let route = RouteId::of(signing_key(3).verifying_key());
    let label = WebLabel::of(&route);
    let text = label.to_string();
    assert_eq!(text.len(), WEB_LABEL_LEN);
    assert!(
        text.bytes()
            .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
    );
    assert_eq!(WebLabel::parse(&text), Some(label));
    assert_eq!(
        label.host("fra1.sheprd-relay.dev"),
        format!("{text}.fra1.sheprd-relay.dev")
    );
}

#[test]
fn only_exact_lowercase_labels_parse() {
    let text = WebLabel::of(&route_of([7; 32])).to_string();
    assert_eq!(WebLabel::parse(&text.to_ascii_uppercase()), None);
    assert_eq!(WebLabel::parse(&text[..31]), None);
    assert_eq!(WebLabel::parse(&format!("{text}a")), None);
    for bad in ['0', '1', '8', '9', '-', '.'] {
        let mut changed = text.clone();
        changed.replace_range(..1, &bad.to_string());
        assert_eq!(WebLabel::parse(&changed), None, "{bad}");
    }
    assert_eq!(WebLabel::parse(""), None);
}

#[test]
fn debug_shows_only_the_start() {
    let label = WebLabel::of(&route_of([9; 32]));
    let shown = format!("{label:?}");
    assert!(!shown.contains(&label.to_string()));
    assert!(shown.contains(&label.to_string()[..8]));
}
