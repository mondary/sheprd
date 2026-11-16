//! Sessions: a paired device signs a fresh challenge with its key and gets
//! a bearer token. Only the token's SHA-256 is kept.

use super::{
    Devices, Error, Result, SECRET_LEN, Scope, decode_fixed, encode, passkey, random_bytes,
    random_id, sha256,
};
use crate::control::DeviceId;
use p256::ecdsa::{Signature, signature::Verifier};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How long a challenge can be answered.
pub(crate) const CHALLENGE_TTL_MS: u64 = 60_000;
/// How long a session's bearer token is accepted.
pub(crate) const SESSION_TTL_MS: u64 = 12 * 60 * 60 * 1000;
/// How long one started on a web origin is: a stolen token soon expires,
/// and the next needs the passkey again.
pub(crate) const WEB_SESSION_TTL_MS: u64 = 10 * 60 * 1000;
/// Unanswered challenges kept per device; a new one evicts the oldest.
pub(crate) const MAX_CHALLENGES_PER_DEVICE: usize = 8;
/// Sessions kept per device; a new one ends the oldest.
const MAX_SESSIONS_PER_DEVICE: usize = 16;
/// What a device signs before the challenge nonce, so its key's signature
/// over a nonce means nothing elsewhere.
const SIGNED_PREFIX: &[u8] = b"herdr-remote-session\0";
/// Length of an IEEE P1363 P-256 signature: r || s.
const SIGNATURE_LEN: usize = 64;

/// A challenge, named by the daemon.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ChallengeId(pub(crate) String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Challenge {
    pub(crate) id: ChallengeId,
    /// 32 random bytes, base64url.
    pub(crate) nonce: String,
    pub(crate) expires_ms: u64,
    /// On a web origin, the credential ID of the device's passkey there,
    /// for the browser to ask it for an assertion over the nonce.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) passkey: Option<String>,
}

/// A started session. The token is shown once, to the device.
#[derive(Clone, Debug)]
pub(crate) struct Session {
    /// 32 random bytes, base64url: the bearer token.
    pub(crate) token: SecretString,
    pub(crate) device: DeviceId,
    pub(crate) expires_ms: u64,
}

/// A challenge taken to be answered.
#[derive(Debug)]
pub(crate) struct Pending {
    pub(crate) device: DeviceId,
    pub(crate) nonce: [u8; SECRET_LEN],
    issued_ms: u64,
    expires_ms: u64,
    /// Where it was issued; it is answered there or not at all.
    pub(crate) scope: Scope,
}

#[derive(Debug)]
struct Live {
    device: DeviceId,
    issued_ms: u64,
    expires_ms: u64,
    /// Where it was started; its token works there alone, so a token
    /// from the network is no use on a web origin, which wants a passkey.
    scope: Scope,
}

#[derive(Debug, Default)]
pub(crate) struct Sessions {
    challenges: HashMap<ChallengeId, Pending>,
    /// Keyed by SHA-256 of the bearer token.
    sessions: HashMap<[u8; 32], Live>,
}

impl Sessions {
    /// Issues a single-use challenge to a paired device, answerable in
    /// `scope`. On a web origin, the device must have a passkey there.
    pub(crate) fn challenge(
        &mut self,
        device: &DeviceId,
        scope: Scope,
        now_ms: u64,
        devices: &Devices,
    ) -> Result<Challenge> {
        let record = devices.get(device).ok_or(Error::UnknownDevice)?;
        // A device paired at a web name with a passkey signs in there
        // alone: its key, which a handoff there may replace, never opens a
        // session without the passkey.
        let passkey = match &scope {
            Scope::Network if record.passkey.is_some() => {
                return Err(passkey::Error::WebOnly.into());
            }
            Scope::Network => None,
            Scope::Web(host) => Some(
                record
                    .passkey
                    .as_ref()
                    .filter(|passkey| &passkey.host == host)
                    .ok_or(passkey::Error::NotRegistered)?
                    .id
                    .clone(),
            ),
        };
        self.challenges
            .retain(|_, pending| pending.expires_ms > now_ms);
        let outstanding = self
            .challenges
            .iter()
            .filter(|(_, pending)| &pending.device == device);
        if outstanding.clone().count() >= MAX_CHALLENGES_PER_DEVICE
            && let Some(oldest) = outstanding
                .min_by_key(|(_, pending)| pending.issued_ms)
                .map(|(id, _)| id.clone())
        {
            self.challenges.remove(&oldest);
        }
        let nonce = random_bytes::<SECRET_LEN>()?;
        let id = ChallengeId(random_id()?);
        let expires_ms = now_ms.saturating_add(CHALLENGE_TTL_MS);
        self.challenges.insert(
            id.clone(),
            Pending {
                device: device.clone(),
                nonce,
                issued_ms: now_ms,
                expires_ms,
                scope,
            },
        );
        Ok(Challenge {
            id,
            nonce: encode(&nonce),
            expires_ms,
            passkey,
        })
    }

    /// Takes a live challenge issued in `scope` and checks the device's
    /// signature over it. The challenge is used up whatever the outcome.
    pub(crate) fn take(
        &mut self,
        challenge: &ChallengeId,
        signature: &str,
        scope: &Scope,
        now_ms: u64,
        devices: &Devices,
    ) -> Result<Pending> {
        let pending = self
            .challenges
            .remove(challenge)
            .filter(|pending| pending.expires_ms > now_ms && &pending.scope == scope)
            .ok_or(Error::UnknownChallenge)?;
        let signature: [u8; SIGNATURE_LEN] =
            decode_fixed(signature).map_err(|_| Error::InvalidSignature)?;
        let signature = Signature::from_slice(&signature).map_err(|_| Error::InvalidSignature)?;
        let key = devices
            .get(&pending.device)
            .ok_or(Error::UnknownDevice)?
            .public_key
            .verifying_key()?;
        key.verify(&signed_message(&pending.nonce), &signature)
            .map_err(|_| Error::InvalidSignature)?;
        Ok(pending)
    }

    /// The device a bearer token belongs to, while its session lasts and
    /// only in the scope it was started in.
    pub(crate) fn authenticate(
        &mut self,
        bearer: &str,
        scope: &Scope,
        now_ms: u64,
    ) -> Option<DeviceId> {
        let token: [u8; SECRET_LEN] = decode_fixed(bearer).ok()?;
        let digest = sha256(&[&token]);
        let live = self.sessions.get(&digest)?;
        if live.expires_ms <= now_ms {
            self.sessions.remove(&digest);
            return None;
        }
        (&live.scope == scope).then(|| live.device.clone())
    }

    /// Ends every session and drops every challenge of a device.
    pub(crate) fn end_device(&mut self, device: &DeviceId) {
        self.sessions.retain(|_, live| &live.device != device);
        self.challenges
            .retain(|_, pending| &pending.device != device);
    }

    /// Starts a session for `device` in `scope`.
    pub(crate) fn start(&mut self, device: DeviceId, scope: Scope, now_ms: u64) -> Result<Session> {
        self.sessions.retain(|_, live| live.expires_ms > now_ms);
        let owned = self
            .sessions
            .iter()
            .filter(|(_, live)| live.device == device);
        if owned.clone().count() >= MAX_SESSIONS_PER_DEVICE
            && let Some(oldest) = owned
                .min_by_key(|(_, live)| live.issued_ms)
                .map(|(digest, _)| *digest)
        {
            self.sessions.remove(&oldest);
        }
        let token = random_bytes::<SECRET_LEN>()?;
        let ttl = match scope {
            Scope::Network => SESSION_TTL_MS,
            Scope::Web(_) => WEB_SESSION_TTL_MS,
        };
        let expires_ms = now_ms.saturating_add(ttl);
        self.sessions.insert(
            sha256(&[&token]),
            Live {
                device: device.clone(),
                issued_ms: now_ms,
                expires_ms,
                scope,
            },
        );
        Ok(Session {
            token: encode(&token).into(),
            device,
            expires_ms,
        })
    }
}

/// The bytes a device signs for a challenge's nonce.
pub(crate) fn signed_message(nonce: &[u8]) -> Vec<u8> {
    [SIGNED_PREFIX, nonce].concat()
}
