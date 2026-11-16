use std::{io, path::PathBuf};

/// Why the relay did not get a computer its certificate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the ACME server refused or failed")]
    Acme(#[from] instant_acme::Error),
    #[error("the ACME authorization is {0}")]
    Authorization(&'static str),
    #[error("too many certificate orders are running")]
    Busy,
    #[error("the challenge certificate could not be made")]
    Challenge(#[source] rcgen::Error),
    #[error("the CSR is not one the relay signs for")]
    Csr(#[from] Csr),
    #[error("the ACME server offered no TLS-ALPN-01 challenge")]
    NoChallenge,
    #[error("the order ended {0} instead of ready")]
    NotReady(&'static str),
    #[error("this computer asked for a certificate too recently")]
    Rationed,
    #[error("could not read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the ACME account file is not valid")]
    Saved(#[source] serde_json::Error),
    #[error("ordering the certificate took too long")]
    TimedOut,
    #[error("the relay issued its weekly allowance of certificates")]
    Weekly,
    #[error("could not write {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    // keep-sorted end
}

/// Why a computer's CSR was refused before any order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Csr {
    // keep-sorted start
    #[error("the CSR is not P-256 ECDSA")]
    Key,
    #[error("the CSR names something other than the computer's web name")]
    Names,
    #[error("the CSR does not parse or its signature does not hold")]
    Parse,
    #[error("the CSR is larger than the relay reads")]
    TooLong,
    // keep-sorted end
}
