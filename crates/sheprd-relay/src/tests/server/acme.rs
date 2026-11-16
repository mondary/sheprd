//! Web certificates on the relay: which CSRs it orders for, how often, and
//! the TLS-ALPN-01 answer its front gives while an order waits.

use super::front_hello::real_hello;
use crate::{
    Limits, RouteId, WebLabel,
    acme::{self, Csr, Error, Issuer, Settings, certificate, csr, ration::Ration},
    front::{self, ACME_TLS_ALPN, Config},
    hub::{Access, Hub},
    pin::provider,
    serve::capped::Capped,
};
use axum::serve::Listener as _;
use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_rustls::TlsConnector;

const RELAY: &str = "fra1.sheprd-relay.dev";
/// The `acmeIdentifier` extension's OID, 1.3.6.1.5.5.7.1.31, as DER.
const ACME_IDENTIFIER: [u8; 10] = [0x06, 0x08, 0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x01, 0x1f];

fn csr_for(names: &[&str], key: &KeyPair) -> Vec<u8> {
    let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    let params = CertificateParams::new(names).unwrap();
    params.serialize_request(key).unwrap().der().to_vec()
}

fn host() -> String {
    WebLabel::of(&RouteId::from_bytes([3; 32])).host(RELAY)
}

#[test]
fn only_a_p256_csr_for_the_web_name_alone_is_signed_for() {
    let key = KeyPair::generate().unwrap();
    let host = host();
    assert_eq!(csr::check(&csr_for(&[&host], &key), &host), Ok(()));
    assert_eq!(
        csr::check(&csr_for(&["example.org"], &key), &host),
        Err(Csr::Names)
    );
    assert_eq!(
        csr::check(&csr_for(&[&host, "example.org"], &key), &host),
        Err(Csr::Names)
    );
    let ed25519 = KeyPair::generate_for(&PKCS_ED25519).unwrap();
    assert_eq!(
        csr::check(&csr_for(&[&host], &ed25519), &host),
        Err(Csr::Key)
    );
    assert_eq!(csr::check(b"not a csr", &host), Err(Csr::Parse));
    assert_eq!(csr::check(&[0; 4096], &host), Err(Csr::TooLong));
    let mut forged = csr_for(&[&host], &key);
    let last = forged.len() - 1;
    forged[last] ^= 1;
    assert_eq!(csr::check(&forged, &host), Err(Csr::Parse));
}

#[test]
fn a_computer_waits_between_attempts_and_longer_after_a_certificate() {
    let mut ration = Ration::new(2);
    let (first, second, third) = (
        RouteId::from_bytes([1; 32]),
        RouteId::from_bytes([2; 32]),
        RouteId::from_bytes([3; 32]),
    );
    let start = Instant::now();
    assert!(ration.attempt(first, start).is_ok());
    ration.failed();
    // A failed attempt holds the computer back for the retry delay only.
    assert!(matches!(ration.attempt(first, start), Err(Error::Rationed)));
    let later = start + acme::ration::RETRY;
    assert!(ration.attempt(first, later).is_ok());
    ration.issued(first, later);
    assert!(matches!(
        ration.attempt(first, later + acme::ration::RETRY),
        Err(Error::Rationed)
    ));
    assert!(
        ration
            .attempt(first, later + acme::ration::PER_COMPUTER)
            .is_ok()
    );
    ration.failed();
    // The relay's weekly allowance holds across computers.
    assert!(ration.attempt(second, later).is_ok());
    ration.issued(second, later);
    assert!(matches!(ration.attempt(third, later), Err(Error::Weekly)));
    let next_week = later + Duration::from_secs(7 * 24 * 60 * 60);
    assert!(ration.attempt(third, next_week).is_ok());
}

#[test]
fn a_challenge_certificate_carries_the_acme_identifier() {
    let answer = certificate(&host(), &[7; 32]).unwrap();
    let der = answer.certificate.as_ref();
    assert!(
        der.windows(ACME_IDENTIFIER.len())
            .any(|window| window == ACME_IDENTIFIER)
    );
    assert!(der.windows(32).any(|window| window == [7; 32]));
    assert!(certificate(&host(), &[7; 31]).is_err());
}

/// Accepts whatever certificate the server shows: the test reads it back
/// rather than trusting it.
#[derive(Debug)]
struct Anything;

impl ServerCertVerifier for Anything {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A private relay's hub with an issuer, and its front.
async fn front_with_issuer() -> (std::net::SocketAddr, Arc<Hub>, Arc<Issuer>) {
    let settings = Settings {
        directory: "https://127.0.0.1:1/directory".into(),
        dir: std::env::temp_dir(),
        root: None,
        weekly: 4,
    };
    let issuer = Arc::new(Issuer::new(settings, Arc::from(RELAY)));
    let access = Access::tokens("a listen token long enough").unwrap();
    let hub = Arc::new(
        Hub::new(Limits::default(), access)
            .with_web(true)
            .with_issuer(Arc::clone(&issuer)),
    );
    let listener = Capped::new(
        TcpListener::bind("127.0.0.1:0").await.unwrap(),
        8,
        Some(8),
        Duration::from_secs(60),
    );
    let address = listener.local_addr().unwrap();
    let config = Config {
        relay_host: Arc::from(RELAY),
        upstream: None,
    };
    tokio::spawn(front::run(listener, Arc::clone(&hub), config));
    (address, hub, issuer)
}

#[tokio::test]
async fn the_front_answers_a_pending_challenge_with_its_certificate() {
    let (address, hub, issuer) = front_with_issuer().await;
    let label = WebLabel::of(&RouteId::from_bytes([3; 32]));
    let answering = issuer.challenges.answer(label, &host(), &[9; 32]).unwrap();
    let expected = issuer.challenges.get(&label).unwrap().certificate.clone();

    let mut config = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Anything))
        .with_no_client_auth();
    config.alpn_protocols = vec![ACME_TLS_ALPN.to_vec()];
    let stream = TcpStream::connect(address).await.unwrap();
    let name = ServerName::try_from(host()).unwrap();
    let tls = timeout(
        Duration::from_secs(5),
        TlsConnector::from(Arc::new(config)).connect(name, stream),
    )
    .await
    .unwrap()
    .unwrap();
    let (_, session) = tls.get_ref();
    assert_eq!(session.alpn_protocol(), Some(ACME_TLS_ALPN));
    assert_eq!(session.peer_certificates().unwrap()[0], expected);
    assert!(
        hub.metrics
            .render()
            .contains("sheprd_relay_front_total{outcome=\"challenge\"} 1")
    );

    // Once the order is decided, the name is closed again.
    drop(answering);
    assert!(issuer.challenges.get(&label).is_none());
    let mut stream = TcpStream::connect(address).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut stream, &real_hello(&host(), &[ACME_TLS_ALPN]))
        .await
        .unwrap();
    let mut byte = [0; 1];
    let read = timeout(Duration::from_secs(5), stream.read(&mut byte))
        .await
        .unwrap();
    assert!(matches!(read, Ok(0) | Err(_)));
}

#[test]
fn orders_running_at_once_cannot_pass_the_weekly_allowance_together() {
    let mut ration = Ration::new(1);
    let now = Instant::now();
    let (first, second) = (RouteId::from_bytes([1; 32]), RouteId::from_bytes([2; 32]));
    assert!(ration.attempt(first, now).is_ok());
    // The first order still runs: its certificate is reserved.
    assert!(matches!(ration.attempt(second, now), Err(Error::Weekly)));
    // It failed, so the reservation is free again.
    ration.failed();
    assert!(ration.attempt(second, now).is_ok());
    ration.issued(second, now);
    assert!(matches!(
        ration.attempt(RouteId::from_bytes([3; 32]), now),
        Err(Error::Weekly)
    ));
}
