//! The JSON API: pairing, sessions, chat commands, new workspaces and
//! chats, and push subscriptions. Auth calls that write to disk run on the
//! blocking pool.

use super::{
    ApiError, Device, Peer, Shared, TypedFrom,
    fragments::folders,
    render::{now_ms, urlencode},
};
use crate::{
    auth::{ChallengeId, ClaimId, Scope, passkey},
    control::{DeviceId, PairingState},
    push::Subscription,
};
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, header},
};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use serde_json::{Value, json};
use sheprd_chat_protocol::{Agent, Command};
use std::time::Duration;

/// How long starting a chat waits for it to open before answering with the
/// list instead.
pub(crate) const NEW_CHAT_WAIT: Duration = Duration::from_secs(10);

/// Runs `action` with the auth state on the blocking pool.
pub(super) async fn with_auth<T: Send + 'static>(
    state: &Shared,
    action: impl FnOnce(&mut crate::auth::Auth) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || action(&mut state.auth()))
        .await
        .map_err(|_| ApiError::Invalid("the request was interrupted"))?
}

/// Says which computer answers, by its certificate's fingerprint: an
/// address a phone saved may belong to another computer on another network.
///
/// `passkey` says that a device pairs and signs in here with a passkey:
/// this is a web name through a relay.
pub(super) async fn ping(State(state): State<Shared>, scope: Scope) -> Json<Value> {
    Json(json!({
        "ok": true,
        "computer": state.computer,
        "fingerprint": state.fingerprint,
        "passkey": scope.web_host().is_some(),
    }))
}

#[derive(Deserialize)]
pub(super) struct ClaimRequest {
    token: SecretString,
    name: String,
    public_key: String,
    /// The phone's relay keys, when the computer's nearby offer named
    /// relays: stored with the device when approved.
    #[serde(default)]
    relay: Option<RelayKeysRequest>,
    /// The passkey the phone made for this claim, which a claim on a web
    /// origin must carry.
    #[serde(default)]
    passkey: Option<passkey::Registration>,
    /// Another computer's Sheprd asking to open terminals here over SSH
    /// through the relays (`crate::ssh_access`): its `ssh-ed25519` key.
    #[serde(default)]
    ssh: Option<SshRequest>,
}

#[derive(Deserialize)]
pub(super) struct SshRequest {
    public_key: String,
}

#[derive(Deserialize)]
pub(super) struct RelayKeysRequest {
    dial: String,
    tunnel: String,
}

/// A claim from an app installed from another computer carries that app's
/// origin, which approving the claim admits; a claim from this computer's
/// own page carries none.
///
/// On a web origin, the claim must carry a passkey made there, with
/// person verification, for the challenge [`passkey::register_challenge`]
/// derives from the token and the device key: the device signs in there
/// with it.
pub(super) async fn claim(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    scope: Scope,
    headers: HeaderMap,
    Json(request): Json<ClaimRequest>,
) -> Result<Json<Value>, ApiError> {
    let app = headers
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        // Every device paired from another computer's app records that
        // origin, even one already admitted for another device, so revoking
        // one leaves the others working.
        .filter(|origin| !state.serves(origin))
        .map(|origin| app_origin(origin).ok_or(ApiError::Invalid("the app's origin is not valid")))
        .transpose()?;
    // A desktop claims through the relays' tunnel, never at a web name
    // open to the internet, where a stolen token must not buy SSH access.
    if request.ssh.is_some() {
        network_only(&scope)?;
    }
    let ssh = desktop_key(&state, request.ssh.as_ref(), request.relay.as_ref()).await?;
    let claim = with_auth(&state, move |auth| {
        let passkey = match scope.web_host() {
            Some(host) => Some(claim_passkey(
                host,
                request.token.expose_secret(),
                &request.public_key,
                request.passkey.as_ref(),
            )?),
            None => None,
        };
        if ssh.is_some() {
            auth.ssh_access()?;
        }
        let claim = auth.claim(
            peer.ip(),
            request.token.expose_secret(),
            &request.name,
            &request.public_key,
            now_ms(),
        )?;
        if let Some(app) = app {
            auth.claimed_from(&claim.id, app);
        }
        if let Some(passkey) = passkey {
            auth.claimed_passkey(&claim.id, passkey);
        }
        if let Some(key) = ssh {
            auth.claimed_ssh(&claim.id, key);
        }
        if let Some(relay) = request.relay {
            let keys = crate::auth::devices::RelayKeys {
                dial: relay.dial,
                tunnel: relay.tunnel,
            };
            // Keys that are not keys are left out; the phone can send them
            // again once paired.
            if keys.dial_key().is_some() && keys.tunnel_key().is_some() {
                auth.claimed_relay(&claim.id, keys);
            }
        }
        Ok(claim)
    })
    .await?;
    Ok(Json(json!({
        "claim": claim.id.0,
        "code": claim.code,
        "expires_ms": claim.expires_ms,
    })))
}

/// Refuses what only the native app or a network path asks for on a web
/// name, open to the internet: a token stolen there lasts minutes, and
/// must not buy keys or tokens that outlast it without the passkey.
pub(super) fn network_only(scope: &Scope) -> Result<(), ApiError> {
    match scope.web_host() {
        Some(_) => Err(ApiError::NotFound),
        None => Ok(()),
    }
}

/// The passkey a claim on the web name `host` made: checked before the
/// token is spent, so a claim without one leaves the pairing open.
fn claim_passkey(
    host: &str,
    token: &str,
    public_key: &str,
    registration: Option<&passkey::Registration>,
) -> Result<passkey::Passkey, ApiError> {
    let registration = registration.ok_or(crate::auth::Error::from(passkey::Error::Required))?;
    let token: [u8; 32] =
        crate::auth::decode_fixed(token).map_err(|_| crate::auth::Error::InvalidToken)?;
    let public_key = crate::auth::PublicKey::parse(public_key)?;
    let challenge = passkey::register_challenge(&token, public_key.as_bytes());
    Ok(passkey::register(host, &challenge, registration).map_err(crate::auth::Error::from)?)
}

/// The SSH key a desktop claims with, checked before its token is spent:
/// a desktop reaches this computer only through the relays, so it must send
/// relay keys too, and an SSH server must be listening to splice it to.
async fn desktop_key(
    state: &Shared,
    ssh: Option<&SshRequest>,
    relay: Option<&RelayKeysRequest>,
) -> Result<Option<crate::ssh_access::SshKey>, ApiError> {
    let Some(ssh) = ssh else {
        return Ok(None);
    };
    let key =
        crate::ssh_access::SshKey::parse(&ssh.public_key).map_err(crate::auth::Error::from)?;
    let reachable = relay.is_some_and(|relay| {
        let keys = crate::auth::devices::RelayKeys {
            dial: relay.dial.clone(),
            tunnel: relay.tunnel.clone(),
        };
        keys.dial_key().is_some() && keys.tunnel_key().is_some()
    });
    if !reachable {
        return Err(ApiError::Invalid("a desktop must send relay keys"));
    }
    let access = with_auth(state, |auth| Ok(auth.ssh_access()?.clone())).await?;
    let listening = tokio::task::spawn_blocking(move || access.server_listening())
        .await
        .unwrap_or(false);
    if !listening {
        return Err(crate::auth::Error::from(crate::ssh_access::Error::ServerOff).into());
    }
    Ok(Some(key))
}

/// `origin` when it is an `https://` origin as a browser sends it: a host
/// and an optional port, nothing more, at most 255 bytes.
pub(super) fn app_origin(origin: &str) -> Option<String> {
    let host = origin.strip_prefix("https://")?;
    let valid = !host.is_empty()
        && origin.len() <= 255
        && host.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        });
    valid.then(|| origin.to_owned())
}

pub(super) async fn claim_status(
    State(state): State<Shared>,
    scope: Scope,
    Path(claim): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (pairing, login) = with_auth(&state, move |auth| {
        let pairing = auth.claim_status(&ClaimId(claim), now_ms())?;
        // A desktop learns who to log in as, and which host keys to pin,
        // only once allowed.
        let login = match &pairing {
            PairingState::Approved { device } => auth.desktop_login(device)?,
            _ => None,
        };
        Ok((pairing, login))
    })
    .await?;
    Ok(Json(match pairing {
        PairingState::Approved { device } => {
            let mut approved = json!({
                "state": "approved",
                "device": device.0,
                "origins": state.origins_for(&scope),
                "fingerprint": state.fingerprint,
                "computer": state.computer,
            });
            if let Some(login) = login {
                approved["ssh"] = json!(login);
            }
            approved
        }
        PairingState::Claimed { .. } | PairingState::Waiting { .. } => json!({"state": "claimed"}),
        PairingState::Denied => json!({"state": "denied"}),
        PairingState::Expired => json!({"state": "expired"}),
    }))
}

/// A token the paired browser puts in its Home Screen app's start address.
pub(super) async fn handoff(
    State(state): State<Shared>,
    Device(device): Device,
    scope: Scope,
) -> Result<Json<Value>, ApiError> {
    let handoff = with_auth(&state, move |auth| {
        Ok(auth.handoff_in(&device, &scope, now_ms())?)
    })
    .await?;
    Ok(Json(json!({
        "token": handoff.token.expose_secret(),
        "expires_ms": handoff.expires_ms,
    })))
}

#[derive(Deserialize)]
pub(super) struct RedeemRequest {
    token: SecretString,
    public_key: String,
}

/// The Home Screen app takes over its browser's device with its own key;
/// it answers as an approved claim does.
pub(super) async fn redeem(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    scope: Scope,
    Json(request): Json<RedeemRequest>,
) -> Result<Json<Value>, ApiError> {
    let origins = state.origins_for(&scope);
    let device = with_auth(&state, move |auth| {
        Ok(auth.redeem_in(
            peer.ip(),
            request.token.expose_secret(),
            &request.public_key,
            &scope,
            now_ms(),
        )?)
    })
    .await?;
    Ok(Json(json!({
        "device": device.0,
        "origins": origins,
        "fingerprint": state.fingerprint,
        "computer": state.computer,
    })))
}

#[derive(Deserialize)]
pub(super) struct ChallengeRequest {
    device: String,
}

/// A challenge to sign; on a web origin, it names the device's passkey
/// there, which must sign it too.
pub(super) async fn challenge(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    scope: Scope,
    Json(request): Json<ChallengeRequest>,
) -> Result<Json<Value>, ApiError> {
    let challenge = with_auth(&state, move |auth| {
        Ok(auth.challenge_in(peer.ip(), &DeviceId(request.device), scope, now_ms())?)
    })
    .await?;
    Ok(Json(json!({
        "challenge": challenge.id.0,
        "nonce": challenge.nonce,
        "expires_ms": challenge.expires_ms,
        "passkey": challenge.passkey,
    })))
}

#[derive(Deserialize)]
pub(super) struct SessionRequest {
    challenge: String,
    signature: String,
    /// The passkey's assertion over the nonce, which a web origin needs.
    #[serde(default)]
    passkey: Option<passkey::Assertion>,
}

pub(super) async fn session(
    State(state): State<Shared>,
    scope: Scope,
    Json(request): Json<SessionRequest>,
) -> Result<Json<Value>, ApiError> {
    let origins = state.origins_for(&scope);
    let session = with_auth(&state, move |auth| {
        Ok(auth.verify_in(
            &ChallengeId(request.challenge),
            &request.signature,
            &scope,
            request.passkey.as_ref(),
            now_ms(),
        )?)
    })
    .await?;
    // The addresses served now, so an installed app that paired on another
    // network can still find this computer later.
    Ok(Json(json!({
        "token": session.token.expose_secret(),
        "expires_ms": session.expires_ms,
        "origins": origins,
    })))
}

/// The addresses served now, for a phone that signed in earlier: networks
/// change while a session lasts, and its saved list must follow them.
pub(super) async fn origins(
    State(state): State<Shared>,
    _device: Device,
    scope: Scope,
) -> Json<Value> {
    Json(json!({ "origins": state.origins_for(&scope) }))
}

pub(super) async fn command(
    State(state): State<Shared>,
    _device: Device,
    TypedFrom(origin): TypedFrom,
    Path(pane): Path<String>,
    Json(command): Json<Command>,
) -> Result<Json<Value>, ApiError> {
    // A phone names only pictures it uploaded, never a path here, and the
    // connection says where it typed, whatever the phone claims.
    let command = match command {
        Command::Prompt {
            text, attachments, ..
        } => Command::Prompt {
            text,
            attachments: state.uploads.resolve(&attachments, now_ms())?,
            origin: Some(origin),
        },
        command => command,
    };
    let rejected = state.hub.command(&pane, command).await?;
    Ok(Json(
        json!({ "rejected": rejected.map(|rejection| rejection.to_string()) }),
    ))
}

pub(super) async fn upload(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    bytes: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    state.hub.chat(&pane).ok_or(ApiError::NotFound)?;
    let id = crate::auth::random_id()?;
    let stored = state.clone();
    let id = tokio::task::spawn_blocking(move || stored.uploads.store(id, &bytes, now_ms()))
        .await
        .map_err(|_| ApiError::Invalid("the upload was interrupted"))??;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
pub(super) struct NewWorkspace {
    cwd: String,
    #[serde(default)]
    branch: String,
    agent: String,
}

pub(super) async fn create_workspace(
    State(state): State<Shared>,
    _device: Device,
    Json(request): Json<NewWorkspace>,
) -> Result<Json<Value>, ApiError> {
    let (agent, branch) =
        check_new_workspace(&state, &request.cwd, &request.branch, &request.agent).await?;
    state
        .hub
        .create_workspace(&request.cwd, branch.as_deref(), agent, None)
        .await?;
    Ok(Json(json!({ "location": "/" })))
}

/// The agent a request names, by its `name()`.
pub(crate) fn parse_agent(name: &str) -> Result<Agent, ApiError> {
    name.parse().map_err(|_| ApiError::Invalid("unknown agent"))
}

/// Checks a new workspace request: a known agent, a folder the form
/// offers (never a path the phone made up), and a valid branch if any.
/// Answers the agent and the trimmed branch, `None` for no worktree.
pub(crate) async fn check_new_workspace(
    state: &Shared,
    cwd: &str,
    branch: &str,
    agent: &str,
) -> Result<(Agent, Option<String>), ApiError> {
    let agent = parse_agent(agent)?;
    let layout = state.hub.layout();
    let offered = tokio::task::spawn_blocking(move || folders(&layout))
        .await
        .map_err(|_| ApiError::Folder)?;
    if !offered.offers(cwd) {
        return Err(ApiError::Folder);
    }
    let branch = branch.trim();
    if !branch.is_empty() && !valid_branch(branch) {
        return Err(ApiError::Invalid("that is not a valid branch name"));
    }
    Ok((agent, (!branch.is_empty()).then(|| branch.to_owned())))
}

/// A conservative subset of Git's branch name rules.
pub(crate) fn valid_branch(name: &str) -> bool {
    name.len() <= 200
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
}

#[derive(Deserialize)]
pub(super) struct NewChat {
    agent: String,
}

pub(super) async fn start_chat(
    State(state): State<Shared>,
    _device: Device,
    Path(workspace): Path<String>,
    Json(request): Json<NewChat>,
) -> Result<Json<Value>, ApiError> {
    let agent = parse_agent(&request.agent)?;
    let pane = state.hub.start_chat(&workspace, agent, None).await?;
    // The chat is listed once its program reports in; open it then, or
    // the list when it is slow to start.
    let location = if state.hub.chat_ready(&pane, NEW_CHAT_WAIT).await {
        format!("/chat/{}", urlencode(&pane))
    } else {
        "/".to_owned()
    };
    Ok(Json(json!({ "location": location })))
}

/// The iOS companion's Live Activity tokens: the push-to-start token when
/// it changed, and every activity it still shows.
pub(super) async fn register_apns(
    State(state): State<Shared>,
    Device(device): Device,
    scope: Scope,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    network_only(&scope)?;
    if state.apns.is_none() {
        return Err(ApiError::NoPush);
    }
    let update = crate::apns::Registration::parse(&body)
        .map_err(|_| ApiError::Invalid("that registration is not valid"))?;
    with_auth(&state, move |auth| {
        let devices = auth.devices_mut();
        let mut stored = crate::apns::Registration::from_stored(
            devices.get(&device).and_then(|record| record.apns.as_ref()),
        );
        stored.merge(update);
        let value = stored
            .to_stored()
            .map_err(|_| ApiError::Invalid("that registration cannot be stored"))?;
        Ok(devices.set_apns(&device, value)?)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn push_key(
    State(state): State<Shared>,
    _device: Device,
) -> Result<Json<Value>, ApiError> {
    let sender = state.push.as_ref().ok_or(ApiError::NoPush)?;
    Ok(Json(json!({ "key": sender.vapid().public_key_b64url() })))
}

pub(super) async fn subscribe_push(
    State(state): State<Shared>,
    Device(device): Device,
    Json(subscription): Json<Subscription>,
) -> Result<Json<Value>, ApiError> {
    if state.push.is_none() {
        return Err(ApiError::NoPush);
    }
    // Deserializing validated it: an allowed push service, sound keys.
    let value = serde_json::to_value(&subscription)
        .map_err(|_| ApiError::Invalid("that subscription cannot be stored"))?;
    with_auth(&state, move |auth| {
        Ok(auth.devices_mut().set_push(&device, Some(value))?)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn unsubscribe_push(
    State(state): State<Shared>,
    Device(device): Device,
) -> Result<Json<Value>, ApiError> {
    with_auth(&state, move |auth| {
        Ok(auth.devices_mut().set_push(&device, None)?)
    })
    .await?;
    Ok(Json(json!({ "ok": true })))
}
