//! Web access through a private relay (`docs/relay-web-plan.md`): browsers
//! reach this computer at its web name, `<label>.<relay host>`, and the
//! relay carries their TLS without ending it. This computer ends it with a
//! certificate for that name whose key it made and keeps: it sends the
//! relay only a CSR, checks what comes back, and renews it before it runs
//! out.

mod check;
mod error;
mod store;
pub(crate) mod transparency;

pub(crate) use check::{Issued, check, public_roots};
pub use error::Error;
#[cfg(test)]
pub(crate) use store::dir as store_dir;

use super::Relays;
use crate::control::web::{Transparency, WebStatus};
use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256, PublicKeyData as _};
use rustls::{
    ServerConfig,
    crypto::ring,
    pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
};
use sheprd_relay::{
    Endpoint,
    client::{self, Connection},
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;

/// The first wait after a failed attempt: the relay allows one every ten
/// minutes per computer.
const FIRST_RETRY: Duration = Duration::from_secs(10 * 60);
/// The longest wait between failed attempts.
const LONGEST_RETRY: Duration = Duration::from_secs(6 * 60 * 60);
/// The shortest wait before looking again, however near the renewal.
const LEAST_WAIT: Duration = Duration::from_secs(60);

/// What browsers are served with on one relay.
#[derive(Clone, Debug)]
pub(crate) struct Serving {
    pub(crate) config: Arc<ServerConfig>,
    /// The web name, `<label>.<relay host>`.
    pub(crate) host: String,
    /// `https://<web name>`, the origin the app runs at there.
    pub(crate) origin: String,
}

/// Keeps `endpoint`'s web certificate current over `connection`, served
/// as soon as one is good, and watches Certificate Transparency for its
/// name, every hour and after each issuance, until the connection closes.
/// A certificate with another key turns web access off on this relay.
pub(super) async fn renew(relays: Arc<Relays>, endpoint: Endpoint, connection: Connection) {
    let mut retry = FIRST_RETRY;
    let mut renew_at = Instant::now();
    let mut check_at = Instant::now();
    let agent = transparency::agent();
    loop {
        if Instant::now() >= renew_at {
            renew_at = match current(&relays, &endpoint, &connection).await {
                Ok(Current { renew_at, issued }) => {
                    retry = FIRST_RETRY;
                    if issued {
                        check_at = Instant::now();
                    }
                    let wait = renew_at
                        .duration_since(SystemTime::now())
                        .unwrap_or_default()
                        .max(LEAST_WAIT);
                    Instant::now() + wait
                }
                Err(error) => {
                    tracing::warn!(relay = %endpoint, error = %error, "no web certificate yet");
                    relays.set_web_status(&endpoint, |status| {
                        status.error = Some(error.to_string());
                    });
                    let wait = retry;
                    retry = (retry * 2).min(LONGEST_RETRY);
                    Instant::now() + wait
                }
            };
        }
        if Instant::now() >= check_at {
            if watch(&relays, &endpoint, &agent).await {
                return;
            }
            check_at = Instant::now() + transparency::EVERY;
        }
        tokio::select! {
            () = tokio::time::sleep_until(renew_at.min(check_at)) => {}
            _ = connection.closed() => return,
        }
    }
}

/// One Certificate Transparency lookup of `endpoint`'s web name, recorded
/// in its status; true when it found another key and turned web access
/// off.
async fn watch(relays: &Arc<Relays>, endpoint: &Endpoint, agent: &ureq::Agent) -> bool {
    let key = endpoint.to_string();
    let Some(host) = endpoint.web_host(&client::route(&relays.keys.route)) else {
        return false;
    };
    let own = relays.web_keys(&key);
    // Before this computer has a key there, any certificate would look
    // foreign; the first lookup follows the first key.
    if own.is_empty() {
        return false;
    }
    let api = relays.transparency.clone();
    let agent = agent.clone();
    let dir = store::dir(&relays.state_dir, &key);
    let found = tokio::task::spawn_blocking(move || {
        let accepted = store::load_accepted(&dir);
        transparency::check(&agent, &api, &host, &own, &accepted)
    })
    .await
    .map_err(|_| Error::Cancelled)
    .and_then(|found| found.map_err(Error::from));
    let checked_ms = now_ms();
    match found {
        Ok(transparency::Finding::Clean) => {
            relays.set_web_status(endpoint, |status| {
                status.transparency = Transparency::Clean { checked_ms };
            });
            false
        }
        Ok(transparency::Finding::Foreign(certificate)) => {
            relays.web_alert(&key, certificate, checked_ms).await;
            true
        }
        Err(error) => {
            tracing::warn!(relay = %endpoint, error = %error, "could not check Certificate Transparency");
            relays.set_web_status(endpoint, |status| {
                status.transparency = Transparency::Failed {
                    error: error.to_string(),
                    checked_ms,
                };
            });
            false
        }
    }
}

/// What [`current`] leaves: when to look again, and whether it got a new
/// certificate, which the CT watch then looks for.
struct Current {
    renew_at: SystemTime,
    issued: bool,
}

/// Serves the kept certificate while it is good, and asks for a new one
/// once it is due: when to look again.
async fn current(
    relays: &Relays,
    endpoint: &Endpoint,
    connection: &Connection,
) -> Result<Current, Error> {
    let host = endpoint
        .web_host(&client::route(&relays.keys.route))
        .ok_or(Error::NoName)?;
    let dir = store::dir(&relays.state_dir, &endpoint.to_string());
    let kept = blocking({
        let dir = dir.clone();
        move || store::load(&dir)
    })
    .await?;
    let roots = &relays.web_roots;
    let mut key = None;
    relays.set_web_status(endpoint, |status| {
        status.url = Some(format!("https://{host}"));
    });
    if let Some(der) = kept.key
        && let Ok(pair) = KeyPair::try_from(der.as_slice())
    {
        relays.own_web_key(endpoint, &pair);
        if let Some(pem) = kept.chain
            && let Ok(issued) = check(&pem, &pair, &host, roots, SystemTime::now())
        {
            relays.serve_web(endpoint, serving(&issued, &pair, &host)?);
            relays.served_until(endpoint, issued.not_after);
            if SystemTime::now() < issued.renew_at() {
                return Ok(Current {
                    renew_at: issued.renew_at(),
                    issued: false,
                });
            }
        }
        // Renewing keeps the key, so a pinned key stays pinned, and so
        // does finishing an order a restart cut short.
        key = Some(pair);
    }
    let key = match key {
        Some(key) => key,
        None => {
            // Kept before its CSR goes out: a restart while the CA issues
            // finds it again, rather than making another and meeting this
            // one's certificate in the CT logs as a stranger's.
            let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
            let der = key.serialize_der();
            let dir = dir.clone();
            blocking(move || store::save_key(&dir, &der)).await?;
            key
        }
    };
    // Known as this computer's before the CA logs it, so the CT watch
    // never takes the certificate being ordered for another's.
    relays.own_web_key(endpoint, &key);
    let csr = CertificateParams::new(vec![host.clone()])?.serialize_request(&key)?;
    let pem = client::certificate(connection, &relays.keys.route, csr.der())
        .await
        .map_err(Error::Issue)?;
    let issued = check(&pem, &key, &host, roots, SystemTime::now())?;
    blocking(move || store::save_chain(&dir, &pem)).await?;
    relays.serve_web(endpoint, serving(&issued, &key, &host)?);
    relays.served_until(endpoint, issued.not_after);
    tracing::info!(relay = %endpoint, "serving browsers with a new web certificate");
    Ok(Current {
        renew_at: issued.renew_at(),
        issued: true,
    })
}

/// The TLS browsers are served with: `issued` and its `key`, HTTP/1.1.
fn serving(issued: &Issued, key: &KeyPair, host: &str) -> Result<Serving, Error> {
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    let mut config = ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(issued.chain.clone(), key)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Serving {
        config: Arc::new(config),
        host: host.to_owned(),
        origin: format!("https://{host}"),
    })
}

/// The web status of each relay `access` turns web access on for, with
/// any alert kept from before. Blocking.
pub(super) fn initial_status(
    state_dir: &Path,
    access: &BTreeMap<String, super::access::Access>,
) -> BTreeMap<String, WebStatus> {
    access
        .iter()
        .filter(|(_, access)| access.web)
        .map(|(endpoint, _)| {
            let transparency = store::load_alert(&store::dir(state_dir, endpoint))
                .map(|alert| Transparency::Alert {
                    certificate: alert.certificate,
                    found_ms: alert.found_ms,
                })
                .unwrap_or_default();
            let status = WebStatus {
                transparency,
                ..WebStatus::default()
            };
            (endpoint.clone(), status)
        })
        .collect()
}

/// The CT search used unless the daemon is told another.
pub(super) fn default_transparency() -> Result<url::Url, Error> {
    Ok(transparency::api(transparency::DEFAULT_API)?)
}

impl Relays {
    /// Looks web names up at `api` instead of Cert Spotter.
    pub(crate) fn set_transparency(&mut self, api: url::Url) {
        self.transparency = api;
    }

    /// Web access on `endpoint`, when it is turned on there.
    pub(super) fn web_status(&self, endpoint: &str) -> Option<WebStatus> {
        self.web_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(endpoint)
            .cloned()
    }

    #[cfg(test)]
    pub(crate) fn status_for_tests(&self, endpoint: &str) -> Option<WebStatus> {
        self.web_status(endpoint)
    }

    /// Whether a CT alert turned web access off on `endpoint`.
    pub(super) fn web_alerted(&self, endpoint: &str) -> bool {
        self.web_status(endpoint)
            .is_some_and(|status| matches!(status.transparency, Transparency::Alert { .. }))
    }

    fn set_web_status(&self, endpoint: &Endpoint, change: impl FnOnce(&mut WebStatus)) {
        if let Some(status) = self
            .web_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&endpoint.to_string())
        {
            change(status);
        }
    }

    /// Records the certificate served on `endpoint` until `not_after`.
    fn served_until(&self, endpoint: &Endpoint, not_after: SystemTime) {
        self.set_web_status(endpoint, |status| {
            status.expires_ms = Some(millis(not_after));
            status.error = None;
        });
    }

    /// Takes `key` as one of this computer's web keys on `endpoint`.
    fn own_web_key(&self, endpoint: &Endpoint, key: &KeyPair) {
        use sha2::Digest as _;
        let hash: [u8; 32] = sha2::Sha256::digest(key.subject_public_key_info()).into();
        let mut keys = self.web_keys.lock().unwrap_or_else(PoisonError::into_inner);
        let own = keys.entry(endpoint.to_string()).or_default();
        if !own.contains(&hash) {
            own.push(hash);
        }
    }

    fn web_keys(&self, endpoint: &str) -> Vec<[u8; 32]> {
        self.web_keys
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(endpoint)
            .cloned()
            .unwrap_or_default()
    }

    /// A certificate for `endpoint`'s web name has another key: web access
    /// goes off there, kept off across restarts until the user turns it on
    /// again, and paired devices are told.
    async fn web_alert(self: &Arc<Self>, endpoint: &str, certificate: String, found_ms: u64) {
        tracing::error!(
            relay = %endpoint,
            %certificate,
            "Certificate Transparency lists a certificate for this computer's web name with another key; web access is off on this relay"
        );
        let alert = store::Alert {
            certificate: certificate.clone(),
            found_ms,
        };
        let dir = store::dir(&self.state_dir, endpoint);
        if let Err(error) = blocking(move || store::save_alert(&dir, &alert)).await {
            // Off for now all the same; only a restart forgets it.
            tracing::error!(relay = %endpoint, error = %error, "could not keep the web access alert");
        }
        if let Some(status) = self
            .web_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(endpoint)
        {
            status.transparency = Transparency::Alert {
                certificate,
                found_ms,
            };
        }
        self.web
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(endpoint);
        if let Some(state) = self.served_state() {
            state.notify_all(crate::push::Notification::security(
                "Web access turned off".to_owned(),
                format!(
                    "A certificate for this computer's web address on {endpoint} was issued to someone else. Check Sheprd's Remote control settings."
                ),
            ));
        }
        self.restart(endpoint);
    }

    /// Turns web access on `endpoint` on again after an alert: the
    /// certificate it named is accepted, so the watch does not turn web
    /// access off again over it while it stays listed, the alert goes, and
    /// the listener starts over. Another certificate with a foreign key
    /// still alerts. Blocking.
    pub(crate) fn resume_web(self: &Arc<Self>, endpoint: &str) -> Result<bool, Error> {
        let Some(mut status) = self.web_status(endpoint) else {
            return Ok(false);
        };
        let dir = store::dir(&self.state_dir, endpoint);
        let alerted = match &status.transparency {
            Transparency::Alert { certificate, .. } => Some(certificate.clone()),
            _ => store::load_alert(&dir).map(|alert| alert.certificate),
        };
        if let Some(certificate) = alerted.filter(|certificate| !certificate.is_empty()) {
            store::accept(&dir, &certificate)?;
        }
        store::clear_alert(&dir)?;
        status.transparency = Transparency::Unchecked;
        self.web_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(endpoint.to_owned(), status);
        self.restart(endpoint);
        Ok(true)
    }
}

fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    })
}

fn now_ms() -> u64 {
    millis(SystemTime::now())
}

/// Runs file work off the runtime's threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or(Err(Error::Cancelled))
}
