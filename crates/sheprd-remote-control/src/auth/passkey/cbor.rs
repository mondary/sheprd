//! The little CBOR (RFC 8949) WebAuthn needs: an attestation object and a
//! COSE key. Definite lengths only, as WebAuthn's CTAP2 canonical form
//! uses; nesting and every length are bounded by the input, so a hostile
//! object cannot make it allocate or recurse deeply.

use super::Error;

/// How deep skipped arrays and maps may nest.
const MAX_DEPTH: u8 = 8;

/// One item's head: its major type and its argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Head {
    Unsigned(u64),
    /// `-1 - n`.
    Negative(u64),
    Bytes(u64),
    Text(u64),
    Array(u64),
    Map(u64),
    Tag(u64),
    /// Simple values and floats, whose argument is already read.
    Simple,
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// What is left after the items read so far.
    pub(super) fn rest(&self) -> &'a [u8] {
        self.bytes
    }

    fn take(&mut self, length: u64) -> Result<&'a [u8], Error> {
        let length = usize::try_from(length).map_err(|_| Error::Cbor)?;
        if length > self.bytes.len() {
            return Err(Error::Cbor);
        }
        let (taken, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(taken)
    }

    pub(super) fn head(&mut self) -> Result<Head, Error> {
        let first = *self.take(1)?.first().ok_or(Error::Cbor)?;
        let (major, info) = (first >> 5, first & 0x1f);
        let argument = match info {
            0..=23 => u64::from(info),
            24..=27 => {
                let width = 1u64 << (info - 24);
                self.take(width)?
                    .iter()
                    .fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
            }
            // Reserved, or an indefinite length.
            _ => return Err(Error::Cbor),
        };
        Ok(match major {
            0 => Head::Unsigned(argument),
            1 => Head::Negative(argument),
            2 => Head::Bytes(argument),
            3 => Head::Text(argument),
            4 => Head::Array(argument),
            5 => Head::Map(argument),
            6 => Head::Tag(argument),
            _ => Head::Simple,
        })
    }

    pub(super) fn bytes(&mut self) -> Result<&'a [u8], Error> {
        match self.head()? {
            Head::Bytes(length) => self.take(length),
            _ => Err(Error::Cbor),
        }
    }

    pub(super) fn text(&mut self) -> Result<&'a str, Error> {
        match self.head()? {
            Head::Text(length) => std::str::from_utf8(self.take(length)?).map_err(|_| Error::Cbor),
            _ => Err(Error::Cbor),
        }
    }

    /// A map's entry count.
    pub(super) fn map(&mut self) -> Result<u64, Error> {
        match self.head()? {
            Head::Map(entries) => Ok(entries),
            _ => Err(Error::Cbor),
        }
    }

    /// An integer key or value, as `i128` so every CBOR integer fits.
    pub(super) fn integer(&mut self) -> Result<i128, Error> {
        match self.head()? {
            Head::Unsigned(value) => Ok(i128::from(value)),
            Head::Negative(value) => Ok(-1 - i128::from(value)),
            _ => Err(Error::Cbor),
        }
    }

    /// Reads past one whole item.
    pub(super) fn skip(&mut self) -> Result<(), Error> {
        self.skip_at(0)
    }

    fn skip_at(&mut self, depth: u8) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Cbor);
        }
        match self.head()? {
            Head::Unsigned(_) | Head::Negative(_) | Head::Simple => Ok(()),
            Head::Bytes(length) | Head::Text(length) => self.take(length).map(drop),
            Head::Tag(_) => self.skip_at(depth + 1),
            Head::Array(items) => {
                for _ in 0..items {
                    self.skip_at(depth + 1)?;
                }
                Ok(())
            }
            Head::Map(entries) => {
                for _ in 0..entries {
                    self.skip_at(depth + 1)?;
                    self.skip_at(depth + 1)?;
                }
                Ok(())
            }
        }
    }
}
