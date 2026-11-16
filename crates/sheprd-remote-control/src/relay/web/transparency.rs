//! The Certificate Transparency watch (`docs/relay-web-plan.md`): every
//! certificate a public CA issues is logged, so this computer asks a CT
//! search (Cert Spotter's by default) for those naming its web name, and
//! any with a key that is not its own means someone else can serve pages
//! there. Wildcards covering the name count too. A query that fails is
//! reported, never taken as clean.

mod error;

pub use error::Error;

use serde::Deserialize;
use std::{io::Read as _, time::Duration};
use url::Url;

/// Cert Spotter's issuance search, which needs no account for a few
/// queries an hour.
pub(crate) const DEFAULT_API: &str = "https://api.certspotter.com/v1/issuances";
/// How often a web name is looked up, besides after each issuance.
pub(crate) const EVERY: Duration = Duration::from_secs(60 * 60);
/// How long one query may take.
const TIMEOUT: Duration = Duration::from_secs(30);
/// The most pages read for one name; more is reported, not guessed at.
const MAX_PAGES: usize = 20;
/// The largest answer page read.
const MAX_PAGE: u64 = 1024 * 1024;

/// What the search found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Finding {
    /// Every certificate has one of this computer's keys.
    Clean,
    /// This certificate (its SHA-256, hex) has another key.
    Foreign(String),
}

/// One logged certificate, as Cert Spotter lists it.
#[derive(Deserialize)]
struct Issuance {
    id: String,
    /// SHA-256 of its SubjectPublicKeyInfo, hex.
    pubkey_sha256: String,
    #[serde(default)]
    cert_sha256: Option<String>,
}

/// The agent queries go through: bounded, no redirects.
pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

/// Looks `host` up at `api`, comparing each certificate's key with `own`
/// (SHA-256 of each SubjectPublicKeyInfo this computer holds). The
/// certificates in `accepted`, ones the user turned web access on again
/// after, are passed over by name; another with the same foreign key is
/// still found. Blocking.
pub(crate) fn check(
    agent: &ureq::Agent,
    api: &Url,
    host: &str,
    own: &[[u8; 32]],
    accepted: &[String],
) -> Result<Finding, Error> {
    let mut after: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut url = api.clone();
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("domain", host)
                .append_pair("match_wildcards", "true");
            if let Some(after) = &after {
                query.append_pair("after", after);
            }
        }
        let page = fetch(agent, &url)?;
        let Some(last) = page.last() else {
            return Ok(Finding::Clean);
        };
        for issuance in &page {
            let key = hex32(&issuance.pubkey_sha256).ok_or(Error::KeyHash)?;
            if own.contains(&key) {
                continue;
            }
            let certificate = issuance
                .cert_sha256
                .clone()
                .unwrap_or_else(|| issuance.id.clone());
            if !accepted.contains(&certificate) {
                return Ok(Finding::Foreign(certificate));
            }
        }
        after = Some(last.id.clone());
    }
    Err(Error::TooMany)
}

fn fetch(agent: &ureq::Agent, url: &Url) -> Result<Vec<Issuance>, Error> {
    let response = agent.get(url.as_str()).call().map_err(Error::Unreachable)?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Error::Status(status));
    }
    let mut body = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_PAGE)
        .read_to_end(&mut body)
        .map_err(|error| Error::Unreachable(ureq::Error::Io(error)))?;
    serde_json::from_slice(&body).map_err(Error::Answer)
}

/// 64 hex digits as 32 bytes.
fn hex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut bytes = [0; 32];
    for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(bytes)
}

/// Parses the CT search's address: HTTPS, or HTTP to loopback alone, for
/// a search running on this computer.
pub(crate) fn api(text: &str) -> Result<Url, Error> {
    let url = Url::parse(text).map_err(Error::Url)?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name == "localhost",
        None => false,
    };
    match url.scheme() {
        "https" => Ok(url),
        "http" if loopback => Ok(url),
        _ => Err(Error::Insecure),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
