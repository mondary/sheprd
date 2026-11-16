//! Where a relay's web key and certificate are kept: one directory per
//! relay under `STATE_DIR/relay/web/`, named by a hash of its endpoint,
//! private to this user. The key never leaves it but in TLS handshakes.

use super::Error;
use crate::auth::devices::write_private;
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const KEY: &str = "key.der";
const CHAIN: &str = "chain.pem";
/// A Certificate Transparency alert that turned web access off, kept until
/// the user turns it on again.
const ALERT: &str = "alert.json";
/// The certificates the user turned web access on again after, which the
/// watch no longer alerts on.
const ACCEPTED: &str = "accepted.json";
/// The most accepted certificates kept; the oldest go first.
const MAX_ACCEPTED: usize = 32;

/// What a kept alert says.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Alert {
    pub(crate) certificate: String,
    pub(crate) found_ms: u64,
}

/// The directory for `endpoint`'s web certificate.
#[must_use]
pub(crate) fn dir(state_dir: &Path, endpoint: &str) -> PathBuf {
    let digest = Sha256::digest(endpoint.as_bytes());
    let name: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    state_dir.join("relay").join("web").join(name)
}

/// The kept key (PKCS#8 DER) and chain (PEM), when both are there.
/// Blocking.
pub(crate) fn load(dir: &Path) -> Result<Option<(Vec<u8>, String)>, Error> {
    let read = |name: &str| match fs::read(dir.join(name)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Read {
            path: dir.join(name),
            source,
        }),
    };
    let (Some(key), Some(chain)) = (read(KEY)?, read(CHAIN)?) else {
        return Ok(None);
    };
    Ok(String::from_utf8(chain).ok().map(|chain| (key, chain)))
}

/// Keeps `key` and `chain`, each written whole or not at all. Blocking.
pub(crate) fn save(dir: &Path, key: &[u8], chain: &str) -> Result<(), Error> {
    for (name, bytes) in [(KEY, key), (CHAIN, chain.as_bytes())] {
        let path = dir.join(name);
        write_private(&path, bytes).map_err(|source| Error::Save { path, source })?;
    }
    Ok(())
}

/// The alert kept in `dir`, if any. One that does not read is still an
/// alert: web access stays off rather than trusting a damaged file.
/// Blocking.
pub(crate) fn load_alert(dir: &Path) -> Option<Alert> {
    let bytes = match fs::read(dir.join(ALERT)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(_) => Vec::new(),
    };
    Some(serde_json::from_slice(&bytes).unwrap_or(Alert {
        certificate: String::new(),
        found_ms: 0,
    }))
}

/// Keeps `alert` in `dir`. Blocking.
pub(crate) fn save_alert(dir: &Path, alert: &Alert) -> Result<(), Error> {
    let path = dir.join(ALERT);
    let bytes = serde_json::to_vec(alert).map_err(|source| Error::Save {
        path: path.clone(),
        source: crate::auth::Error::Encode(source),
    })?;
    write_private(&path, &bytes).map_err(|source| Error::Save { path, source })
}

/// Forgets the alert in `dir`. Blocking.
pub(crate) fn clear_alert(dir: &Path) -> Result<(), Error> {
    let path = dir.join(ALERT);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Remove { path, source }),
    }
}

/// The certificates accepted in `dir`. One file that does not read
/// accepts none, so the watch alerts again rather than staying quiet.
/// Blocking.
pub(crate) fn load_accepted(dir: &Path) -> Vec<String> {
    fs::read(dir.join(ACCEPTED))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Adds `certificate` to those accepted in `dir`. Blocking.
pub(crate) fn accept(dir: &Path, certificate: &str) -> Result<(), Error> {
    let mut accepted = load_accepted(dir);
    if accepted.iter().any(|kept| kept == certificate) {
        return Ok(());
    }
    accepted.push(certificate.to_owned());
    let excess = accepted.len().saturating_sub(MAX_ACCEPTED);
    accepted.drain(..excess);
    let path = dir.join(ACCEPTED);
    let bytes = serde_json::to_vec(&accepted).map_err(|source| Error::Save {
        path: path.clone(),
        source: crate::auth::Error::Encode(source),
    })?;
    write_private(&path, &bytes).map_err(|source| Error::Save { path, source })
}
