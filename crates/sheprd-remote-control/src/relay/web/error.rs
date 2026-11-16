use std::{io, path::PathBuf};

/// Why this computer has no web certificate on a relay.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the web certificate's file work was cancelled")]
    Cancelled,
    #[error("the relay returned no certificate")]
    Empty,
    #[error("the relay's certificate has no usable validity")]
    Expired,
    #[error("the relay could not issue the certificate")]
    Issue(#[source] sheprd_relay::client::Error),
    #[error("could not make the web key or its CSR")]
    Key(#[from] rcgen::Error),
    #[error("the relay is reached by address, so it gives no web name")]
    NoName,
    #[error("the relay's certificate does not parse")]
    Parse,
    #[error("could not read the web certificate in {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("there are no roots to check the web certificate against")]
    Roots(#[from] rustls::client::VerifierBuilderError),
    #[error("could not save the web certificate in {}", path.display())]
    Save {
        path: PathBuf,
        #[source]
        source: crate::auth::Error,
    },
    #[error("could not set up TLS with the web certificate")]
    Tls(#[from] rustls::Error),
    #[error("the relay's certificate does not chain to a public root for this name")]
    Untrusted(#[source] rustls::Error),
    #[error("the relay's certificate is for another key than this computer's")]
    WrongKey,
    // keep-sorted end
}
