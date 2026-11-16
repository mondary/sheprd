//! A software authenticator for tests: it makes ES256 passkeys and signs
//! assertions as a browser and platform authenticator would, with every
//! field a test may spoil.

use super::{Assertion, Registration, encode, sha256};
use p256::ecdsa::{DerSignature, SigningKey, signature::Signer};

/// The flags a person's verified use sets: present and verified.
pub(crate) const VERIFIED: u8 = 0x01 | 0x04;

pub(crate) struct Authenticator {
    pub(crate) key: SigningKey,
    pub(crate) credential: Vec<u8>,
    /// What the next use reports as its counter.
    pub(crate) count: u32,
}

/// What the browser puts in client data, each a test may change.
pub(crate) struct Client<'a> {
    pub(crate) kind: &'a str,
    pub(crate) origin: String,
    pub(crate) cross_origin: bool,
}

impl Client<'_> {
    pub(crate) fn json(&self, challenge: &[u8]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "type": self.kind,
            "challenge": encode(challenge),
            "origin": self.origin,
            "crossOrigin": self.cross_origin,
        }))
        .unwrap()
    }
}

impl Authenticator {
    pub(crate) fn new(seed: u8) -> Self {
        Self {
            key: SigningKey::from_slice(&[seed; 32]).unwrap(),
            credential: vec![seed; 16],
            count: 0,
        }
    }

    /// rpIdHash, flags, and counter.
    pub(crate) fn head(&self, host: &str, flags: u8) -> Vec<u8> {
        [
            &sha256(&[host.as_bytes()])[..],
            &[flags],
            &self.count.to_be_bytes(),
        ]
        .concat()
    }

    /// The COSE key: kty EC2, alg `alg`, crv P-256, x, y.
    pub(crate) fn cose(&self, alg: i8) -> Vec<u8> {
        let point = self.key.verifying_key().to_sec1_point(false);
        let point = point.as_bytes();
        let mut out = vec![0xa5, 0x01, 0x02, 0x03];
        out.push(integer(alg));
        out.extend([0x20, 0x01, 0x21, 0x58, 0x20]);
        out.extend(&point[1..33]);
        out.extend([0x22, 0x58, 0x20]);
        out.extend(&point[33..65]);
        out
    }

    /// Authenticator data with the attested credential and `cose` after it.
    pub(crate) fn attested(&self, host: &str, flags: u8, cose: &[u8]) -> Vec<u8> {
        let length = u16::try_from(self.credential.len()).unwrap().to_be_bytes();
        [
            &self.head(host, flags | 0x40)[..],
            &[0; 16],
            &length,
            &self.credential,
            cose,
        ]
        .concat()
    }

    /// A registration made at `host` for `challenge`, attestation `none`.
    pub(crate) fn register(&self, host: &str, challenge: &[u8]) -> Registration {
        let data = self.attested(host, VERIFIED, &self.cose(-7));
        let client = Client {
            kind: "webauthn.create",
            origin: format!("https://{host}"),
            cross_origin: false,
        };
        registration(&client.json(challenge), &attestation_object(&data))
    }

    /// An assertion at `host` over `challenge`, signing `data` as given.
    pub(crate) fn sign(&self, data: &[u8], client_data: &[u8]) -> Assertion {
        let signed = [data, &sha256(&[client_data])].concat();
        let signature: DerSignature = self.key.sign(&signed);
        Assertion {
            client_data: encode(client_data),
            authenticator_data: encode(data),
            signature: encode(signature.as_bytes()),
        }
    }

    /// A verified use at `host` answering `challenge`.
    pub(crate) fn assert(&self, host: &str, challenge: &[u8]) -> Assertion {
        let client = Client {
            kind: "webauthn.get",
            origin: format!("https://{host}"),
            cross_origin: false,
        };
        self.sign(&self.head(host, VERIFIED), &client.json(challenge))
    }
}

pub(crate) fn registration(client_data: &[u8], attestation: &[u8]) -> Registration {
    Registration {
        client_data: encode(client_data),
        attestation: encode(attestation),
    }
}

/// `{"fmt": "none", "attStmt": {}, "authData": data}`.
pub(crate) fn attestation_object(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0xa3, 0x63];
    out.extend(b"fmt");
    out.push(0x64);
    out.extend(b"none");
    out.push(0x67);
    out.extend(b"attStmt");
    out.push(0xa0);
    out.push(0x68);
    out.extend(b"authData");
    out.push(0x59);
    out.extend(u16::try_from(data.len()).unwrap().to_be_bytes());
    out.extend(data);
    out
}

/// A small CBOR integer, -24 to 23.
fn integer(value: i8) -> u8 {
    if value >= 0 {
        u8::try_from(value).unwrap()
    } else {
        0x20 | u8::try_from(-1 - i16::from(value)).unwrap()
    }
}
