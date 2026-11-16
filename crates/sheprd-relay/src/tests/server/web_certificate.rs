//! Asking for a web certificate over QUIC, as the daemon will: only a
//! listener with web access on, on a private relay that orders
//! certificates, is heard, and only for a CSR naming its own web name.

use super::*;
use crate::{
    WebLabel,
    client::{self, Error as ClientError, Listener},
    wire::Status,
};
use rcgen::{CertificateParams, KeyPair};
use secrecy::SecretString;

const RELAY: &str = "fra1.sheprd-relay.dev";
const LISTEN_TOKEN: &str = "a listen token long enough";

/// A private relay offering web access and certificates, whose ACME
/// server is unreachable: no test may order a real certificate.
fn private_options(acme_dir: &std::path::Path) -> Options {
    Options {
        acme_dir: Some(acme_dir.to_owned()),
        acme_directory: "https://127.0.0.1:1/directory".into(),
        front: Some("127.0.0.1:0".parse().unwrap()),
        listen_token: Some(SecretString::from(LISTEN_TOKEN)),
        public_host: Some(RELAY.into()),
        ..options(Limits::default())
    }
}

fn csr(host: &str) -> Vec<u8> {
    let key = KeyPair::generate().unwrap();
    let params = CertificateParams::new(vec![host.to_owned()]).unwrap();
    params.serialize_request(&key).unwrap().der().to_vec()
}

fn refused(result: client::Result<impl std::fmt::Debug>) -> Status {
    match result {
        Err(ClientError::Refused(status)) => status,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn only_a_listener_with_web_access_on_is_heard() {
    let dir = std::env::temp_dir().join(format!("sheprd-relay-acme-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let relay = start_both(&private_options(&dir)).await;
    let key = signing_key(180);
    let connection = client::connect(relay.quic, relay.pin).await.unwrap();
    let mut listener = Listener::listen(connection, &key, LISTEN_TOKEN.as_bytes())
        .await
        .unwrap();
    let host = WebLabel::of(&listener.route()).host(RELAY);

    // Web access off: no certificate, whatever the CSR.
    assert_eq!(
        refused(listener.certificate(&key, &csr(&host)).await),
        Status::Forbidden
    );

    listener.set_web(true).await.unwrap();
    // Another name is refused before anything is ordered.
    assert_eq!(
        refused(listener.certificate(&key, &csr("example.org")).await),
        Status::Refused
    );
    // The right name is ordered; the CA here cannot be reached.
    assert_eq!(
        refused(listener.certificate(&key, &csr(&host)).await),
        Status::Failed
    );
    // A failed attempt waits before the next.
    assert_eq!(
        refused(listener.certificate(&key, &csr(&host)).await),
        Status::Limited
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_public_relay_offers_no_web_access() {
    let options = Options {
        front: Some("127.0.0.1:0".parse().unwrap()),
        public_host: Some(RELAY.into()),
        ..options(Limits::default())
    };
    let relay = start_both(&options).await;
    let key = signing_key(181);
    let connection = client::connect(relay.quic, relay.pin).await.unwrap();
    let mut listener = Listener::listen(connection, &key, b"").await.unwrap();
    assert_eq!(refused(listener.set_web(true).await), Status::Forbidden);
    let host = WebLabel::of(&listener.route()).host(RELAY);
    assert_eq!(
        refused(listener.certificate(&key, &csr(&host)).await),
        Status::Forbidden
    );
}
