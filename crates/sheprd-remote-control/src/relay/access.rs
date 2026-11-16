//! What each configured relay asks of this computer: a private relay's
//! listen token, read from its file at start, and whether browsers reach
//! this computer through it (`docs/relay-web-plan.md`).

use super::{Error, Result};
use crate::server::Relay;
use std::{collections::BTreeMap, fmt, fs, path::Path};

/// The longest listen token the relay protocol carries.
const MAX_TOKEN: usize = 255;

/// A listen token, kept out of logs.
#[derive(Clone, Default, Eq, PartialEq)]
pub(crate) struct Token(Vec<u8>);

impl Token {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if self.0.is_empty() {
            "Token(none)"
        } else {
            "Token(…)"
        })
    }
}

/// One relay's access.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Access {
    pub(crate) token: Token,
    pub(crate) web: bool,
}

/// Each configured relay's access, by endpoint as the daemon names it,
/// its token files read. Blocking.
pub(crate) fn load(relays: &[Relay]) -> Result<BTreeMap<String, Access>> {
    relays
        .iter()
        .map(|relay| {
            let token = match &relay.token_file {
                Some(path) => read(path)?,
                None => Token::default(),
            };
            let access = Access {
                token,
                web: relay.web,
            };
            Ok((relay.endpoint.to_string(), access))
        })
        .collect()
}

/// A token file's one token, trimmed: not empty, and short enough to send.
fn read(path: &Path) -> Result<Token> {
    let text = fs::read_to_string(path).map_err(|source| Error::TokenFile {
        path: path.to_owned(),
        source,
    })?;
    let token = text.trim();
    if token.is_empty() || token.len() > MAX_TOKEN {
        return Err(Error::Token {
            path: path.to_owned(),
        });
    }
    Ok(Token(token.as_bytes().to_vec()))
}
