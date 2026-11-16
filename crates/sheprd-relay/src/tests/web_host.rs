//! A computer's web name on a relay, from the relay's endpoint.

use crate::{Endpoint, RouteId, WebLabel};

#[test]
fn a_named_relay_gives_each_route_a_web_name() {
    let route = RouteId::from_bytes([5; 32]);
    let label = WebLabel::of(&route);
    let endpoint = Endpoint::parse("https://Fra1.Sheprd-Relay.dev:8443/sheprd").unwrap();
    assert_eq!(
        endpoint.web_host(&route),
        Some(format!("{label}.fra1.sheprd-relay.dev"))
    );
}

#[test]
fn a_relay_reached_by_address_has_no_web_names() {
    let route = RouteId::from_bytes([5; 32]);
    for address in ["https://203.0.113.7", "https://[2001:db8::1]"] {
        assert_eq!(
            Endpoint::parse(address).unwrap().web_host(&route),
            None,
            "{address}"
        );
    }
}
