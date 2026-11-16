use super::{soft::*, *};

const HOST: &str = "abcdefghijklmnopqrstuvwxyz234567.fra1.sheprd-relay.dev";
const CHALLENGE: [u8; 32] = [7; 32];

/// Whether an error is the one a case expects.
type Expect = fn(&Error) -> bool;

fn registered(authenticator: &Authenticator) -> Passkey {
    register(HOST, &CHALLENGE, &authenticator.register(HOST, &CHALLENGE)).unwrap()
}

fn client(kind: &str) -> Client<'_> {
    Client {
        kind,
        origin: format!("https://{HOST}"),
        cross_origin: false,
    }
}

#[test]
fn a_registered_passkey_signs_in() {
    let authenticator = Authenticator::new(1);
    let passkey = registered(&authenticator);
    assert_eq!(passkey.host, HOST);
    assert_eq!(passkey.id, encode(&authenticator.credential));
    assert_eq!(passkey.sign_count, 0);
    let nonce = [9; 32];
    let count = verify(&passkey, HOST, &nonce, &authenticator.assert(HOST, &nonce)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn a_registration_for_another_page_or_request_is_refused() {
    let authenticator = Authenticator::new(1);
    let data = attestation_object(&authenticator.attested(HOST, VERIFIED, &authenticator.cose(-7)));
    let cases: Vec<(Client<'_>, &[u8], Expect)> = vec![
        (client("webauthn.get"), &CHALLENGE, |e| {
            matches!(e, Error::WrongType)
        }),
        (client("webauthn.create"), &[8; 32], |e| {
            matches!(e, Error::WrongChallenge)
        }),
        (
            Client {
                origin: "https://evil.example".into(),
                ..client("webauthn.create")
            },
            &CHALLENGE,
            |e| matches!(e, Error::WrongOrigin),
        ),
        (
            Client {
                cross_origin: true,
                ..client("webauthn.create")
            },
            &CHALLENGE,
            |e| matches!(e, Error::CrossOrigin),
        ),
    ];
    for (client, challenge, expected) in cases {
        let error = register(
            HOST,
            &CHALLENGE,
            &registration(&client.json(challenge), &data),
        )
        .unwrap_err();
        assert!(expected(&error), "{error:?}");
    }
}

#[test]
fn a_registration_needs_a_verified_person_and_this_relying_party() {
    let authenticator = Authenticator::new(1);
    let json = client("webauthn.create").json(&CHALLENGE);
    let cases: Vec<(&str, u8, Expect)> = vec![
        ("other.fra1.sheprd-relay.dev", VERIFIED, |e| {
            matches!(e, Error::WrongRelyingParty)
        }),
        (HOST, 0x04, |e| matches!(e, Error::NotPresent)),
        (HOST, 0x01, |e| matches!(e, Error::NotVerified)),
    ];
    for (rp, flags, expected) in cases {
        let data = authenticator.attested(rp, flags, &authenticator.cose(-7));
        let error = register(
            HOST,
            &CHALLENGE,
            &registration(&json, &attestation_object(&data)),
        )
        .unwrap_err();
        assert!(expected(&error), "{error:?}");
    }
}

#[test]
fn only_es256_keys_are_kept() {
    let authenticator = Authenticator::new(1);
    let json = client("webauthn.create").json(&CHALLENGE);
    // EdDSA's algorithm number with a P-256 point.
    let data = authenticator.attested(HOST, VERIFIED, &authenticator.cose(-8));
    let error = register(
        HOST,
        &CHALLENGE,
        &registration(&json, &attestation_object(&data)),
    )
    .unwrap_err();
    assert!(matches!(error, Error::Key), "{error:?}");
}

#[test]
fn malformed_attestations_are_refused_without_panicking() {
    let authenticator = Authenticator::new(1);
    let json = client("webauthn.create").json(&CHALLENGE);
    let good = authenticator.attested(HOST, VERIFIED, &authenticator.cose(-7));
    let mut objects: Vec<Vec<u8>> = (0..attestation_object(&good).len())
        .map(|cut| attestation_object(&good)[..cut].to_vec())
        .collect();
    // Bytes after the key without the extensions flag.
    objects.push(attestation_object(&[&good[..], &[0]].concat()));
    // An indefinite-length map, and arrays nested past the bound.
    objects.push(vec![0xbf, 0xff]);
    objects.push([vec![0xa1, 0x61, b'x'], vec![0x81; 64], vec![0x00]].concat());
    // A byte string claiming more than there is.
    objects.push(vec![
        0xa1, 0x68, b'a', b'u', b't', b'h', b'D', b'a', b't', b'a', 0x5b, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff,
    ]);
    // No credential.
    objects.push(attestation_object(&authenticator.head(HOST, VERIFIED)));
    for object in objects {
        assert!(register(HOST, &CHALLENGE, &registration(&json, &object)).is_err());
    }
    let huge = registration(&json, &vec![0; MAX_ATTESTATION + 1]);
    assert!(matches!(
        register(HOST, &CHALLENGE, &huge),
        Err(Error::TooLarge)
    ));
}

#[test]
fn an_assertion_must_answer_this_challenge_here_with_a_verified_person() {
    let authenticator = Authenticator::new(1);
    let passkey = registered(&authenticator);
    let nonce = [9; 32];
    let json = client("webauthn.get").json(&nonce);
    let cases: Vec<(Assertion, Expect)> = vec![
        (authenticator.assert(HOST, &[8; 32]), |e| {
            matches!(e, Error::WrongChallenge)
        }),
        (
            authenticator.sign(
                &authenticator.head(HOST, VERIFIED),
                &client("webauthn.create").json(&nonce),
            ),
            |e| matches!(e, Error::WrongType),
        ),
        (
            authenticator.sign(&authenticator.head(HOST, 0x01), &json),
            |e| matches!(e, Error::NotVerified),
        ),
        (
            authenticator.sign(&authenticator.head("other.example", VERIFIED), &json),
            |e| matches!(e, Error::WrongRelyingParty),
        ),
        // Another key's signature.
        (Authenticator::new(2).assert(HOST, &nonce), |e| {
            matches!(e, Error::Signature)
        }),
    ];
    for (assertion, expected) in cases {
        let error = verify(&passkey, HOST, &nonce, &assertion).unwrap_err();
        assert!(expected(&error), "{error:?}");
    }
    let elsewhere = verify(
        &passkey,
        "other.example",
        &nonce,
        &authenticator.assert(HOST, &nonce),
    );
    assert!(matches!(elsewhere, Err(Error::WrongRelyingParty)));
}

#[test]
fn a_counting_authenticator_must_count_forward() {
    let mut authenticator = Authenticator::new(1);
    authenticator.count = 5;
    let passkey = registered(&authenticator);
    assert_eq!(passkey.sign_count, 5);
    let nonce = [9; 32];
    let again = verify(&passkey, HOST, &nonce, &authenticator.assert(HOST, &nonce));
    assert!(matches!(again, Err(Error::WrongCounter)), "{again:?}");
    authenticator.count = 6;
    assert_eq!(
        verify(&passkey, HOST, &nonce, &authenticator.assert(HOST, &nonce)).unwrap(),
        6
    );
    // A clone that falls back to zero is refused too.
    authenticator.count = 0;
    let zero = verify(&passkey, HOST, &nonce, &authenticator.assert(HOST, &nonce));
    assert!(matches!(zero, Err(Error::WrongCounter)), "{zero:?}");
}

#[test]
fn a_signature_over_other_data_is_refused() {
    let authenticator = Authenticator::new(1);
    let passkey = registered(&authenticator);
    let nonce = [9; 32];
    let mut assertion = authenticator.assert(HOST, &nonce);
    // The same signature, with client data for another challenge.
    assertion.client_data = encode(&client("webauthn.get").json(&[8; 32]));
    assert!(verify(&passkey, HOST, &[8; 32], &assertion).is_err());
}

#[test]
fn signatures_with_either_s_verify() {
    use p256::ecdsa::{Signature, signature::Signer as _};
    let authenticator = Authenticator::new(1);
    let passkey = registered(&authenticator);
    let nonce = [9; 32];
    let data = authenticator.head(HOST, VERIFIED);
    let client_data = client("webauthn.get").json(&nonce);
    let signed = [&data[..], &sha256(&[&client_data])].concat();
    let signature: Signature = authenticator.key.sign(&signed);
    let (r, s) = signature.split_scalars();
    // Authenticators do not normalize S, so both forms are real.
    let flipped = Signature::from_scalars(r.to_bytes(), (-*s).to_bytes()).unwrap();
    for signature in [signature, flipped] {
        let assertion = Assertion {
            client_data: encode(&client_data),
            authenticator_data: encode(&data),
            signature: encode(signature.to_der().as_bytes()),
        };
        verify(&passkey, HOST, &nonce, &assertion).unwrap();
    }
}
