//! The CT lookup against a stub search on loopback: clean answers, a
//! foreign key, pages, and every way a query fails, none taken as clean.

use super::*;
use std::{
    io::{BufRead as _, BufReader, Write as _},
    net::TcpListener,
    sync::mpsc,
    thread,
};

const HOST: &str = "abc.fra1.sheprd-relay.dev";
const OWN: [u8; 32] = [0xaa; 32];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn issuance(id: &str, key: [u8; 32]) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "pubkey_sha256": hex(&key),
        "cert_sha256": format!("cert-{id}"),
        "dns_names": [HOST],
    })
}

/// A search answering each request with the next of `answers` (status,
/// body); the request lines it saw come back on the channel.
fn stub(answers: Vec<(u16, String)>) -> (Url, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, requests) = mpsc::channel();
    thread::spawn(move || {
        for (status, body) in answers {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let _ = seen.send(line.trim().to_owned());
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
            }
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let api = api(&format!("http://{address}/v1/issuances")).unwrap();
    (api, requests)
}

fn page(issuances: &[serde_json::Value]) -> (u16, String) {
    (200, serde_json::Value::from(issuances.to_vec()).to_string())
}

#[test]
fn only_this_computers_keys_are_clean() {
    let (api, requests) = stub(vec![page(&[issuance("1", OWN)]), page(&[])]);
    assert_eq!(check(&agent(), &api, HOST, &[OWN]).unwrap(), Finding::Clean);
    let first = requests.recv().unwrap();
    assert!(
        first.contains("domain=abc.fra1.sheprd-relay.dev"),
        "{first}"
    );
    assert!(first.contains("match_wildcards=true"), "{first}");
    // The next page starts after the last certificate read.
    assert!(requests.recv().unwrap().contains("after=1"));
}

#[test]
fn another_key_on_any_page_is_found() {
    let (api, _) = stub(vec![
        page(&[issuance("1", OWN)]),
        page(&[issuance("2", OWN), issuance("3", [0xbb; 32])]),
    ]);
    assert_eq!(
        check(&agent(), &api, HOST, &[OWN]).unwrap(),
        Finding::Foreign("cert-3".into())
    );
}

#[test]
fn a_failed_query_is_never_clean() {
    let cases = [
        (429, "[]".to_owned()),
        (500, "[]".to_owned()),
        (200, "{\"not\": \"a list\"}".to_owned()),
        (
            200,
            serde_json::json!([{"id": "1", "pubkey_sha256": "zz"}]).to_string(),
        ),
    ];
    for answer in cases {
        let (api, _) = stub(vec![answer.clone()]);
        assert!(check(&agent(), &api, HOST, &[OWN]).is_err(), "{answer:?}");
    }
    // Nothing listening at all.
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let api = api(&format!("http://{closed}/v1/issuances")).unwrap();
    assert!(matches!(
        check(&agent(), &api, HOST, &[OWN]),
        Err(Error::Unreachable(_))
    ));
}

#[test]
fn endless_pages_are_reported_not_trusted() {
    let pages = (0..MAX_PAGES)
        .map(|n| page(&[issuance(&n.to_string(), OWN)]))
        .collect();
    let (api, _) = stub(pages);
    assert!(matches!(
        check(&agent(), &api, HOST, &[OWN]),
        Err(Error::TooMany)
    ));
}

#[test]
fn the_search_is_reached_over_https_or_loopback_alone() {
    assert!(api(DEFAULT_API).is_ok());
    assert!(api("http://127.0.0.1:8080/v1/issuances").is_ok());
    assert!(matches!(api("http://ct.example/v1"), Err(Error::Insecure)));
    assert!(api("not a url").is_err());
}
