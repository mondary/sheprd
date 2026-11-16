//! What a certificate the relay returns must be before this computer
//! serves with it: for this computer's own key, so the relay cannot have
//! swapped in one whose key it holds; for exactly the web name, chained to
//! a public root; and valid now.

use super::Error;
use rcgen::{KeyPair, PublicKeyData as _};
use rustls::{
    RootCertStore,
    client::{WebPkiServerVerifier, danger::ServerCertVerifier as _},
    crypto::ring,
    pki_types::{CertificateDer, ServerName, UnixTime, pem::PemObject as _},
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// A checked certificate chain and when it is good.
#[derive(Clone, Debug)]
pub(crate) struct Issued {
    pub(crate) chain: Vec<CertificateDer<'static>>,
    pub(crate) not_after: SystemTime,
    pub(crate) not_before: SystemTime,
}

impl Issued {
    /// When to renew: two-thirds of the way through its life, as ACME
    /// clients do, which stays right as lifetimes shrink.
    #[must_use]
    pub(crate) fn renew_at(&self) -> SystemTime {
        let life = self
            .not_after
            .duration_since(self.not_before)
            .unwrap_or_default();
        self.not_before + life * 2 / 3
    }
}

/// The public roots Let's Encrypt and every other public CA chain to.
#[must_use]
pub(crate) fn public_roots() -> RootCertStore {
    RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    }
}

/// Checks `pem` (the chain the relay returned) against `key` and `host`,
/// trusting `roots`, at `now`.
pub(crate) fn check(
    pem: &str,
    key: &KeyPair,
    host: &str,
    roots: &RootCertStore,
    now: SystemTime,
) -> Result<Issued, Error> {
    let chain = CertificateDer::pem_slice_iter(pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Parse)?;
    let (leaf, intermediates) = chain.split_first().ok_or(Error::Empty)?;
    let (_, parsed) = x509_parser::parse_x509_certificate(leaf).map_err(|_| Error::Parse)?;
    if parsed.public_key().raw != key.subject_public_key_info().as_slice() {
        return Err(Error::WrongKey);
    }
    let time = |seconds: i64| {
        u64::try_from(seconds)
            .ok()
            .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds))
            .ok_or(Error::Expired)
    };
    let validity = parsed.validity();
    let issued = Issued {
        chain: chain.clone(),
        not_after: time(validity.not_after.timestamp())?,
        not_before: time(validity.not_before.timestamp())?,
    };
    let verifier = WebPkiServerVerifier::builder_with_provider(
        Arc::new(roots.clone()),
        Arc::new(ring::default_provider()),
    )
    .build()?;
    let name = ServerName::try_from(host.to_owned()).map_err(|_| Error::Parse)?;
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    verifier
        .verify_server_cert(
            leaf,
            intermediates,
            &name,
            &[],
            UnixTime::since_unix_epoch(since),
        )
        .map_err(Error::Untrusted)?;
    Ok(issued)
}
