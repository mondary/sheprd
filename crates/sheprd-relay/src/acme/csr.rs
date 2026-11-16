//! Checking a computer's CSR before the relay orders anything for it: its
//! signature holds, its key is P-256, and the one name it asks for is the
//! computer's own web name. The relay never sees, and so never needs, the
//! private key.

use super::error::Csr;
use crate::wire::MAX_CSR;
use rcgen::{CertificateSigningRequestParams, PKCS_ECDSA_P256_SHA256, SanType};
use rustls::pki_types::CertificateSigningRequestDer;

/// Checks that `der` is a CSR for exactly `host`, signed by its P-256 key.
pub fn check(der: &[u8], host: &str) -> Result<(), Csr> {
    if der.len() > MAX_CSR {
        return Err(Csr::TooLong);
    }
    let parsed =
        CertificateSigningRequestParams::from_der(&CertificateSigningRequestDer::from(der))
            .map_err(|_| Csr::Parse)?;
    if parsed.public_key.algorithm() != &PKCS_ECDSA_P256_SHA256 {
        return Err(Csr::Key);
    }
    let names = &parsed.params.subject_alt_names;
    let only_host = matches!(names.as_slice(), [SanType::DnsName(name)] if name.as_str() == host);
    if !only_host {
        return Err(Csr::Names);
    }
    Ok(())
}
