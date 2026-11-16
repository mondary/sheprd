//! Web access through a private relay (`docs/relay-web-plan.md`): browsers
//! reach this computer at its web name, `<label>.<relay host>`, and the
//! relay carries their TLS without ending it. This computer ends it with a
//! certificate for that name whose key it made and keeps: it sends the
//! relay only a CSR, checks what comes back, and renews it before it runs
//! out.

mod check;
mod error;
mod store;

pub(crate) use check::{Issued, check, public_roots};
pub use error::Error;
#[cfg(test)]
pub(crate) use store::dir as store_dir;

use super::Relays;
use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};
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
    sync::Arc,
    time::{Duration, SystemTime},
};

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
/// as soon as one is good, until the connection closes.
pub(super) async fn renew(relays: Arc<Relays>, endpoint: Endpoint, connection: Connection) {
    let mut retry = FIRST_RETRY;
    loop {
        let wait = match current(&relays, &endpoint, &connection).await {
            Ok(renew_at) => {
                retry = FIRST_RETRY;
                renew_at
                    .duration_since(SystemTime::now())
                    .unwrap_or_default()
                    .max(LEAST_WAIT)
            }
            Err(error) => {
                tracing::warn!(relay = %endpoint, error = %error, "no web certificate yet");
                let wait = retry;
                retry = (retry * 2).min(LONGEST_RETRY);
                wait
            }
        };
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            _ = connection.closed() => return,
        }
    }
}

/// Serves the kept certificate while it is good, and asks for a new one
/// once it is due: when to look again.
async fn current(
    relays: &Relays,
    endpoint: &Endpoint,
    connection: &Connection,
) -> Result<SystemTime, Error> {
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
    if let Some((der, pem)) = kept
        && let Ok(pair) = KeyPair::try_from(der.as_slice())
    {
        if let Ok(issued) = check(&pem, &pair, &host, roots, SystemTime::now()) {
            relays.serve_web(endpoint, serving(&issued, &pair, &host)?);
            if SystemTime::now() < issued.renew_at() {
                return Ok(issued.renew_at());
            }
        }
        // Renewing keeps the key, so a pinned key stays pinned.
        key = Some(pair);
    }
    let key = match key {
        Some(key) => key,
        None => KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?,
    };
    let csr = CertificateParams::new(vec![host.clone()])?.serialize_request(&key)?;
    let pem = client::certificate(connection, &relays.keys.route, csr.der())
        .await
        .map_err(Error::Issue)?;
    let issued = check(&pem, &key, &host, roots, SystemTime::now())?;
    let der = key.serialize_der();
    blocking(move || store::save(&dir, &der, &pem)).await?;
    relays.serve_web(endpoint, serving(&issued, &key, &host)?);
    tracing::info!(relay = %endpoint, "serving browsers with a new web certificate");
    Ok(issued.renew_at())
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

/// Runs file work off the runtime's threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or(Err(Error::Cancelled))
}
