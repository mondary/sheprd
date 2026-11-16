use super::*;

/// herdr-core's rule, from its refusal: 1-32 of `a-z 0-9 - _ .`, starting
/// with a letter or digit.
fn core_accepts(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
}

#[test]
fn the_recovery_document_name_is_one_herdr_core_accepts() {
    for id in [ID, "ffffffffffffffffffffffffffffffff"] {
        let name = document(&id.parse().unwrap());
        assert!(core_accepts(&name), "{name}");
    }
}

#[test]
fn chats_with_different_endpoints_keep_different_documents() {
    assert_ne!(
        document(&ID.parse().unwrap()),
        document(&"1123456789abcdef0123456789abcdef".parse().unwrap())
    );
}
