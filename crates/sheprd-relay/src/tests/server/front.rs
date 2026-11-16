//! The TCP front over real sockets: the relay's own name reaches the
//! upstream behind a PROXY header with every byte intact; anything else is
//! closed without reaching it.

use super::front_hello::real_hello;
use crate::{
    Limits, RouteId, WebLabel,
    front::{self, Config, proxy, relay_host},
    hub::{Access, Hub},
    serve::capped::Capped,
};
use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

const RELAY: &str = "fra1.sheprd-relay.dev";
const WAIT: Duration = Duration::from_secs(5);

/// A front on a free port, its hub, and the upstream it forwards to.
async fn start() -> (SocketAddr, Arc<Hub>, TcpListener) {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hub = Arc::new(Hub::new(Limits::default(), Access::Open));
    let listener = Capped::new(
        TcpListener::bind("127.0.0.1:0").await.unwrap(),
        16,
        Some(16),
        Duration::from_secs(60),
    );
    let address = axum::serve::Listener::local_addr(&listener).unwrap();
    let config = Config {
        relay_host: Arc::from(RELAY),
        upstream: Some(Arc::from(upstream.local_addr().unwrap().to_string())),
    };
    tokio::spawn(front::run(listener, Arc::clone(&hub), config));
    (address, hub, upstream)
}

fn counted(hub: &Hub, outcome: &str) -> bool {
    hub.metrics.render().contains(&format!(
        "sheprd_relay_front_total{{outcome=\"{outcome}\"}} 1"
    ))
}

#[tokio::test]
async fn the_relays_own_name_reaches_the_upstream_intact() {
    let (address, hub, upstream) = start().await;
    let mut client = TcpStream::connect(address).await.unwrap();
    let mut sent = real_hello(RELAY, &[b"h2"]);
    // Bytes after the hello in the same write go along too.
    sent.extend_from_slice(b"\x14\x03\x03\x00\x01\x01");
    client.write_all(&sent).await.unwrap();

    let (mut behind, _) = timeout(WAIT, upstream.accept()).await.unwrap().unwrap();
    let header = proxy::header(client.local_addr().unwrap(), address);
    let mut received = vec![0; header.len() + sent.len()];
    timeout(WAIT, behind.read_exact(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&received[..header.len()], header.as_slice());
    assert_eq!(&received[header.len()..], sent.as_slice());

    behind.write_all(b"server bytes").await.unwrap();
    let mut answer = [0; 12];
    timeout(WAIT, client.read_exact(&mut answer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&answer, b"server bytes");
    assert!(counted(&hub, "relay"));
}

#[tokio::test]
async fn other_names_are_closed_without_reaching_the_upstream() {
    let (address, hub, upstream) = start().await;
    let label = WebLabel::of(&RouteId::from_bytes([1; 32]));
    for name in ["example.org".to_owned(), label.host(RELAY)] {
        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(&real_hello(&name, &[])).await.unwrap();
        let mut byte = [0; 1];
        let read = timeout(WAIT, client.read(&mut byte)).await.unwrap();
        assert!(matches!(read, Ok(0) | Err(_)), "{name} stayed open");
    }
    assert!(
        timeout(Duration::from_millis(200), upstream.accept())
            .await
            .is_err()
    );
    assert!(
        hub.metrics
            .render()
            .contains("sheprd_relay_front_total{outcome=\"refused\"} 2")
    );
}

#[tokio::test]
async fn bytes_that_are_not_tls_are_closed_and_counted() {
    let (address, hub, upstream) = start().await;
    let mut client = TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut byte = [0; 1];
    let read = timeout(WAIT, client.read(&mut byte)).await.unwrap();
    assert!(matches!(read, Ok(0) | Err(_)));
    assert!(
        timeout(Duration::from_millis(200), upstream.accept())
            .await
            .is_err()
    );
    assert!(counted(&hub, "malformed"));
}

#[test]
fn proxy_headers_carry_both_addresses_and_ports() {
    let from = SocketAddr::from((Ipv4Addr::new(203, 0, 113, 7), 50_000));
    let to = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 2), 443));
    let header = proxy::header(from, to);
    assert_eq!(&header[..12], b"\r\n\r\n\0\r\nQUIT\n");
    assert_eq!(header[12..16], [0x21, 0x11, 0, 12]);
    assert_eq!(header[16..20], [203, 0, 113, 7]);
    assert_eq!(header[20..24], [10, 0, 0, 2]);
    assert_eq!(header[24..28], [0xc3, 0x50, 1, 0xbb]);
    assert_eq!(header.len(), 28);

    // Mixed families are both written as IPv6.
    let to = SocketAddr::from((Ipv6Addr::LOCALHOST, 443));
    let header = proxy::header(from, to);
    assert_eq!(header[12..16], [0x21, 0x21, 0, 36]);
    assert_eq!(
        header[16..32],
        Ipv4Addr::new(203, 0, 113, 7).to_ipv6_mapped().octets()
    );
    assert_eq!(header.len(), 16 + 36);
}

#[test]
fn the_front_answers_only_as_a_host_name() {
    assert_eq!(relay_host("Fra1.Sheprd-Relay.dev"), Some(RELAY.into()));
    assert_eq!(
        relay_host("relay.example.org:8443/sheprd"),
        Some("relay.example.org".into())
    );
    assert_eq!(relay_host("203.0.113.7"), None);
    assert_eq!(relay_host("[2001:db8::1]:443"), None);
    assert_eq!(relay_host(""), None);
}
