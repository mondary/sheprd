//! Pairing and signing in on a web origin through a relay, where a device
//! needs a passkey for that name: through the router, with a software
//! authenticator.

use super::*;
use crate::auth::{
    Scope, WEB_SESSION_TTL_MS,
    passkey::{register_challenge, soft::Authenticator},
};
use crate::control::{PairingState, Request, Response};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde_json::{Value, json};

const HOST: &str = "abcdefghijklmnopqrstuvwxyz234567.fra1.sheprd-relay.dev";

/// Sends `request` as a browser at the web name would reach it.
async fn send_web(
    state: &Shared,
    mut request: HttpRequest<Body>,
) -> (StatusCode, HeaderMap, String) {
    request.extensions_mut().insert(Scope::Web(HOST.to_owned()));
    send(state, request).await
}

fn control(state: &Shared, request: Request) -> Response {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, request)
}

fn with_bearer(path: &str, token: &str) -> HttpRequest<Body> {
    HttpRequest::get(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Asks for a challenge at the web name and answers it, with `assertion`
/// made by `sign` over the nonce: the session's answer.
async fn sign_in(
    state: &Shared,
    device: &str,
    key: &SigningKey,
    passkey: impl FnOnce(&[u8]) -> Option<Value>,
) -> (StatusCode, Value) {
    let (status, _, challenge) = send_web(
        state,
        post("/api/session/challenge", json!({"device": device})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{challenge}");
    let challenge: Value = serde_json::from_str(&challenge).unwrap();
    let nonce = URL_SAFE_NO_PAD
        .decode(challenge["nonce"].as_str().unwrap())
        .unwrap();
    let mut message = b"herdr-remote-session\0".to_vec();
    message.extend_from_slice(&nonce);
    let signature: Signature = key.sign(&message);
    let mut body = json!({
        "challenge": challenge["challenge"],
        "signature": URL_SAFE_NO_PAD.encode(signature.to_bytes()),
    });
    if let Some(assertion) = passkey(&nonce) {
        body["passkey"] = assertion;
    }
    let (status, _, session) = send_web(state, post("/api/session", body)).await;
    (status, serde_json::from_str(&session).unwrap())
}

#[tokio::test]
async fn a_browser_on_the_web_name_pairs_and_signs_in_with_a_passkey() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    // The pairing page answers there only while a code is shown.
    assert_eq!(
        send_web(&state, get("/pair")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(send(&state, get("/pair")).await.0, StatusCode::OK);
    let Response::Offer(offer) = control(&state, Request::Pair) else {
        panic!("expected an offer");
    };
    assert_eq!(send_web(&state, get("/pair")).await.0, StatusCode::OK);
    let token = offer
        .url
        .split_once("#t=")
        .unwrap()
        .1
        .split('&')
        .next()
        .unwrap()
        .to_owned();
    let key = SigningKey::from_slice(&[7; 32]).unwrap();
    let public = key.verifying_key().to_sec1_point(false);
    let public = URL_SAFE_NO_PAD.encode(public.as_bytes());
    let claim = |passkey: Option<Value>| {
        let mut body = json!({"token": token, "name": "Pixel", "public_key": public});
        if let Some(passkey) = passkey {
            body["passkey"] = passkey;
        }
        post("/api/pair/claim", body)
    };

    // Without a passkey, the claim is refused and the token stays good.
    let (status, _, body) = send_web(&state, claim(None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // One made for another claim is refused too.
    let authenticator = Authenticator::new(3);
    let wrong = authenticator.register(HOST, &[1; 32]);
    let wrong = json!({"client_data": wrong.client_data, "attestation": wrong.attestation});
    assert_eq!(
        send_web(&state, claim(Some(wrong))).await.0,
        StatusCode::BAD_REQUEST
    );

    let challenge = register_challenge(
        &URL_SAFE_NO_PAD.decode(&token).unwrap(),
        &URL_SAFE_NO_PAD.decode(&public).unwrap(),
    );
    let made = authenticator.register(HOST, &challenge);
    let made = json!({"client_data": made.client_data, "attestation": made.attestation});
    let (status, _, body) = send_web(&state, claim(Some(made))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let code = serde_json::from_str::<Value>(&body).unwrap()["code"]
        .as_str()
        .unwrap()
        .to_owned();
    let Response::Pairing(PairingState::Claimed { passkey, .. }) = control(
        &state,
        Request::Pairing {
            id: offer.id.clone(),
        },
    ) else {
        panic!("expected a claimed pairing");
    };
    assert!(passkey, "the desktop is told the device uses a passkey");
    control(
        &state,
        Request::Approve {
            id: offer.id,
            allow: true,
            code,
            hosts: Vec::new(),
        },
    );
    let device = state.auth().devices().list(|_| false)[0].id.0.clone();

    // The challenge names the passkey; answering without it fails.
    let (status, _, challenge) = send_web(
        &state,
        post("/api/session/challenge", json!({"device": device})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let challenge: Value = serde_json::from_str(&challenge).unwrap();
    assert_eq!(
        challenge["passkey"],
        URL_SAFE_NO_PAD.encode(&authenticator.credential)
    );
    let (status, refused) = sign_in(&state, &device, &key, |_| None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");

    let (status, session) = sign_in(&state, &device, &key, |nonce| {
        let assertion = authenticator.assert(HOST, nonce);
        Some(json!({
            "client_data": assertion.client_data,
            "authenticator_data": assertion.authenticator_data,
            "signature": assertion.signature,
        }))
    })
    .await;
    assert_eq!(status, StatusCode::OK, "{session}");
    let lasts = session["expires_ms"].as_u64().unwrap() - now_ms();
    assert!(lasts <= WEB_SESSION_TTL_MS, "{lasts}");
    let token = session["token"].as_str().unwrap();
    // The page learns its own web name is this computer's.
    assert!(
        session["origins"]
            .as_array()
            .unwrap()
            .contains(&json!(format!("https://{HOST}")))
    );

    // The token works at the web name alone, with its hardening headers.
    let (status, headers, _) = send_web(&state, with_bearer("/f/home", token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key(header::STRICT_TRANSPORT_SECURITY));
    assert_eq!(headers["cross-origin-opener-policy"], "same-origin");
    assert_eq!(
        send(&state, with_bearer("/f/home", token)).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_token_from_the_network_is_refused_on_the_web_name() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, token) = signed_in(&state, 5);
    assert_eq!(
        send(&state, with_bearer("/f/home", &token)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send_web(&state, with_bearer("/f/home", &token)).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Nor can a device without a passkey there ask to sign in.
    let (status, _, _) = send_web(
        &state,
        post("/api/session/challenge", json!({"device": device.0})),
    )
    .await;
    assert_ne!(status, StatusCode::OK);
}
