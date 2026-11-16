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
