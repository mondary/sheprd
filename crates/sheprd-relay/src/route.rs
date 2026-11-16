//! Routes, sessions, and the proof a listener gives.
//!
//! A route is named by the SHA-256 of a P-256 public key, so only the holder
//! of the private key can listen on it. The key is the computer's relay
//! identity, separate from its end-to-end keys: the relay learns nothing
//! that would let it open the stream.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use sha2::{Digest, Sha256};
use std::fmt;

/// A listener's address on the relay: SHA-256 of its public key.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct RouteId([u8; 32]);

impl RouteId {
    /// The route `key` (a SEC1 P-256 public key) listens on.
    #[must_use]
    pub fn of(key: &VerifyingKey) -> Self {
        Self(Sha256::digest(key.to_sec1_point(true).as_bytes()).into())
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Reads a route from its URL form, 43 base64url characters.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != 43 {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
        bytes.try_into().ok().map(Self)
    }

    /// The first eight characters, for the dashboard: enough to tell routes
    /// apart, too few to dial one.
    #[must_use]
    pub fn short(&self) -> String {
        let mut text = self.to_string();
        text.truncate(8);
        text
    }
}

impl fmt::Display for RouteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&URL_SAFE_NO_PAD.encode(self.0))
    }
}

/// Route ids stay out of logs; `Debug` shows only the short form.
impl fmt::Debug for RouteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "RouteId({}…)", self.short())
    }
}

/// How many of a route's bytes its web label carries: 160 bits, as many
/// as base32 fits in 32 characters.
const WEB_LABEL_BYTES: usize = 20;
/// A web label's length in characters.
pub const WEB_LABEL_LEN: usize = 32;
/// Base32's lowercase alphabet (RFC 4648), which DNS names can carry.
const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// The first label of a route's web address, `<label>.<relay host>`, where
/// browsers reach a computer that turned web access on
/// (`docs/relay-web-plan.md`): the start of its route in lowercase base32,
/// one DNS label. It names the route, not a secret: the computer's
/// certificate puts it in Certificate Transparency logs.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct WebLabel([u8; WEB_LABEL_BYTES]);

impl WebLabel {
    /// The label browsers reach `route` by.
    #[must_use]
    pub fn of(route: &RouteId) -> Self {
        let mut bytes = [0; WEB_LABEL_BYTES];
        bytes.copy_from_slice(&route.0[..WEB_LABEL_BYTES]);
        Self(bytes)
    }

    /// Reads a label: exactly [`WEB_LABEL_LEN`] lowercase base32
    /// characters, as a server name carries it once lowercased.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != WEB_LABEL_LEN {
            return None;
        }
        let mut bytes = [0; WEB_LABEL_BYTES];
        // Each group of eight characters holds five bytes.
        for (group, out) in text.as_bytes().chunks(8).zip(bytes.chunks_mut(5)) {
            let mut bits: u64 = 0;
            for &character in group {
                let value = BASE32.iter().position(|&known| known == character)?;
                bits = (bits << 5) | u64::try_from(value).ok()?;
            }
            out.copy_from_slice(&bits.to_be_bytes()[3..]);
        }
        Some(Self(bytes))
    }

    /// The route's web address on the relay named `relay_host`.
    #[must_use]
    pub fn host(&self, relay_host: &str) -> String {
        format!("{self}.{relay_host}")
    }
}

impl fmt::Display for WebLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = [0; WEB_LABEL_LEN];
        for (group, out) in self.0.chunks(5).zip(text.chunks_mut(8)) {
            let mut padded = [0; 8];
            padded[3..].copy_from_slice(group);
            let bits = u64::from_be_bytes(padded);
            for (index, character) in out.iter_mut().enumerate() {
                let shift = 35 - 5 * index;
                *character = BASE32[usize::try_from((bits >> shift) & 31).unwrap_or(0)];
            }
        }
        // Every byte comes from the ASCII alphabet.
        formatter.write_str(std::str::from_utf8(&text).map_err(|_| fmt::Error)?)
    }
}

/// Labels stay out of logs, as routes do.
impl fmt::Debug for WebLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.to_string();
        write!(formatter, "WebLabel({}…)", &text[..8])
    }
}

/// One dial waiting for its listener, named by 128 random bits that only the
/// listener learns.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct SessionId([u8; 16]);

impl SessionId {
    /// A fresh id.
    pub fn random() -> crate::Result<Self> {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(crate::Error::Random)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != 22 {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
        bytes.try_into().ok().map(Self)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&URL_SAFE_NO_PAD.encode(self.0))
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionId(…)")
    }
}

/// Why a proof was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    Key,
    Signature,
}

/// The route `key` (SEC1 P-256) listens on, if `signature` (`r ‖ s`) holds
/// over `message`. Both transports' proofs end here.
pub fn prove(key: &[u8], signature: &[u8], message: &[u8]) -> Result<RouteId, Refusal> {
    let key = VerifyingKey::from_sec1_bytes(key).map_err(|_| Refusal::Key)?;
    let signature = Signature::from_slice(signature).map_err(|_| Refusal::Signature)?;
    key.verify(message, &signature)
        .map_err(|_| Refusal::Signature)?;
    Ok(RouteId::of(&key))
}

/// What a pairing dial key's derivation hashes ahead of the secret.
const PAIRING_DIAL_CONTEXT: &[u8] = b"sheprd-relay/pairing-dial/v1\0";

/// The one-dial key a phone pairs through a relay with, from the pairing
/// secret both sides hold (the claim token the computer sealed to the
/// phone): SHA-256 of a context, the secret, and a counter, the first that
/// is a P-256 scalar. The computer registers its public half as a pairing
/// key, which the relay lets through once.
#[must_use]
pub fn pairing_dial_key(secret: &[u8; 32]) -> p256::ecdsa::SigningKey {
    let mut counter: u8 = 0;
    loop {
        let digest = Sha256::new()
            .chain_update(PAIRING_DIAL_CONTEXT)
            .chain_update(secret)
            .chain_update([counter])
            .finalize();
        if let Ok(key) = p256::ecdsa::SigningKey::from_slice(&digest) {
            return key;
        }
        // Out of range about once in 2^32 tries; the next counter is not.
        counter = counter.wrapping_add(1);
    }
}
