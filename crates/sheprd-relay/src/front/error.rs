/// Why the front could not start, or turned a connection away before
/// routing it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the ClientHello is malformed")]
    Malformed,
    #[error("the front needs --public-host to be a host name, not an address")]
    NoHost,
    #[error("the first handshake message is not a ClientHello")]
    NotClientHello,
    #[error("the first record is not a handshake")]
    NotHandshake,
    #[error("a TLS record has a bad length")]
    Record,
    #[error("the ClientHello repeats an extension or a host name")]
    Repeated,
    #[error("the server name is not a DNS host name")]
    ServerName,
    #[error("the ClientHello is longer than the front reads")]
    TooLong,
    #[error("bytes follow the ClientHello")]
    Trailing,
    #[error("the TLS version is not 3.x")]
    Version,
    // keep-sorted end
}
