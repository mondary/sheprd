//! Accepting on the front, and sending each connection where its server
//! name says. Every byte read to find the name is passed on unchanged.

use super::{
    ClientHello, Destination,
    hello::{self, MAX_READ},
    proxy,
};
use crate::{
    hub::Hub,
    metrics::{Front, Offense, Refused},
    serve::capped::{Capped, Held},
    wire::Parse,
};
use axum::serve::Listener as _;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional},
    net::TcpStream,
    time::timeout,
};

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
        // Web sessions and certificate challenges are not served yet, so
        // their names answer as any other unknown name does.
        Destination::Web(_) | Destination::Challenge(_) | Destination::Refuse => {
            hub.metrics.front(Front::Refused);
        }
    }
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
