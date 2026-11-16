use super::*;

#[test]
fn a_relay_named_twice_keeps_its_token_and_web_access() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fra1.token");
    fs::write(&path, "secret\n").unwrap();
    let endpoint = sheprd_relay::Endpoint::parse("https://fra1.sheprd-relay.dev").unwrap();
    let private = Relay {
        token_file: Some(path),
        web: true,
        ..Relay::from(endpoint.clone())
    };
    let access = load(&[private, Relay::from(endpoint.clone())]).unwrap();
    let kept = &access[&endpoint.to_string()];
    assert_eq!(kept.token.as_bytes(), b"secret");
    assert!(kept.web);
}
