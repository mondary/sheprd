//! Passkeys for devices on this computer's web origin through a relay
//! (`docs/relay-web-plan.md`, Passkeys): WebAuthn's server side, written
//! on `p256` for the one algorithm asked for, ES256.
//!
//! The relying party ID is the web name itself, `<label>.<relay host>`,
//! never a parent domain, so a passkey for one computer never works for
//! another. Registration asks for no attestation and trusts none: what it
//! proves is that a person verified on an authenticator (UV) while
//! claiming a pairing the desktop then approves. Each assertion must be
//! verified by a person too, and answers a challenge only this daemon
//! issued, once.

mod cbor;
mod error;

pub use error::Error;

use super::{PublicKey, encode, sha256};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cbor::Reader;
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde::{Deserialize, Serialize};

/// What a registration's challenge hashes before the pairing token and the
/// device key, so a registration answers that claim and nothing else.
pub(crate) const REGISTER_PREFIX: &[u8] = b"sheprd-passkey-register\0";
/// The longest client data JSON accepted.
const MAX_CLIENT_DATA: usize = 4 * 1024;
/// The longest attestation object accepted.
const MAX_ATTESTATION: usize = 16 * 1024;
/// The longest authenticator data in an assertion: 37 bytes, then
/// extensions, which are not asked for.
const MAX_AUTHENTICATOR_DATA: usize = 1024;
/// The longest DER ECDSA P-256 signature.
const MAX_SIGNATURE: usize = 72;
/// The longest credential ID WebAuthn allows.
const MAX_CREDENTIAL_ID: usize = 1023;

/// Authenticator data flags.
const USER_PRESENT: u8 = 0x01;
const USER_VERIFIED: u8 = 0x04;
const ATTESTED: u8 = 0x40;
const EXTENSIONS: u8 = 0x80;
/// rpIdHash, flags, and the signature counter.
const AUTHENTICATOR_DATA_HEAD: usize = 32 + 1 + 4;

/// One device's passkey, as the device store keeps it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Passkey {
    /// The credential ID, base64url, which sign-in names for the browser.
    pub(crate) id: String,
    pub(crate) public_key: PublicKey,
    /// The web name it was made for: its relying party ID.
    pub(crate) host: String,
    /// The authenticator's counter at its last use; zero for passkeys that
    /// sync, which never count.
    #[serde(default)]
    pub(crate) sign_count: u32,
}

/// A registration as the browser returns it, base64url.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Registration {
    pub(crate) client_data: String,
    pub(crate) attestation: String,
}

/// An assertion as the browser returns it, base64url.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Assertion {
    pub(crate) client_data: String,
    pub(crate) authenticator_data: String,
    pub(crate) signature: String,
}

/// The registration challenge for a claim of `token` with `device_key`:
/// the phone computes the same, so no round trip is needed, and it is as
/// fresh and single use as the token.
pub(crate) fn register_challenge(token: &[u8], device_key: &[u8]) -> [u8; 32] {
    sha256(&[REGISTER_PREFIX, token, device_key])
}

/// Checks a new passkey made at `host` for `challenge`.
pub(crate) fn register(
    host: &str,
    challenge: &[u8],
    registration: &Registration,
) -> Result<Passkey, Error> {
    let client_data = decode_bounded(&registration.client_data, MAX_CLIENT_DATA)?;
    check_client_data(&client_data, "webauthn.create", challenge, host)?;
    let attestation = decode_bounded(&registration.attestation, MAX_ATTESTATION)?;
    let data = authenticator_data(&attestation)?;
    let flags = check_authenticator_data(data, host)?;
    if flags & ATTESTED == 0 {
        return Err(Error::NoCredential);
    }
    let sign_count = counter(data)?;
    // aaguid, then the credential ID's length and the ID.
    let attested = data
        .get(AUTHENTICATOR_DATA_HEAD + 16..)
        .ok_or(Error::Malformed)?;
    let (length, rest) = attested.split_at_checked(2).ok_or(Error::Malformed)?;
    let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
    if length == 0 || length > MAX_CREDENTIAL_ID {
        return Err(Error::Malformed);
    }
    let (id, key) = rest.split_at_checked(length).ok_or(Error::Malformed)?;
    let mut reader = Reader::new(key);
    let public_key = cose_key(&mut reader)?;
    // Extensions may follow the key only when the flag says so.
    if !reader.rest().is_empty() && flags & EXTENSIONS == 0 {
        return Err(Error::Malformed);
    }
    Ok(Passkey {
        id: encode(id),
        public_key,
        host: host.to_owned(),
        sign_count,
    })
}

/// Checks an assertion by `passkey` at `host` for `challenge`; answers the
/// counter to keep.
pub(crate) fn verify(
    passkey: &Passkey,
    host: &str,
    challenge: &[u8],
    assertion: &Assertion,
) -> Result<u32, Error> {
    if passkey.host != host {
        return Err(Error::WrongRelyingParty);
    }
    let client_data = decode_bounded(&assertion.client_data, MAX_CLIENT_DATA)?;
    check_client_data(&client_data, "webauthn.get", challenge, host)?;
    let data = decode_bounded(&assertion.authenticator_data, MAX_AUTHENTICATOR_DATA)?;
    check_authenticator_data(&data, host)?;
    let signature = decode_bounded(&assertion.signature, MAX_SIGNATURE)?;
    let signature = Signature::from_der(&signature).map_err(|_| Error::Signature)?;
    let key =
        VerifyingKey::from_sec1_bytes(passkey.public_key.as_bytes()).map_err(|_| Error::Key)?;
    let signed = [data.as_slice(), &sha256(&[&client_data])].concat();
    key.verify(&signed, &signature)
        .map_err(|_| Error::Signature)?;
    let count = counter(&data)?;
    // A counter that stays at zero is a passkey that syncs; any other must
    // move forward.
    if (count != 0 || passkey.sign_count != 0) && count <= passkey.sign_count {
        return Err(Error::WrongCounter);
    }
    Ok(count)
}

/// Decodes base64url (no padding) of at most `max` bytes, refusing longer
/// input before decoding so a request cannot make it allocate.
fn decode_bounded(encoded: &str, max: usize) -> Result<Vec<u8>, Error> {
    if encoded.len() > base64::encoded_len(max, false).ok_or(Error::TooLarge)? {
        return Err(Error::TooLarge);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| Error::Encoding)?;
    if bytes.len() > max {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}

/// The fields of the client data JSON that are checked.
#[derive(Deserialize)]
struct ClientData<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    challenge: &'a str,
    origin: &'a str,
    #[serde(default, rename = "crossOrigin")]
    cross_origin: bool,
}

fn check_client_data(json: &[u8], kind: &str, challenge: &[u8], host: &str) -> Result<(), Error> {
    let data: ClientData<'_> = serde_json::from_slice(json).map_err(Error::ClientData)?;
    if data.kind != kind {
        return Err(Error::WrongType);
    }
    if !super::constant_time_eq(data.challenge.as_bytes(), encode(challenge).as_bytes()) {
        return Err(Error::WrongChallenge);
    }
    if data.origin != format!("https://{host}") {
        return Err(Error::WrongOrigin);
    }
    if data.cross_origin {
        return Err(Error::CrossOrigin);
    }
    Ok(())
}

/// Checks the relying party and that a person was present and verified;
/// answers the flags.
fn check_authenticator_data(data: &[u8], host: &str) -> Result<u8, Error> {
    if data.len() < AUTHENTICATOR_DATA_HEAD {
        return Err(Error::Malformed);
    }
    if !super::constant_time_eq(&data[..32], &sha256(&[host.as_bytes()])) {
        return Err(Error::WrongRelyingParty);
    }
    let flags = data[32];
    if flags & USER_PRESENT == 0 {
        return Err(Error::NotPresent);
    }
    if flags & USER_VERIFIED == 0 {
        return Err(Error::NotVerified);
    }
    Ok(flags)
}

fn counter(data: &[u8]) -> Result<u32, Error> {
    let bytes = data.get(33..37).ok_or(Error::Malformed)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// The authenticator data of an attestation object. Its statement is not
/// read: no attestation is asked for or trusted, whatever its format.
fn authenticator_data(attestation: &[u8]) -> Result<&[u8], Error> {
    let mut reader = Reader::new(attestation);
    let mut data = None;
    for _ in 0..reader.map()? {
        match reader.text()? {
            "authData" if data.is_none() => data = Some(reader.bytes()?),
            "authData" => return Err(Error::Malformed),
            _ => reader.skip()?,
        }
    }
    if !reader.rest().is_empty() {
        return Err(Error::Malformed);
    }
    data.ok_or(Error::Malformed)
}

/// An ES256 COSE key (kty EC2, alg -7, crv P-256) as an uncompressed point.
fn cose_key(reader: &mut Reader<'_>) -> Result<PublicKey, Error> {
    let (mut kty, mut alg, mut crv, mut x, mut y) = (None, None, None, None, None);
    for _ in 0..reader.map()? {
        match reader.integer()? {
            1 => kty = Some(reader.integer()?),
            3 => alg = Some(reader.integer()?),
            -1 => crv = Some(reader.integer()?),
            -2 => x = Some(reader.bytes()?),
            -3 => y = Some(reader.bytes()?),
            _ => reader.skip()?,
        }
    }
    let (Some(2), Some(-7), Some(1), Some(x), Some(y)) = (kty, alg, crv, x, y) else {
        return Err(Error::Key);
    };
    if x.len() != 32 || y.len() != 32 {
        return Err(Error::Key);
    }
    let point = [&[0x04][..], x, y].concat();
    PublicKey::parse(&encode(&point)).map_err(|_| Error::Key)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod soft;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
