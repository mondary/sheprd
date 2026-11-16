/// Why a passkey registration or assertion was refused. Phones see one
/// text for every check, so a forger learns nothing from which failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the passkey's data is not valid")]
    Cbor,
    #[error("the passkey's data is not valid")]
    ClientData(#[source] serde_json::Error),
    #[error("the passkey was used on another page")]
    CrossOrigin,
    #[error("the passkey's data is not valid")]
    Encoding,
    #[error("the passkey's key is not a P-256 key for ES256")]
    Key,
    #[error("the passkey's data is not valid")]
    Malformed,
    #[error("the passkey made no credential")]
    NoCredential,
    #[error("this device has no passkey for this address; pair it here again")]
    NotRegistered,
    #[error("the passkey was not used by a person")]
    NotPresent,
    #[error("the passkey did not verify who is using it")]
    NotVerified,
    #[error("this address needs a passkey")]
    Required,
    #[error("the passkey's signature does not match")]
    Signature,
    #[error("the passkey's data is too long")]
    TooLarge,
    #[error("the passkey answered another request")]
    WrongChallenge,
    /// The authenticator's counter did not move forward: two copies of one
    /// credential that does not sync, one of them cloned.
    #[error("the passkey's counter went backwards")]
    WrongCounter,
    #[error("the passkey belongs to another address")]
    WrongOrigin,
    #[error("the passkey belongs to another address")]
    WrongRelyingParty,
    #[error("the passkey answered another kind of request")]
    WrongType,
    // keep-sorted end
}
