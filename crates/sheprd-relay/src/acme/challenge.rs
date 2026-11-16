//! TLS-ALPN-01 (RFC 8737): while an order waits, the relay answers its
//! front for the computer's web name, on ALPN `acme-tls/1`, with a
//! self-signed certificate carrying the challenge's digest. Only the
//! validation server asks for that protocol; browsers still reach the
//! computer.

use super::error::Error;
use crate::WebLabel;
use rcgen::{CertificateParams, CustomExtension, KeyPair};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

/// A challenge certificate and its key.
#[derive(Debug)]
pub struct Answer {
    pub certificate: CertificateDer<'static>,
    pub key: PrivatePkcs8KeyDer<'static>,
}

/// The challenges the relay answers now, by web label.
#[derive(Debug, Default)]
pub struct Challenges {
    answers: Mutex<HashMap<WebLabel, Arc<Answer>>>,
}

impl Challenges {
    fn lock(&self) -> MutexGuard<'_, HashMap<WebLabel, Arc<Answer>>> {
        match self.answers.lock() {
            Ok(answers) => answers,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Answers for `label`'s `host` with `digest` (the key authorization's
    /// SHA-256) until the returned guard drops.
    pub fn answer(
        self: &Arc<Self>,
        label: WebLabel,
        host: &str,
        digest: &[u8],
    ) -> Result<Answering, Error> {
        let answer = Arc::new(certificate(host, digest)?);
        self.lock().insert(label, answer);
        Ok(Answering {
            challenges: Arc::clone(self),
            label,
        })
    }

    /// The answer for `label`, while one is pending.
    #[must_use]
    pub fn get(&self, label: &WebLabel) -> Option<Arc<Answer>> {
        self.lock().get(label).cloned()
    }
}

/// Removes its challenge when dropped, however the order ends.
#[derive(Debug)]
pub struct Answering {
    challenges: Arc<Challenges>,
    label: WebLabel,
}

impl Drop for Answering {
    fn drop(&mut self) {
        self.challenges.lock().remove(&self.label);
    }
}

/// A self-signed certificate for `host` with the `acmeIdentifier`
/// extension holding `digest`.
pub fn certificate(host: &str, digest: &[u8]) -> Result<Answer, Error> {
    if digest.len() != 32 {
        return Err(Error::Authorization("given a digest of the wrong size"));
    }
    let mut params = CertificateParams::new(vec![host.to_owned()]).map_err(Error::Challenge)?;
    params.custom_extensions = vec![CustomExtension::new_acme_identifier(digest)];
    let key = KeyPair::generate().map_err(Error::Challenge)?;
    let certificate = params.self_signed(&key).map_err(Error::Challenge)?;
    Ok(Answer {
        certificate: certificate.der().clone(),
        key: PrivatePkcs8KeyDer::from(key.serialize_der()),
    })
}
