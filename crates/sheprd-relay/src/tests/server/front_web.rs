//! Web access through the front: a browser's connection to a computer's
//! web name reaches that computer's listener byte for byte, only while it
//! has web access on, and within the route's web limit.

use super::{front_hello::real_hello, signing_key};
use crate::{
    Limits, RouteId, WebLabel, broker,
    front::{self, Config},
    hub::{Accepted, Access, Caller, DialRefusal, Hub, Registration},
    serve::capped::Capped,
    splice::Peer,
    wire::{Message, Status},
};
use axum::{body::Bytes, serve::Listener as _};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
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
const HOME: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// A front on a free port, routing for `hub`.
async fn front(hub: &Arc<Hub>) -> SocketAddr {
    let listener = Capped::new(
        TcpListener::bind("127.0.0.1:0").await.unwrap(),
        16,
        Some(16),
        Duration::from_secs(60),
    );
    let address = listener.local_addr().unwrap();
    let config = Config {
        relay_host: Arc::from(RELAY),
        upstream: None,
    };
    tokio::spawn(front::run(listener, Arc::clone(hub), config));
    address
}

/// A private relay's hub, running its front: one that offers web access.
fn hub(limits: Limits) -> Arc<Hub> {
    let access = Access::tokens("a listen token long enough").unwrap();
    Arc::new(Hub::new(limits, access).with_web(true))
}

/// A listener on the route of key `seed`, with web access on.
fn listening(hub: &Arc<Hub>, seed: u8) -> (Registration, WebLabel) {
    let route = RouteId::of(signing_key(seed).verifying_key());
    let registration = hub.register(route, HOME).unwrap();
    assert!(registration.set_web(true));
    (registration, WebLabel::of(&route))
}

/// Accepts the next web dial as the computer would, returning the
/// computer's end of the session.
async fn accept_web(hub: &Arc<Hub>, registration: &mut Registration) -> TcpStream {
    let incoming = timeout(WAIT, registration.incoming.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(incoming.caller, Caller::Web(_)));
    let sender = hub.accept(incoming.session).unwrap();
    // The computer's side, as a TCP peer the splice carries.
    let mut side = Capped::new(
        TcpListener::bind("127.0.0.1:0").await.unwrap(),
        4,
        None,
        Duration::from_secs(60),
    );
    let computer = TcpStream::connect(side.local_addr().unwrap())
        .await
        .unwrap();
    let (held, _) = side.accept().await;
    let accepted = Accepted {
        peer: Peer::Tcp {
            prefix: Bytes::new(),
            stream: Box::new(held),
        },
        permit: None,
        window: None,
    };
    assert!(sender.send(accepted).is_ok());
    computer
}

async fn closed(stream: &mut TcpStream) -> bool {
    let mut byte = [0; 1];
    matches!(
        timeout(WAIT, stream.read(&mut byte)).await,
        Ok(Ok(0) | Err(_))
    )
}

#[tokio::test]
async fn a_browser_reaches_the_computer_byte_for_byte() {
    let hub = hub(Limits::default());
    let address = front(&hub).await;
    let (mut registration, label) = listening(&hub, 1);

    let mut browser = TcpStream::connect(address).await.unwrap();
    let mut sent = real_hello(&label.host(RELAY), &[b"h2"]);
    sent.extend_from_slice(b"early bytes");
    browser.write_all(&sent).await.unwrap();

    let mut computer = accept_web(&hub, &mut registration).await;
    let mut received = vec![0; sent.len()];
    timeout(WAIT, computer.read_exact(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, sent);

    computer.write_all(b"server flight").await.unwrap();
    let mut answer = [0; 13];
    timeout(WAIT, browser.read_exact(&mut answer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&answer, b"server flight");
    assert!(
        hub.metrics
            .render()
            .contains("sheprd_relay_front_total{outcome=\"web\"} 1")
    );

    // Turning web access off ends the session.
    assert!(registration.set_web(false));
    assert!(closed(&mut browser).await);
}

#[tokio::test]
async fn a_new_listener_on_the_route_ends_the_old_ones_browsers() {
    let hub = hub(Limits::default());
    let address = front(&hub).await;
    let (mut registration, label) = listening(&hub, 6);

    let mut browser = TcpStream::connect(address).await.unwrap();
    let hello = real_hello(&label.host(RELAY), &[]);
    browser.write_all(&hello).await.unwrap();
    let mut computer = accept_web(&hub, &mut registration).await;
    let mut received = vec![0; hello.len()];
    timeout(WAIT, computer.read_exact(&mut received))
        .await
        .unwrap()
        .unwrap();

    // The computer reconnects, with web access off this time.
    let route = RouteId::of(signing_key(6).verifying_key());
    let replaced = hub.register(route, HOME).unwrap();
    assert!(closed(&mut browser).await);
    assert!(replaced.set_web(false));
}

#[tokio::test]
async fn without_web_access_the_name_is_closed_like_any_other() {
    let hub = hub(Limits::default());
    let address = front(&hub).await;
    let (registration, label) = listening(&hub, 2);
    assert!(registration.set_web(false));

    let mut browser = TcpStream::connect(address).await.unwrap();
    browser
        .write_all(&real_hello(&label.host(RELAY), &[]))
        .await
        .unwrap();
    assert!(closed(&mut browser).await);
    assert!(
        hub.metrics
            .render()
            .contains("sheprd_relay_front_total{outcome=\"refused\"} 1")
    );
}

#[test]
fn web_names_follow_their_listener() {
    let hub = hub(Limits::default());
    let (registration, label) = listening(&hub, 3);
    let route = RouteId::of(signing_key(3).verifying_key());
    assert!(matches!(hub.dial_web(label, HOME), Ok(Ok(_))));

    // A new listener on the route starts with web access off.
    let replaced = hub.register(route, HOME).unwrap();
    assert!(matches!(
        hub.dial_web(label, HOME),
        Ok(Err(DialRefusal::Offline))
    ));
    assert!(replaced.set_web(true));
    // The old registration no longer speaks for the route.
    assert!(!registration.set_web(false));
    assert!(matches!(hub.dial_web(label, HOME), Ok(Ok(_))));

    // Leaving takes the name with it.
    drop(replaced);
    assert!(matches!(
        hub.dial_web(label, HOME),
        Ok(Err(DialRefusal::Offline))
    ));
}

#[test]
fn a_route_holds_at_most_its_web_limit() {
    let limits = Limits {
        max_web_sessions_per_route: 2,
        ..Limits::default()
    };
    let hub = hub(limits);
    let (_registration, label) = listening(&hub, 4);
    let first = hub.dial_web(label, HOME).unwrap().unwrap();
    let _second = hub.dial_web(label, HOME).unwrap().unwrap();
    assert!(matches!(
        hub.dial_web(label, HOME),
        Ok(Err(DialRefusal::Busy))
    ));
    // A device's dial still has room on the route.
    drop(first);
    assert!(matches!(hub.dial_web(label, HOME), Ok(Ok(_))));
}

#[test]
fn only_a_private_relay_with_a_front_offers_web_access() {
    let public = Arc::new(Hub::new(Limits::default(), Access::Open).with_web(true));
    let private = Access::tokens("a listen token long enough").unwrap();
    let frontless = Arc::new(Hub::new(Limits::default(), private).with_web(false));
    for hub in [public, frontless] {
        assert!(!hub.offers_web());
        let route = RouteId::of(signing_key(5).verifying_key());
        let registration = hub.register(route, HOME).unwrap();
        assert_eq!(
            broker::apply(&hub, &registration, Message::Web(true)),
            Status::Forbidden
        );
        assert!(!registration.set_web(true));
        // Turning it off is always fine: there is nothing to end.
        assert_eq!(
            broker::apply(&hub, &registration, Message::Web(false)),
            Status::Ok
        );
        assert!(matches!(
            hub.dial_web(WebLabel::of(&route), HOME),
            Ok(Err(DialRefusal::Offline))
        ));
    }
}
