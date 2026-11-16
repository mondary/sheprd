//! The Certificate Transparency watch end to end: a certificate for this
//! computer's web name with another key turns web access off on that
//! relay, tells paired devices, survives a restart, and stays off until
//! the user turns it on again.

use super::*;
use crate::control::web::Transparency;
use std::{
    io::{BufRead as _, BufReader, Write as _},
    sync::atomic::{AtomicBool, Ordering},
};

/// A CT search on loopback: lists one certificate, with another key while
/// `foreign` holds, else with `own`.
fn search(own: [u8; 32], foreign: Arc<AtomicBool>) -> url::Url {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
            }
            let key = if foreign.load(Ordering::SeqCst) {
                [0xbb; 32]
            } else {
                own
            };
            let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
            // The first page lists it; the next, after it, is empty.
            let body = if line.contains("after=") {
                "[]".to_owned()
            } else {
                format!(r#"[{{"id":"1","pubkey_sha256":"{hex}","cert_sha256":"c0ffee"}}]"#)
            };
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    crate::relay::web::transparency::api(&format!("http://{address}/v1/issuances")).unwrap()
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..200 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never {what}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_certificate_with_another_key_turns_web_access_off_until_resumed() {
    use rcgen::PublicKeyData as _;
    use sha2::Digest as _;
    let (http, _) = private_relay().await;
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
    let configured = vec![relay];
    let mut relays = Relays::load(dir.path(), configured.clone()).unwrap();
    let ca = Ca::new();
    relays.trust_web_roots(ca.roots.clone());
    let host = endpoint
        .web_host(&client::route(&relays.keys.route))
        .unwrap();
    let key = KeyPair::generate().unwrap();
    let store = crate::relay::web::store_dir(dir.path(), &endpoint.to_string());
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(store.join("key.der"), key.serialize_der()).unwrap();
    std::fs::write(store.join("chain.pem"), ca.issue(&host, &key, 1, 90)).unwrap();
    let own: [u8; 32] = sha2::Sha256::digest(key.subject_public_key_info()).into();
    let foreign = Arc::new(AtomicBool::new(true));
    relays.set_transparency(search(own, Arc::clone(&foreign)));
    let relays = Arc::new(relays);
    Arc::get_mut(&mut shared).unwrap().relay = Some(Arc::clone(&relays));
    let mut notices = shared.notices.subscribe();

    let (_stop, stopped) = watch::channel(false);
    relays.start(served(dir.path(), &shared).0, stopped);
    let key_name = endpoint.to_string();
    let transparency = |relays: &Relays| {
        relays.status()[&key_name]
            .web
            .clone()
            .map(|web| web.transparency)
    };
    until("alerted", || {
        matches!(transparency(&relays), Some(Transparency::Alert { .. }))
    })
    .await;
    let Some(Transparency::Alert { certificate, .. }) = transparency(&relays) else {
        unreachable!();
    };
    assert_eq!(certificate, "c0ffee");
    // Browsers are no longer served there, and pages there are not admitted.
    let origin = format!("https://{host}");
    assert!(relays.web_serving(&endpoint).is_none());
    assert!(!shared.serves(&origin));
    assert!(!relays.wants_web(&endpoint));
    // Paired devices are told.
    let notice = tokio::time::timeout(Duration::from_secs(5), notices.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(notice.kind, crate::push::Kind::Security);
    // It stays off across a restart.
    let again = Relays::load(dir.path(), configured).unwrap();
    assert!(!again.wants_web(&endpoint));
    assert!(matches!(
        again
            .status_for_tests(&key_name)
            .map(|web| web.transparency),
        Some(Transparency::Alert { .. })
    ));

    // The user turns it on again, and the name is clean now.
    foreign.store(false, Ordering::SeqCst);
    let resumer = Arc::clone(&relays);
    let endpoint_name = key_name.clone();
    assert!(
        tokio::task::spawn_blocking(move || resumer.resume_web(&endpoint_name))
            .await
            .unwrap()
            .unwrap()
    );
    until("served again", || shared.serves(&origin)).await;
    until("clean", || {
        matches!(transparency(&relays), Some(Transparency::Clean { .. }))
    })
    .await;
}
