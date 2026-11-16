//! `remote web status`, `remote web enable`, and `remote web disable`: web
//! access through a private relay (`docs/relay-web-plan.md`). Enabling
//! only opens the window's question, where the user confirms; disabling
//! saves `[remote] relays` at once.
use super::{
    super::{Watches, in_free_window, settings::answer_when_done, socket, window_for},
    OFF,
};
use crate::{
    SheprdWindow,
    config::{
        Config,
        preferences::Preference,
        remote::{Edit, Relay, relay_address},
    },
    control::protocol::{
        Caller, ErrorCode, Response,
        remote::{RemoteOp, Reply, WebRelay},
    },
    remote::{
        Supervisor,
        web::{self, WebAsk},
    },
};
use gpui::{App, Context, Window};
use sheprd_remote_control::control::{Request, Status};
use std::time::Duration;

/// Why a relay without a token file cannot serve web access.
const PRIVATE: &str = "web access needs a private relay: set its listen token file in Settings > Remote control > Relays first";

pub(super) fn serve(
    caller: &Caller,
    op: RemoteOp,
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    match op {
        RemoteOp::WebStatus => status(caller, wait, incoming, watches, cx),
        RemoteOp::WebEnable { relay } => {
            incoming.respond(in_free_window(caller, cx, |view, window, cx| {
                ask(view, relay.as_deref(), window, cx)
            }))
        }
        RemoteOp::WebDisable { relay } => {
            disable(caller, relay.as_deref(), wait, incoming, watches, cx)
        }
        RemoteOp::Status
        | RemoteOp::Pair { .. }
        | RemoteOp::Devices
        | RemoteOp::Revoke { .. }
        | RemoteOp::Restart => incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "Not a web access request",
        )),
    }
}

fn status(
    caller: &Caller,
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let (enabled, relays) = match window_for(caller, cx) {
        Ok(handle) => handle
            .read_with(cx, |view, _| {
                (
                    view.config.remote.enabled,
                    view.config.remote.relays.clone(),
                )
            })
            .unwrap_or_default(),
        Err(response) => return incoming.respond(response),
    };
    if !enabled {
        return incoming.respond(Response::Remote {
            reply: Reply::Web {
                relays: listed(&relays, None),
                error: None,
            },
        });
    }
    let Some(client) = Supervisor::client(cx) else {
        return incoming.respond(Response::error(
            ErrorCode::Unsupported,
            "This Sheprd does not run remote control",
        ));
    };
    let request = client.request(Request::Status);
    let task = cx.spawn(async move |_| {
        let (status, error) = match request.await {
            Ok(sheprd_remote_control::control::Response::Status(status)) => (Some(status), None),
            Ok(_) => (None, Some(crate::remote::Error::Unexpected.to_string())),
            Err(error) => (None, Some(error.to_string())),
        };
        Response::Remote {
            reply: Reply::Web {
                relays: listed(&relays, status.as_ref()),
                error,
            },
        }
    });
    answer_when_done(task, wait, incoming, watches, cx);
}

/// Each configured relay that has a token file, has web access on, or is
/// reported serving it, with what the daemon says about it.
pub(super) fn listed(relays: &[Relay], status: Option<&Status>) -> Vec<WebRelay> {
    relays
        .iter()
        .filter_map(|relay| {
            let reported = status.and_then(|status| web::reported(status, &relay.endpoint));
            let web = reported.and_then(|(_, reported)| reported.web.clone());
            (relay.token_file.is_some() || relay.web || web.is_some()).then(|| WebRelay {
                enabled: relay.web,
                endpoint: reported.map_or_else(
                    || relay_address(&relay.endpoint).unwrap_or_else(|_| relay.endpoint.clone()),
                    |(name, _)| name.clone(),
                ),
                name: relay.label.name.clone(),
                status: web,
                token_file: relay.token_file.is_some(),
            })
        })
        .collect()
}

/// The relay `needle` names, by its address however spelled or by its name;
/// without one, the only relay with a token file.
pub(super) fn pick<'a>(relays: &'a [Relay], needle: Option<&str>) -> Result<&'a Relay, Response> {
    let Some(needle) = needle.map(str::trim) else {
        let mut private = relays.iter().filter(|relay| relay.token_file.is_some());
        return match (private.next(), private.next()) {
            (Some(relay), None) => Ok(relay),
            (Some(_), Some(_)) => Err(Response::error(
                ErrorCode::InvalidRequest,
                "Several relays have a listen token file; name one with --relay",
            )),
            (None, _) if relays.is_empty() => Err(Response::error(
                ErrorCode::NotFound,
                "No relay is configured; add a private relay in Settings > Remote control > Relays",
            )),
            (None, _) => Err(Response::error(
                ErrorCode::InvalidRequest,
                format!("No relay has a listen token file; {PRIVATE}"),
            )),
        };
    };
    if let Some(relay) = relays
        .iter()
        .find(|relay| web::same_relay(&relay.endpoint, needle))
    {
        return Ok(relay);
    }
    let mut named = relays
        .iter()
        .filter(|relay| relay.label.name.as_deref() == Some(needle));
    match (named.next(), named.next()) {
        (Some(relay), None) => Ok(relay),
        (Some(_), Some(_)) => Err(Response::error(
            ErrorCode::InvalidRequest,
            format!("Several relays are named {needle}; name one by its address"),
        )),
        (None, _) => Err(Response::error(
            ErrorCode::NotFound,
            format!("No relay in [remote] relays is named {needle}"),
        )),
    }
}

/// What the answer and the question call a relay.
fn display(relay: &Relay) -> String {
    match &relay.label.name {
        Some(name) => crate::notifications::safe_text(name, 64),
        None => crate::notifications::safe_text(&relay.endpoint, 200),
    }
}

/// Opens the question for turning web access on through the relay `needle`
/// names, unless it is on already or cannot be.
pub(super) fn ask(
    view: &mut SheprdWindow,
    needle: Option<&str>,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    if !view.config.remote.enabled {
        return Response::error(ErrorCode::Failed, OFF);
    }
    let relays = &view.config.remote.relays;
    let relay = match pick(relays, needle) {
        Ok(relay) => relay,
        Err(response) => return response,
    };
    let name = display(relay);
    if relay.token_file.is_none() {
        return Response::error(
            ErrorCode::InvalidRequest,
            format!("{name} has no listen token file; {PRIVATE}"),
        );
    }
    if web::turn_on(relays, &relay.endpoint, &Supervisor::health(cx)).is_empty() {
        return Response::done(format!("Web access through {name} is already on"));
    }
    let ask = WebAsk {
        endpoint: relay.endpoint.clone(),
        name: name.clone(),
    };
    if view.ask_web_access(ask, window, cx) {
        Response::done(format!(
            "Asking in Sheprd to turn on web access through {name}; the user confirms there"
        ))
    } else {
        Response::error(
            ErrorCode::WindowBusy,
            "A dialog is open in the window; close it first",
        )
    }
}

fn disable(
    caller: &Caller,
    needle: Option<&str>,
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let relays = match window_for(caller, cx) {
        Ok(handle) => handle
            .read_with(cx, |view, _| view.config.remote.relays.clone())
            .unwrap_or_default(),
        Err(response) => return incoming.respond(response),
    };
    let (name, saved) = match pick(&relays, needle) {
        Ok(relay) => (display(relay), web::turn_off(&relays, &relay.endpoint)),
        Err(response) => return incoming.respond(response),
    };
    let Some(saved) = saved else {
        return incoming.respond(Response::done(format!(
            "Web access through {name} is already off"
        )));
    };
    let task = cx.background_executor().spawn(async move {
        match Config::save_preference(Preference::Remote(Edit::Relays(saved))) {
            Ok(()) => Response::done(format!("Turned off web access through {name}")),
            Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
        }
    });
    answer_when_done(task, wait, incoming, watches, cx);
}

#[cfg(test)]
mod tests;
