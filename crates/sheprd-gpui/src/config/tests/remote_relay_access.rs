//! A private relay in `[remote] relays`: its listen token's file and the
//! web access switch, read, saved, and refused as the daemon needs them.

use super::*;
use crate::config::remote::Relay;
use std::path::PathBuf;

#[test]
fn a_private_relay_names_its_token_file_and_web_access() -> anyhow::Result<()> {
    let config = Config::parse(
        "[remote]\nrelays = [{ endpoint = 'https://fra1.sheprd-relay.dev', \
         token_file = '/etc/sheprd/fra1.token', web = true }]\n",
    )?;
    assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    assert_eq!(
        config.remote.relays,
        [Relay {
            endpoint: "https://fra1.sheprd-relay.dev".into(),
            token_file: Some(PathBuf::from("/etc/sheprd/fra1.token")),
            web: true,
            ..Relay::default()
        }]
    );
    Ok(())
}

#[test]
fn saving_keeps_the_token_file_and_web_access() -> anyhow::Result<()> {
    use crate::config::{preferences::Preference, remote::Edit};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    let private = Relay {
        endpoint: "https://fra1.sheprd-relay.dev".into(),
        token_file: Some(PathBuf::from("/etc/sheprd/fra1.token")),
        web: true,
        ..Relay::default()
    };
    Config::save_preference_path(
        Preference::Remote(Edit::Relays(vec![
            private.clone(),
            // Named again bare, it keeps what the first gave.
            "https://fra1.sheprd-relay.dev".into(),
        ])),
        &path,
    )?;
    let saved = Config::parse(&fs::read_to_string(&path)?)?.remote.relays;
    assert_eq!(saved, [private]);
    Ok(())
}

#[test]
fn a_relative_token_file_is_refused() {
    let Err(error) = Config::parse(
        "[remote]\nrelays = [{ endpoint = 'https://fra1.sheprd-relay.dev', \
         token_file = 'fra1.token' }]",
    ) else {
        panic!("a relative token file loaded");
    };
    assert!(
        matches!(
            &error,
            crate::Error::RemoteControl(crate::remote::Error::RelayTokenFile { relay })
                if relay == "https://fra1.sheprd-relay.dev"
        ),
        "{error:?}"
    );
}
