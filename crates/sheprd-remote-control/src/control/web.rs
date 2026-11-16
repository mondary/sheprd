//! Web access through a relay, as `sheprd remote status` and `sheprd
//! remote web status` show it (`docs/relay-web-plan.md`): the address
//! browsers open, the certificate served there, and what the Certificate
//! Transparency watch last found.

use serde::{Deserialize, Serialize};

/// Web access through one relay that `[remote] relays` turns it on for.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WebStatus {
    /// `https://<label>.<relay host>`, once the relay's address gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// When the certificate served there expires, in milliseconds since
    /// the Unix epoch; none until one is good.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_ms: Option<u64>,
    /// Why the last attempt to get one failed, while none is good.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub transparency: Transparency,
}

/// What the Certificate Transparency watch found for the web name.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Transparency {
    /// Not asked yet.
    #[default]
    Unchecked,
    /// Every logged certificate for the name has this computer's key.
    Clean { checked_ms: u64 },
    /// The last query failed, which is never taken as clean.
    Failed { error: String, checked_ms: u64 },
    /// A certificate for the name has a key that is not this computer's:
    /// web access is off on this relay until the user turns it on again.
    Alert {
        /// The certificate's SHA-256, hex, to look it up in CT logs.
        certificate: String,
        found_ms: u64,
    },
}
