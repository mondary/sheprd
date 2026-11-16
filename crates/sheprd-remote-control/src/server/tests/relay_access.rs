//! A private relay's daemon options: its listen token's file and the web
//! access switch belong to the `--relay` before them.

use super::*;

#[test]
fn a_token_file_and_web_access_name_the_relay_before_them() {
    let options = serve(&[
        "serve",
        "--state-dir",
        "/s",
        "--relay",
        "https://sfo3.example.org",
        "--relay",
        "https://fra1.sheprd-relay.dev",
        "--relay-token-file",
        "/etc/sheprd/fra1.token",
        "--relay-web",
    ]);
    let access: Vec<_> = options
        .relays
        .iter()
        .map(|relay| (relay.token_file.clone(), relay.web))
        .collect();
    assert_eq!(
        access,
        [
            (None, false),
            (Some(PathBuf::from("/etc/sheprd/fra1.token")), true)
        ]
    );
}

#[test]
fn they_need_a_relay() {
    for option in ["--relay-token-file", "--relay-web"] {
        let mut list = vec!["serve", "--state-dir", "/s", option];
        if option == "--relay-token-file" {
            list.push("/t");
        }
        assert_eq!(parse(args(&list)), Err(ParseError::NoRelay { option }));
    }
}
