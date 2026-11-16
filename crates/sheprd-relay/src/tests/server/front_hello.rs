//! The front's ClientHello reader and where it routes a connection.

use crate::{
    RouteId, WebLabel,
    front::{
        ACME_TLS_ALPN, ClientHello, Destination, Error,
        hello::{MAX_HELLO, MAX_READ, read},
    },
    pin::provider,
    wire::Parse,
};
use std::sync::Arc;

const RELAY: &str = "fra1.sheprd-relay.dev";

/// The first flight a real TLS client sends to `name`, offering `alpn`.
pub(super) fn real_hello(name: &str, alpn: &[&[u8]]) -> Vec<u8> {
    let mut config = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
    let name = rustls::pki_types::ServerName::try_from(name.to_owned()).unwrap();
    let mut client = rustls::ClientConnection::new(Arc::new(config), name).unwrap();
    let mut bytes = Vec::new();
    client.write_tls(&mut bytes).unwrap();
    bytes
}

/// A ClientHello body with `extensions`, each a type and its data.
fn body(extensions: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut body = vec![3, 3];
    body.extend([0; 32]);
    body.push(0);
    body.extend([0, 2, 0x13, 0x01]);
    body.extend([1, 0]);
    let mut all = Vec::new();
    for (kind, data) in extensions {
        all.extend(kind.to_be_bytes());
        all.extend(u16::try_from(data.len()).unwrap().to_be_bytes());
        all.extend(data);
    }
    body.extend(u16::try_from(all.len()).unwrap().to_be_bytes());
    body.extend(all);
    body
}

/// `body` as a ClientHello handshake message.
fn handshake(body: &[u8]) -> Vec<u8> {
    let length = u32::try_from(body.len()).unwrap().to_be_bytes();
    [&[1], &length[1..], body].concat()
}

/// `handshake` in records of at most `size` bytes.
fn records(handshake: &[u8], size: usize) -> Vec<u8> {
    handshake
        .chunks(size)
        .flat_map(|chunk| {
            let length = u16::try_from(chunk.len()).unwrap().to_be_bytes();
            [&[22, 3, 1], &length[..], chunk].concat()
        })
        .collect()
}

fn server_name(names: &[(u8, &[u8])]) -> Vec<u8> {
    let mut list = Vec::new();
    for (kind, name) in names {
        list.push(*kind);
        list.extend(u16::try_from(name.len()).unwrap().to_be_bytes());
        list.extend(*name);
    }
    [&u16::try_from(list.len()).unwrap().to_be_bytes()[..], &list].concat()
}

fn done(bytes: &[u8]) -> Result<ClientHello, Error> {
    match read(bytes)? {
        Parse::Done(hello, used) => {
            assert!(used <= bytes.len());
            Ok(hello)
        }
        Parse::Need(total) => panic!("needs {total} of {}", bytes.len()),
    }
}

#[test]
fn a_real_client_hello_gives_its_name_and_protocols() {
    let bytes = real_hello("Example.ORG", &[b"h2", b"http/1.1"]);
    let hello = done(&bytes).unwrap();
    assert_eq!(hello.server_name.as_deref(), Some("example.org"));
    assert_eq!(hello.alpn, vec![b"h2".to_vec(), b"http/1.1".to_vec()]);
    assert!(hello.offers(b"h2"));
    assert!(!hello.offers(ACME_TLS_ALPN));
}

#[test]
fn every_prefix_asks_for_more_until_the_whole_hello() {
    let bytes = real_hello("example.org", &[b"h2"]);
    for end in 0..bytes.len() {
        match read(&bytes[..end]).unwrap() {
            Parse::Need(total) => assert!(total > end && total <= MAX_READ, "{end}"),
            Parse::Done(..) => panic!("done after {end} of {}", bytes.len()),
        }
    }
    assert!(matches!(read(&bytes), Ok(Parse::Done(_, used)) if used == bytes.len()));
}

#[test]
fn a_hello_split_into_tiny_records_still_reads() {
    let handshake = handshake(&body(&[(0, server_name(&[(0, b"a.example")]))]));
    let hello = done(&records(&handshake, 1)).unwrap();
    assert_eq!(hello.server_name.as_deref(), Some("a.example"));
}

#[test]
fn what_follows_the_hello_is_left_for_the_peer() {
    let mut bytes = real_hello("example.org", &[]);
    let hello_len = bytes.len();
    bytes.extend([20, 3, 3, 0, 1, 1]);
    assert!(matches!(read(&bytes), Ok(Parse::Done(_, used)) if used == hello_len));
}

#[test]
fn a_hello_without_extensions_or_a_name_names_nothing() {
    let mut plain = body(&[]);
    plain.truncate(plain.len() - 2);
    assert_eq!(
        done(&records(&handshake(&plain), 512)),
        Ok(ClientHello::default())
    );
    let unnamed = body(&[(0x2b, vec![2, 3, 4])]);
    assert_eq!(
        done(&records(&handshake(&unnamed), 512))
            .unwrap()
            .server_name,
        None
    );
}

#[test]
fn malformed_records_are_refused() {
    let good = records(&handshake(&body(&[])), 512);
    let mut alert = good.clone();
    alert[0] = 21;
    assert_eq!(read(&alert), Err(Error::NotHandshake));
    let mut version = good.clone();
    version[1] = 2;
    assert_eq!(read(&version), Err(Error::Version));
    assert_eq!(read(&[22, 3, 1, 0, 0]), Err(Error::Record));
    assert_eq!(read(&[22, 3, 1, 0x40, 1]), Err(Error::Record));
    let mut not_hello = handshake(&body(&[]));
    not_hello[0] = 2;
    assert_eq!(read(&records(&not_hello, 512)), Err(Error::NotClientHello));
    let mut trailing = handshake(&body(&[]));
    trailing.push(0);
    assert_eq!(read(&records(&trailing, 512)), Err(Error::Trailing));
}

#[test]
fn oversized_hellos_are_refused_before_they_are_read() {
    let length = u32::try_from(MAX_HELLO + 1).unwrap().to_be_bytes();
    let header = [&[1], &length[1..]].concat();
    assert_eq!(read(&records(&header, 512)), Err(Error::TooLong));
    // Within the size, but in records too small to be worth reading.
    let padding = vec![0; MAX_HELLO - 200];
    let big = handshake(&body(&[(0xffce, padding)]));
    assert_eq!(read(&records(&big, 1)[..MAX_READ + 6]), Err(Error::TooLong));
    assert!(done(&records(&big, 1 << 14)).is_ok());
}

#[test]
fn repeated_or_bad_names_are_refused() {
    let twice = body(&[
        (0, server_name(&[(0, b"a.example")])),
        (0, server_name(&[])),
    ]);
    assert_eq!(
        read(&records(&handshake(&twice), 512)),
        Err(Error::Repeated)
    );
    let two_names = body(&[(0, server_name(&[(0, b"a.example"), (0, b"b.example")]))]);
    assert_eq!(
        read(&records(&handshake(&two_names), 512)),
        Err(Error::Repeated)
    );
    for bad in [
        &b"a_b.example"[..],
        b"example.",
        b"-a.example",
        b"a..b",
        b"",
        b"\xff.x",
    ] {
        let hello = body(&[(0, server_name(&[(0, bad)]))]);
        assert_eq!(
            read(&records(&handshake(&hello), 512)),
            Err(Error::ServerName),
            "{bad:?}"
        );
    }
    // Other name types are skipped, not mistaken for the host.
    let other = body(&[(0, server_name(&[(9, b"x"), (0, b"a.example")]))]);
    let hello = done(&records(&handshake(&other), 512)).unwrap();
    assert_eq!(hello.server_name.as_deref(), Some("a.example"));
}

#[test]
fn every_truncation_of_a_hello_body_is_refused_not_misread() {
    let full = body(&[
        (0, server_name(&[(0, b"a.example")])),
        (16, vec![0, 3, 2, b'h', b'2']),
    ]);
    // Cut right before the extensions, it is a hello with none.
    let no_extensions = full.len() - 2 - u16::from_be_bytes([full[41], full[42]]) as usize;
    assert_eq!(no_extensions, 41);
    for end in 0..full.len() - 1 {
        let cut = records(&handshake(&full[..end]), 512);
        if end == no_extensions {
            assert_eq!(done(&cut), Ok(ClientHello::default()));
        } else {
            assert!(read(&cut).is_err(), "{end}");
        }
    }
}

#[test]
fn routing_follows_the_name() {
    let route = RouteId::from_bytes([4; 32]);
    let label = WebLabel::of(&route);
    let to = |name: Option<String>, alpn: &[&[u8]]| {
        let hello = ClientHello {
            alpn: alpn.iter().map(|protocol| protocol.to_vec()).collect(),
            server_name: name,
        };
        Destination::of(&hello, RELAY)
    };
    assert_eq!(to(Some(RELAY.into()), &[b"h2"]), Destination::Relay);
    assert_eq!(
        to(Some(label.host(RELAY)), &[b"h2"]),
        Destination::Web(label)
    );
    assert_eq!(
        to(Some(label.host(RELAY)), &[ACME_TLS_ALPN]),
        Destination::Challenge(label)
    );
    assert_eq!(to(None, &[]), Destination::Refuse);
    assert_eq!(to(Some("example.org".into()), &[]), Destination::Refuse);
    assert_eq!(to(Some(format!("x.{RELAY}")), &[]), Destination::Refuse);
    assert_eq!(
        to(Some(format!("{label}.evil{RELAY}")), &[]),
        Destination::Refuse
    );
    assert_eq!(
        to(Some(format!("x.{}", label.host(RELAY))), &[]),
        Destination::Refuse
    );
}
