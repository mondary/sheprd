//! The relay's TCP front on port 443 (`docs/relay-web-plan.md`): it reads
//! each connection's ClientHello, never ending TLS, and routes it by the
//! server name. The relay's own name goes to the HTTPS server behind
//! (Caddy, for the dashboard and the WebSocket fallback); a computer's web
//! name, `<label>.<relay host>`, goes to that computer's listener, which
//! ends TLS with a certificate whose key only it holds.

mod error;
pub mod hello;
pub mod proxy;
mod replay;
mod serve;

pub use error::Error;
pub use hello::ClientHello;
pub use serve::{Config, run};

use crate::WebLabel;

/// The ALPN protocol of an ACME TLS-ALPN-01 challenge (RFC 8737).
pub const ACME_TLS_ALPN: &[u8] = b"acme-tls/1";

/// Where the front sends a connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Destination {
    /// The relay's own name: the HTTPS server behind.
    Relay,
    /// A computer's web name: its listener, if it turned web access on.
    Web(WebLabel),
    /// An ACME challenge for a computer's web name, which the relay
    /// answers itself while it orders that computer's certificate.
    Challenge(WebLabel),
    /// No name, or one the relay does not serve: closed.
    Refuse,
}

impl Destination {
    /// Where `hello` goes on the relay named `relay_host` (lowercase, no
    /// trailing dot).
    #[must_use]
    pub fn of(hello: &ClientHello, relay_host: &str) -> Self {
        let Some(name) = hello.server_name.as_deref() else {
            return Self::Refuse;
        };
        if name == relay_host {
            return Self::Relay;
        }
        let label = name
            .strip_suffix(relay_host)
            .and_then(|rest| rest.strip_suffix('.'))
            .and_then(WebLabel::parse);
        match label {
            Some(label) if hello.offers(ACME_TLS_ALPN) => Self::Challenge(label),
            Some(label) => Self::Web(label),
            None => Self::Refuse,
        }
    }
}

/// The host name the front answers as, from the relay's public host
/// (`HOST[:PORT][/PREFIX]`): the host alone, lowercase. `None` for an IP
/// address, which no certificate for a web name can sit under.
#[must_use]
pub fn relay_host(public_host: &str) -> Option<String> {
    let host = public_host.split('/').next()?;
    let host = host.rsplit_once(':').map_or(host, |(host, _)| host);
    let named = !host.is_empty()
        && host.parse::<std::net::IpAddr>().is_err()
        && host.bytes().any(|byte| byte.is_ascii_alphabetic())
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'));
    named.then(|| host.to_ascii_lowercase())
}
