//! Web access through a private relay: the certificate this computer
//! accepts from it, the files it keeps, the listen token it presents, and a
//! browser reaching the app through a real relay's front, end to end.

use super::*;
use crate::relay::{
    access,
    web::{Error as WebError, check},
};
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::RootCertStore;
use secrecy::SecretString;
use sheprd_relay::{WebLabel, front};

const TOKEN: &str = "a listen token long enough";

/// A test CA standing in for Let's Encrypt.
struct Ca {
    issuer: Issuer<'static, KeyPair>,
    roots: RootCertStore,
}

impl Ca {
    fn new() -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
        params
            .distinguished_name
            .push(DnType::CommonName, "test roots");
        let key = KeyPair::generate().unwrap();
        let certificate = params.self_signed(&key).unwrap();
        let mut roots = RootCertStore::empty();
        roots.add(certificate.der().clone()).unwrap();
        Self {
            issuer: Issuer::new(params, key),
            roots,
        }
    }

    /// A chain for `host` with `key`, valid from `days_ago` for `days`.
    fn issue(&self, host: &str, key: &KeyPair, days_ago: i64, days: i64) -> String {
        let mut params = CertificateParams::new(vec![host.to_owned()]).unwrap();
        let start = time::OffsetDateTime::now_utc() - time::Duration::days(days_ago);
        params.not_before = start;
        params.not_after = start + time::Duration::days(days);
        params.signed_by(key, &self.issuer).unwrap().pem()
    }
}

#[test]
fn only_a_chain_for_this_computers_key_and_name_is_served() {
    let ca = Ca::new();
    let key = KeyPair::generate().unwrap();
    let host = "abc.relay.example.org";
    let now = SystemTime::now();
    let issued = check(&ca.issue(host, &key, 1, 90), &key, host, &ca.roots, now).unwrap();
    let life = issued.not_after.duration_since(issued.not_before).unwrap();
    assert_eq!(issued.renew_at(), issued.not_before + life * 2 / 3);

    let other = KeyPair::generate().unwrap();
    assert!(matches!(
        check(&ca.issue(host, &other, 1, 90), &key, host, &ca.roots, now),
        Err(WebError::WrongKey)
    ));
    assert!(matches!(
        check(
            &ca.issue("other.example.org", &key, 1, 90),
            &key,
            host,
            &ca.roots,
            now
        ),
        Err(WebError::Untrusted(_))
    ));
    assert!(matches!(
        check(&ca.issue(host, &key, 100, 90), &key, host, &ca.roots, now),
        Err(WebError::Untrusted(_))
    ));
    // Not from the trusted roots, as the relay could forge.
    assert!(matches!(
        check(
            &Ca::new().issue(host, &key, 1, 90),
            &key,
            host,
            &ca.roots,
            now
        ),
        Err(WebError::Untrusted(_))
    ));
    assert!(matches!(
        check("", &key, host, &ca.roots, now),
        Err(WebError::Empty)
    ));
}

#[test]
fn a_private_relays_token_is_read_from_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("token");
    std::fs::write(&file, format!("  {TOKEN}\n")).unwrap();
    let endpoint = Endpoint::parse("https://fra1.sheprd-relay.dev").unwrap();
    let relay = Relay {
        token_file: Some(file.clone()),
        web: true,
        ..endpoint.clone().into()
    };
    let loaded = access::load(&[
        relay.clone(),
        Endpoint::parse("https://sfo3.example.org").unwrap().into(),
    ])
    .unwrap();
    let private = &loaded[&endpoint.to_string()];
    assert_eq!(private.token.as_bytes(), TOKEN.as_bytes());
    assert!(private.web);
    assert!(!format!("{private:?}").contains(TOKEN));
    assert!(
        loaded["https://sfo3.example.org"]
            .token
            .as_bytes()
            .is_empty()
    );

    std::fs::write(&file, " \n").unwrap();
    assert!(matches!(
        access::load(std::slice::from_ref(&relay)),
        Err(Error::Token { .. })
    ));
    std::fs::remove_file(&file).unwrap();
    assert!(matches!(
        access::load(&[relay]),
        Err(Error::TokenFile { .. })
    ));
}

/// A private relay on loopback named `localhost`, running its front:
/// its HTTP address, and the front's.
async fn private_relay() -> (SocketAddr, SocketAddr) {
    let (endpoint, info) =
        sheprd_relay::bind_quic("127.0.0.1:0".parse().unwrap(), None, None).unwrap();
    let front_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let front_address = front_listener.local_addr().unwrap();
    let options = RelayOptions {
        front: Some(front_address),
        listen_token: Some(SecretString::from(TOKEN)),
        metrics: None,
        public_host: Some("localhost".into()),
        ..RelayOptions::default()
    };
    let (router, hub) = sheprd_relay::app(&options, Some(info)).unwrap();
    assert!(hub.offers_web());
    tokio::spawn(sheprd_relay::quic::run(endpoint, Arc::clone(&hub)));
    let config = front::Config {
        relay_host: Arc::from("localhost"),
        upstream: None,
    };
    front::spawn(front_listener, hub, config);
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = tcp.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            tcp,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (http, front_address)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_reaches_the_app_through_a_private_relays_front() {
    let (http, front_address) = private_relay().await;
    let dir = tempfile::tempdir().unwrap();
    let token_file = dir.path().join("relay.token");
    std::fs::write(&token_file, TOKEN).unwrap();
    let mut shared = state(dir.path());
    let endpoint = Endpoint::parse(&format!("http://localhost:{}", http.port())).unwrap();
    let relay = Relay {
        token_file: Some(token_file),
        web: true,
        ..endpoint.clone().into()
    };
    let mut relays = Relays::load(dir.path(), vec![relay]).unwrap();
    // The certificate the relay would have obtained, from a test CA this
    // computer trusts instead of the public roots: kept where renewal
    // finds it, so no order is made.
    let ca = Ca::new();
    relays.trust_web_roots(ca.roots.clone());
    let route = client::route(&relays.keys.route);
    let host = endpoint.web_host(&route).unwrap();
    assert_eq!(host, WebLabel::of(&route).host("localhost"));
    let key = KeyPair::generate().unwrap();
    let store = crate::relay::web::store_dir(dir.path(), &endpoint.to_string());
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(store.join("key.der"), key.serialize_der()).unwrap();
    std::fs::write(store.join("chain.pem"), ca.issue(&host, &key, 1, 90)).unwrap();
    let relays = Arc::new(relays);
    Arc::get_mut(&mut shared).unwrap().relay = Some(Arc::clone(&relays));

    let (_stop, stopped) = watch::channel(false);
    relays.start(served(dir.path(), &shared).0, stopped);
    listening(&relays).await;
    let origin = format!("https://{host}");
    for _ in 0..100 {
        if shared.serves(&origin) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(shared.serves(&origin), "the web origin was never served");
    // A page served there learns that the origin is this computer's, as
    // pairing and sign-in tell it, so a Home Screen app takes over.
    assert!(shared.origins().contains(&origin), "{:?}", shared.origins());
    // It is never remembered as an address an app may keep using, so
    // turning web access off takes it back.
    assert!(!shared.network_origins().contains(&origin));

    // A browser trusting the same CA, at the web name, through the front.
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(ca.roots.clone())
    .with_no_client_auth();
    let tcp = tokio::net::TcpStream::connect(front_address).await.unwrap();
    let mut https = tokio_rustls::TlsConnector::from(Arc::new(client))
        .connect(ServerName::try_from(host.clone()).unwrap(), tcp)
        .await
        .unwrap();
    let request =
        format!("GET /manifest.webmanifest HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    https.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    let _ = https.read_to_end(&mut response).await;
    let response = String::from_utf8_lossy(&response);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
}

mod pending_key;
mod transparency;
