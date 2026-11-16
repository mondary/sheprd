//! Accepting on the front, and sending each connection where its server
//! name says. Every byte read to find the name is passed on unchanged.

use super::{
    ACME_TLS_ALPN, ClientHello, Destination,
    hello::{self, MAX_READ},
    proxy,
    replay::Replay,
};
use crate::{
    WebLabel,
    acme::Answer,
    broker,
    hub::Hub,
    metrics::{Front, Offense, Refused},
    pin::provider,
    serve::capped::{Capped, Held},
    splice::{Peer, splice},
    wire::Parse,
};
use axum::{body::Bytes, serve::Listener as _};
use rustls::{pki_types::PrivateKeyDer, server::ResolvesServerCert, sign::CertifiedKey};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional},
    net::TcpStream,
    time::timeout,
};
use tokio_rustls::TlsAcceptor;

/// How long a caller has to send its whole ClientHello. A real client
/// sends it at once; slow networks still manage well within this.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// How long reaching the upstream may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// What the front routes by.
#[derive(Clone, Debug)]
pub struct Config {
    /// The relay's own host name, lowercase: the name it answers as, and
    /// the parent of every computer's web name.
    pub relay_host: Arc<str>,
    /// Where the relay's own name goes, `HOST:PORT`; without one, it is
    /// refused.
    pub upstream: Option<Arc<str>>,
}

/// Accepts connections until the task is dropped.
pub async fn run(mut listener: Capped, hub: Arc<Hub>, config: Config) {
    loop {
        let (held, peer) = listener.accept().await;
        if hub.is_banned(peer.ip()) {
            hub.metrics.refuse(Refused::Banned);
            continue;
        }
        tokio::spawn(connection(held, peer, Arc::clone(&hub), config.clone()));
    }
}

/// Why no ClientHello was read.
enum Unread {
    /// The caller closed or broke the connection first.
    Closed,
    /// The bytes are not a ClientHello the front accepts.
    Malformed,
}

async fn connection(mut held: Held, peer: SocketAddr, hub: Arc<Hub>, config: Config) {
    let address = peer.ip();
    let (hello, buffered) = match timeout(HELLO_TIMEOUT, read_hello(&mut held)).await {
        Ok(Ok(read)) => read,
        Ok(Err(Unread::Closed)) => return,
        Ok(Err(Unread::Malformed)) => {
            hub.metrics.front(Front::Malformed);
            hub.offend(address, Offense::Malformed);
            return;
        }
        // Not an offense: a phone on a poor network can be this slow, and
        // the listener's caps already bound how many may wait.
        Err(_) => return hub.metrics.front(Front::Timeout),
    };
    match Destination::of(&hello, &config.relay_host) {
        Destination::Relay => {
            forward(held, &buffered, peer, &hub, config.upstream.as_deref()).await
        }
        Destination::Web(label) => web(held, buffered, address, &hub, label).await,
        Destination::Challenge(label) => challenge(held, buffered, &hub, label).await,
        Destination::Refuse => hub.metrics.front(Front::Refused),
    }
}

/// Answers a TLS-ALPN-01 validation for `label` with its challenge
/// certificate, while the relay orders that computer's certificate. With
/// no order pending, it is closed as any unknown name.
async fn challenge(held: Held, buffered: Vec<u8>, hub: &Hub, label: WebLabel) {
    let answer = hub
        .issuer()
        .and_then(|issuer| issuer.challenges.get(&label));
    let Some(config) = answer.and_then(|answer| challenge_config(&answer).ok()) else {
        return hub.metrics.front(Front::Refused);
    };
    hub.metrics.front(Front::Challenge);
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let _ = timeout(HELLO_TIMEOUT, async move {
        let mut tls = acceptor.accept(Replay::new(buffered, held)).await.ok()?;
        tls.shutdown().await.ok()
    })
    .await;
}

/// A TLS server that offers only `acme-tls/1` and the challenge's
/// certificate (RFC 8737, 3). Served through a fixed resolver: rustls
/// refuses to load, as its own, a certificate whose critical
/// `acmeIdentifier` extension it does not know, and only the validation
/// server reads it.
fn challenge_config(answer: &Answer) -> Result<rustls::ServerConfig, rustls::Error> {
    let provider = provider();
    let key = provider
        .key_provider
        .load_private_key(PrivateKeyDer::Pkcs8(answer.key.clone_key()))?;
    let certified = CertifiedKey::new(vec![answer.certificate.clone()], key);
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(Fixed(Arc::new(certified))));
    config.alpn_protocols = vec![ACME_TLS_ALPN.to_vec()];
    Ok(config)
}

/// Always the one certificate.
#[derive(Debug)]
struct Fixed(Arc<CertifiedKey>);

impl ResolvesServerCert for Fixed {
    fn resolve(&self, _: rustls::server::ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

/// Carries a browser's connection to the computer at `label`, starting
/// with the bytes already read, if it turned web access on. Any refusal
/// closes the connection the same way, so a browser cannot tell an
/// unknown name from one whose computer is offline or busy.
async fn web(held: Held, buffered: Vec<u8>, address: IpAddr, hub: &Arc<Hub>, label: WebLabel) {
    let Ok((accepted, session)) = broker::dial_web(hub, address, label).await else {
        return hub.metrics.front(Front::Refused);
    };
    hub.metrics.front(Front::Web);
    let _held = (accepted.permit, accepted.window);
    let browser = Peer::Tcp {
        prefix: Bytes::from(buffered),
        stream: Box::new(held),
    };
    splice(browser, accepted.peer, session).await;
}

/// Reads until a whole ClientHello is in, returning it and every byte
/// read, which may run past it.
async fn read_hello(held: &mut Held) -> Result<(ClientHello, Vec<u8>), Unread> {
    let mut bytes = Vec::with_capacity(2048);
    loop {
        match hello::read(&bytes).map_err(|_| Unread::Malformed)? {
            Parse::Done(hello, _) => return Ok((hello, bytes)),
            Parse::Need(_) => {
                // `read` never needs more than MAX_READ, so there is room.
                let room = MAX_READ.saturating_sub(bytes.len());
                let limit = u64::try_from(room).unwrap_or(u64::MAX);
                let read = (&mut *held)
                    .take(limit)
                    .read_buf(&mut bytes)
                    .await
                    .map_err(|_| Unread::Closed)?;
                if read == 0 {
                    return Err(Unread::Closed);
                }
            }
        }
    }
}

/// Passes the connection to `upstream`, after a PROXY header naming the
/// caller and the bytes already read.
async fn forward(
    mut held: Held,
    buffered: &[u8],
    peer: SocketAddr,
    hub: &Hub,
    upstream: Option<&str>,
) {
    let Some(upstream) = upstream else {
        return hub.metrics.front(Front::Refused);
    };
    let connected = timeout(CONNECT_TIMEOUT, TcpStream::connect(upstream)).await;
    let Ok(Ok(mut stream)) = connected else {
        hub.metrics.front(Front::Unreachable);
        tracing::warn!("the front could not reach its upstream");
        return;
    };
    let local = held.local_addr().unwrap_or(peer);
    let mut first = proxy::header(peer, local);
    first.extend_from_slice(buffered);
    if stream.write_all(&first).await.is_err() {
        return hub.metrics.front(Front::Unreachable);
    }
    hub.metrics.front(Front::Relay);
    let _ = copy_bidirectional(&mut held, &mut stream).await;
}
