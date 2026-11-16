//! Reading the server name and ALPN from a TLS ClientHello without ending
//! TLS: the front routes on them and passes every byte on unchanged, so
//! the relay never holds a key for what it carries.
//!
//! Strict on purpose. A ClientHello that bends the rules (a record that is
//! not a handshake, a zero-length fragment, trailing bytes, a repeated
//! extension, a second host name, a name that is not a DNS host name) is
//! refused rather than guessed at: browsers and TLS libraries never send
//! one, and whatever the front misreads, the computer would read
//! differently.

use super::Error;
use crate::wire::Parse;
use std::collections::HashSet;

/// The longest ClientHello the front reads, in handshake bytes. Real ones
/// are under 2 KiB; with a post-quantum key share, under 4 KiB.
pub const MAX_HELLO: usize = 16 * 1024;
/// The most bytes the front reads to find a ClientHello, records and all:
/// [`MAX_HELLO`] split into the smallest records a peer may send.
pub const MAX_READ: usize = MAX_HELLO + RECORD_HEADER * MAX_HELLO.div_ceil(MIN_USEFUL_RECORD);

const RECORD_HEADER: usize = 5;
/// The longest record TLS allows.
const MAX_RECORD: usize = 1 << 14;
/// The fewest bytes counted per record when bounding the read: a peer
/// that splits a hello into tinier records is refused by [`MAX_READ`].
const MIN_USEFUL_RECORD: usize = 64;
const HANDSHAKE: u8 = 22;
const CLIENT_HELLO: u8 = 1;
const SERVER_NAME: u16 = 0;
const ALPN: u16 = 16;
const HOST_NAME: u8 = 0;
/// The longest DNS name.
const MAX_NAME: usize = 253;

/// What the front needs from a ClientHello.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClientHello {
    /// The protocols offered, in the client's order.
    pub alpn: Vec<Vec<u8>>,
    /// The host name asked for, lowercased, when there is one.
    pub server_name: Option<String>,
}

impl ClientHello {
    /// Whether the client offers `protocol`.
    #[must_use]
    pub fn offers(&self, protocol: &[u8]) -> bool {
        self.alpn.iter().any(|offered| offered == protocol)
    }
}

/// Reads a ClientHello from the start of a connection, or says how many
/// bytes it needs in all. `Done` carries the bytes its records took. Never
/// asks for more than [`MAX_READ`].
pub fn read(bytes: &[u8]) -> Result<Parse<ClientHello>, Error> {
    let mut handshake = Vec::new();
    let mut offset = 0;
    loop {
        let Some(header) = bytes.get(offset..offset + RECORD_HEADER) else {
            return need(offset + RECORD_HEADER);
        };
        if header[0] != HANDSHAKE {
            return Err(Error::NotHandshake);
        }
        // Record versions are 3.x; the ClientHello says what it means.
        if header[1] != 3 {
            return Err(Error::Version);
        }
        let length = usize::from(u16::from_be_bytes([header[3], header[4]]));
        // Zero-length handshake fragments are forbidden (RFC 8446, 5.1).
        if length == 0 || length > MAX_RECORD {
            return Err(Error::Record);
        }
        let end = offset + RECORD_HEADER + length;
        let Some(fragment) = bytes.get(offset + RECORD_HEADER..end) else {
            return need(end);
        };
        handshake.extend_from_slice(fragment);
        offset = end;
        let Some(message) = handshake.get(..4) else {
            continue;
        };
        if message[0] != CLIENT_HELLO {
            return Err(Error::NotClientHello);
        }
        let body =
            usize::from(message[1]) << 16 | usize::from(message[2]) << 8 | usize::from(message[3]);
        if body > MAX_HELLO {
            return Err(Error::TooLong);
        }
        match handshake.len().cmp(&(4 + body)) {
            std::cmp::Ordering::Less => {
                if offset >= MAX_READ {
                    return Err(Error::TooLong);
                }
            }
            // Nothing may follow the ClientHello before the server answers.
            std::cmp::Ordering::Greater => return Err(Error::Trailing),
            std::cmp::Ordering::Equal => {
                return Ok(Parse::Done(parse(&handshake[4..])?, offset));
            }
        }
    }
}

fn need(total: usize) -> Result<Parse<ClientHello>, Error> {
    if total > MAX_READ {
        return Err(Error::TooLong);
    }
    Ok(Parse::Need(total))
}

/// The ClientHello's body, after its four-byte handshake header.
fn parse(body: &[u8]) -> Result<ClientHello, Error> {
    let mut reader = Reader(body);
    let [major, _] = reader.take::<2>()?;
    if major != 3 {
        return Err(Error::Version);
    }
    reader.take::<32>()?;
    let session = reader.vec8()?;
    if session.len() > 32 {
        return Err(Error::Malformed);
    }
    let suites = reader.vec16()?;
    if suites.is_empty() || suites.len() % 2 != 0 {
        return Err(Error::Malformed);
    }
    if reader.vec8()?.is_empty() {
        return Err(Error::Malformed);
    }
    let mut hello = ClientHello::default();
    // TLS 1.2 allows a ClientHello with no extensions; it names no host.
    if reader.0.is_empty() {
        return Ok(hello);
    }
    let mut extensions = Reader(reader.vec16()?);
    reader.end()?;
    let mut seen = HashSet::new();
    while !extensions.0.is_empty() {
        let kind = u16::from_be_bytes(extensions.take::<2>()?);
        let data = extensions.vec16()?;
        if !seen.insert(kind) {
            return Err(Error::Repeated);
        }
        match kind {
            SERVER_NAME => hello.server_name = server_name(data)?,
            ALPN => hello.alpn = alpn(data)?,
            _ => {}
        }
    }
    Ok(hello)
}

/// The host name in a `server_name` extension (RFC 6066, 3).
fn server_name(data: &[u8]) -> Result<Option<String>, Error> {
    let mut outer = Reader(data);
    let mut list = Reader(outer.vec16()?);
    outer.end()?;
    let mut name = None;
    while !list.0.is_empty() {
        let [kind] = list.take::<1>()?;
        let value = list.vec16()?;
        if kind != HOST_NAME {
            continue;
        }
        if name.is_some() {
            return Err(Error::Repeated);
        }
        name = Some(host_name(value)?);
    }
    Ok(name)
}

/// A DNS host name, lowercased: letters, digits, and hyphens in labels of
/// 1 to 63, no trailing dot.
fn host_name(bytes: &[u8]) -> Result<String, Error> {
    if bytes.is_empty() || bytes.len() > MAX_NAME {
        return Err(Error::ServerName);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::ServerName)?;
    let valid = text.split('.').all(|label| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    if !valid {
        return Err(Error::ServerName);
    }
    Ok(text.to_ascii_lowercase())
}

/// The protocols in an ALPN extension (RFC 7301, 3.1).
fn alpn(data: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let mut outer = Reader(data);
    let mut list = Reader(outer.vec16()?);
    outer.end()?;
    let mut protocols = Vec::new();
    while !list.0.is_empty() {
        let protocol = list.vec8()?;
        if protocol.is_empty() {
            return Err(Error::Malformed);
        }
        protocols.push(protocol.to_vec());
    }
    if protocols.is_empty() {
        return Err(Error::Malformed);
    }
    Ok(protocols)
}

/// Reads fields off the front of a slice; running short is malformed.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn bytes(&mut self, count: usize) -> Result<&'a [u8], Error> {
        if self.0.len() < count {
            return Err(Error::Malformed);
        }
        let (taken, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(taken)
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut fixed = [0; N];
        fixed.copy_from_slice(self.bytes(N)?);
        Ok(fixed)
    }

    /// A field with a one-byte length.
    fn vec8(&mut self) -> Result<&'a [u8], Error> {
        let [length] = self.take::<1>()?;
        self.bytes(usize::from(length))
    }

    /// A field with a two-byte length.
    fn vec16(&mut self) -> Result<&'a [u8], Error> {
        let length = u16::from_be_bytes(self.take::<2>()?);
        self.bytes(usize::from(length))
    }

    /// Nothing may be left.
    fn end(&self) -> Result<(), Error> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::Trailing)
        }
    }
}
