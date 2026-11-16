//! `remote`: remote control from a phone, and web access through a
//! private relay.
use super::{Args, CliError, unknown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteCommand {
    /// Whether the daemon runs, where, and who is connected.
    Status,
    /// Open the pairing dialog; never prints a token.
    Pair {
        address: Option<String>,
    },
    Devices,
    /// Open the confirmation for revoking a device, by ID or name.
    Revoke {
        device: String,
    },
    /// Restart the daemon from the binary beside the app.
    Restart,
    /// Web access on each relay that offers it.
    WebStatus,
    /// Open the confirmation for turning web access on through a relay.
    WebEnable {
        relay: Option<String>,
    },
    /// Turn web access off through a relay.
    WebDisable {
        relay: Option<String>,
    },
}

pub(super) fn remote(args: &mut Args) -> Result<RemoteCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(RemoteCommand::Status);
    };
    Ok(match verb.to_str() {
        Some("status") => RemoteCommand::Status,
        Some("pair") => RemoteCommand::Pair {
            address: args.text_option("--address")?,
        },
        Some("devices" | "list") => RemoteCommand::Devices,
        Some("restart") => RemoteCommand::Restart,
        // A name may be typed unquoted, as `remote revoke Fabien's iPhone`.
        Some("revoke") => RemoteCommand::Revoke {
            device: args.words()?.ok_or(CliError::MissingArgument {
                command: "remote revoke",
                what: "DEVICE",
            })?,
        },
        Some("web") => web(args)?,
        _ => return Err(unknown("remote", verb)),
    })
}

/// `remote web [status]`, `remote web enable|disable [--relay NAME]`.
fn web(args: &mut Args) -> Result<RemoteCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(RemoteCommand::WebStatus);
    };
    Ok(match verb.to_str() {
        Some("status") => RemoteCommand::WebStatus,
        Some("enable") => RemoteCommand::WebEnable {
            relay: args.text_option("--relay")?,
        },
        Some("disable") => RemoteCommand::WebDisable {
            relay: args.text_option("--relay")?,
        },
        _ => return Err(unknown("remote web", verb)),
    })
}
