//! The PROXY protocol v2 header the front puts ahead of a connection it
//! forwards, so the server behind sees the caller's address, for its logs
//! and for the relay's per-address limits and bans behind it.

use std::net::{IpAddr, SocketAddr};

/// Every v2 header starts with these twelve bytes.
const SIGNATURE: [u8; 12] = *b"\r\n\r\n\0\r\nQUIT\n";
/// Version 2, command PROXY.
const PROXY: u8 = 0x21;
const TCP_IPV4: u8 = 0x11;
const TCP_IPV6: u8 = 0x21;

/// The header for a connection from `source` that arrived at
/// `destination`. When their families differ, both are written as IPv6,
/// an IPv4 address in its mapped form.
#[must_use]
pub fn header(source: SocketAddr, destination: SocketAddr) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16 + 36);
    bytes.extend_from_slice(&SIGNATURE);
    bytes.push(PROXY);
    match (source.ip().to_canonical(), destination.ip().to_canonical()) {
        (IpAddr::V4(from), IpAddr::V4(to)) => {
            bytes.push(TCP_IPV4);
            bytes.extend_from_slice(&12u16.to_be_bytes());
            bytes.extend_from_slice(&from.octets());
            bytes.extend_from_slice(&to.octets());
        }
        (from, to) => {
            let v6 = |address: IpAddr| match address {
                IpAddr::V4(address) => address.to_ipv6_mapped(),
                IpAddr::V6(address) => address,
            };
            bytes.push(TCP_IPV6);
            bytes.extend_from_slice(&36u16.to_be_bytes());
            bytes.extend_from_slice(&v6(from).octets());
            bytes.extend_from_slice(&v6(to).octets());
        }
    }
    bytes.extend_from_slice(&source.port().to_be_bytes());
    bytes.extend_from_slice(&destination.port().to_be_bytes());
    bytes
}
