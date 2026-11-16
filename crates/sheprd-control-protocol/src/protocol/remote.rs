//! Remote control: the daemon's status and devices, the pairing dialog, the
//! revoke confirmation, restarting the daemon, and web access through a
//! private relay. The daemon's own types are reused as they are; no pairing
//! token ever crosses this socket.
use serde::{Deserialize, Serialize};
use sheprd_remote_control::control::{Device, Status, web::WebStatus};

/// Asking the daemon, and for `revoke` the device list first, takes up to
/// its own answer timeout a request.
const DAEMON_WAIT_SECONDS: u64 = 12;
/// Writing `[remote] relays` takes the config lock, which another save may
/// hold for a moment.
const SAVE_WAIT_SECONDS: u64 = 5;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteOp {
    Status,
    /// Opens the pairing dialog in the window.
    Pair {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        address: Option<String>,
    },
    Devices,
    /// Opens the confirmation for revoking a device, by ID or name.
    Revoke {
        device: String,
    },
    /// Restarts the daemon from the binary beside the app.
    Restart,
    /// Web access on each relay that offers it (`docs/relay-web-plan.md`).
    WebStatus,
    /// Opens the confirmation for turning web access on through a relay,
    /// named by its address or name; the only private relay without one.
    WebEnable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay: Option<String>,
    },
    /// Turns web access off through a relay at once, named as for
    /// [`Self::WebEnable`].
    WebDisable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relay: Option<String>,
    },
}

impl RemoteOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Status | Self::Devices | Self::Revoke { .. } | Self::WebStatus => {
                DAEMON_WAIT_SECONDS
            }
            // Saving the config runs off the UI thread before the answer.
            Self::WebDisable { .. } => SAVE_WAIT_SECONDS,
            Self::Pair { .. } | Self::Restart | Self::WebEnable { .. } => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Status {
        /// Whether remote control is on in Settings.
        enabled: bool,
        /// What the daemon says, when it answers.
        #[serde(default)]
        status: Option<Box<Status>>,
        /// Why it did not, when it is on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Devices {
        devices: Vec<Device>,
    },
    Web {
        /// Each configured relay with a listen token file or web access
        /// on, in `[remote] relays` order.
        relays: Vec<WebRelay>,
        /// Why the daemon did not answer, when remote control is on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// One relay's web access, as `[remote] relays` sets it and the daemon
/// reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebRelay {
    // keep-sorted start
    /// Whether `[remote] relays` turns web access on through it.
    pub enabled: bool,
    /// The relay's address, as the daemon names it.
    pub endpoint: String,
    /// What `[remote] relays` calls it, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the daemon reports, while it runs and serves web access there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<WebStatus>,
    /// Whether it has a listen token file, which web access needs.
    pub token_file: bool,
    // keep-sorted end
}

#[cfg(test)]
mod tests;
