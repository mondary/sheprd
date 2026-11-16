//! `GET /api/relay` tells a paired phone how to reach this computer through
//! its relays: the route, the relays, and the tunnel identity to pin.
//! `POST /api/relay` takes the phone's own relay keys, which the relays then
//! let through (`relay`). Both need a signed-in device, so a phone learns
//! them over a path it already trusts (the network or Tailscale) before it
//! ever uses a relay.

use super::{ApiError, Device, Shared};
use crate::auth::{decode_fixed, devices::RelayKeys};
use axum::{Json, extract::State};
use serde::Deserialize;

pub(super) async fn info(
    State(state): State<Shared>,
    _device: Device,
    scope: crate::auth::Scope,
) -> Result<Json<crate::relay::Info>, ApiError> {
    super::api::network_only(&scope)?;
    let relay = state.relay.as_ref().ok_or(ApiError::NotFound)?;
    relay.info().map(Json).ok_or(ApiError::NotFound)
}

/// A phone's relay keys, base64url.
#[derive(Deserialize)]
pub(super) struct Keys {
    /// Its dial key: P-256, SEC1 compressed.
    dial: String,
    /// Its tunnel static key: X25519.
    tunnel: String,
}

///
/// Not on a web name: a tunnel key enrolled there would let its holder
/// dial in as this device without the passkey.
pub(super) async fn enroll(
    State(state): State<Shared>,
    Device(device): Device,
    scope: crate::auth::Scope,
    Json(keys): Json<Keys>,
) -> Result<Json<serde_json::Value>, ApiError> {
    super::api::network_only(&scope)?;
    let relay = state.relay.clone().ok_or(ApiError::NotFound)?;
    let keys = RelayKeys {
        dial: keys.dial,
        tunnel: keys.tunnel,
    };
    if keys.dial_key().is_none() {
        return Err(ApiError::Invalid("the dial key is not a P-256 key"));
    }
    if decode_fixed::<32>(&keys.tunnel).is_err() {
        return Err(ApiError::Invalid("the tunnel key is not an X25519 key"));
    }
    let state_for_write = state.clone();
    let stored = tokio::task::spawn_blocking(move || {
        state_for_write
            .auth()
            .devices_mut()
            .set_relay(&device, keys)
    })
    .await
    .map_err(|_| ApiError::NotFound)??;
    if !stored {
        return Err(ApiError::Unauthorized);
    }
    relay.devices_changed();
    Ok(Json(serde_json::json!({})))
}
