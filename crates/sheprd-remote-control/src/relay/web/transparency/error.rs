/// Why a Certificate Transparency query gave no answer. None of these is
/// taken as clean.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the CT search answered something that is not a list of certificates")]
    Answer(#[source] serde_json::Error),
    #[error("the CT search must be reached over HTTPS")]
    Insecure,
    #[error("the CT search answered a certificate without a valid key hash")]
    KeyHash,
    #[error("the CT search answered status {0}")]
    Status(u16),
    #[error("the CT search lists more certificates for this name than are read")]
    TooMany,
    #[error("could not reach the CT search")]
    Unreachable(#[source] ureq::Error),
    #[error("the CT search's address is not valid")]
    Url(#[source] url::ParseError),
    // keep-sorted end
}
