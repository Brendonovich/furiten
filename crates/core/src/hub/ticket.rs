//! Pairing tickets: `rchub1` then lowercase RFC 4648 base32 (no padding) of
//!
//! ```text
//! endpoint id (32 bytes) | secret (16 bytes) | relay URL length (u8) | relay URL (UTF-8)
//! | direct address count (u8) | per address: 4 or 6 (u8), IP (4 or 16 bytes), port (u16 BE)
//! ```

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use super::{hex, unhex};
use crate::store::HubAddr;
use crate::{Error, Result};

const PREFIX: &str = "rchub1";
const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
/// More would only make the ticket and its QR code bigger; the relay covers the rest.
const MAX_DIRECT_ADDRS: usize = 8;

/// What a device needs to pair with a hub: how to reach it and a one-time secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Ticket {
    pub addr: HubAddr,
    pub secret: [u8; 16],
}

impl Ticket {
    /// The secret as sent in a pairing request.
    pub fn secret_hex(&self) -> String {
        hex(&self.secret)
    }

    /// The secret from a pairing request, if it's 32 hex characters.
    pub fn parse_secret(text: &str) -> Option<[u8; 16]> {
        unhex(text)?.try_into().ok()
    }

    fn to_bytes(&self) -> Result<Vec<u8>> {
        let id = unhex(&self.addr.id)
            .filter(|b| b.len() == 32)
            .ok_or_else(|| Error::invalid(format!("{} isn't an endpoint id", self.addr.id)))?;
        let mut out = id;
        out.extend_from_slice(&self.secret);
        let relay = self.addr.relay_url.as_deref().unwrap_or("");
        let relay_len = u8::try_from(relay.len()).map_err(|_| Error::invalid("relay URL too long"))?;
        out.push(relay_len);
        out.extend_from_slice(relay.as_bytes());
        let addrs: Vec<&SocketAddr> = self.addr.direct_addrs.iter().take(MAX_DIRECT_ADDRS).collect();
        out.push(addrs.len() as u8);
        for addr in addrs {
            match addr.ip() {
                IpAddr::V4(ip) => {
                    out.push(4);
                    out.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    out.push(6);
                    out.extend_from_slice(&ip.octets());
                }
            }
            out.extend_from_slice(&addr.port().to_be_bytes());
        }
        Ok(out)
    }

    fn from_bytes(bytes: &[u8]) -> Option<Ticket> {
        let mut r = Reader(bytes);
        let id = hex(r.take(32)?);
        let secret = r.take(16)?.try_into().ok()?;
        let relay_len = r.take(1)?[0] as usize;
        let relay = std::str::from_utf8(r.take(relay_len)?).ok()?;
        let count = r.take(1)?[0];
        let mut direct_addrs = Vec::new();
        for _ in 0..count {
            let ip = match r.take(1)?[0] {
                4 => IpAddr::from(<[u8; 4]>::try_from(r.take(4)?).ok()?),
                6 => IpAddr::from(<[u8; 16]>::try_from(r.take(16)?).ok()?),
                _ => return None,
            };
            let port = u16::from_be_bytes(r.take(2)?.try_into().ok()?);
            direct_addrs.push(SocketAddr::new(ip, port));
        }
        if !r.0.is_empty() {
            return None;
        }
        let relay_url = (!relay.is_empty()).then(|| relay.to_string());
        Some(Ticket { addr: HubAddr { id, relay_url, direct_addrs }, secret })
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.to_bytes().map_err(|_| fmt::Error)?;
        write!(f, "{PREFIX}{}", base32(&bytes))
    }
}

/// Never prints the secret.
impl fmt::Debug for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ticket").field("addr", &self.addr).finish_non_exhaustive()
    }
}

impl FromStr for Ticket {
    type Err = Error;

    /// Case-insensitive, so a ticket scanned from an uppercase QR code works too.
    fn from_str(text: &str) -> Result<Ticket> {
        let text = text.trim().to_ascii_lowercase();
        let bad = || Error::new("bad_ticket", "that isn't a hub ticket (they start with rchub1)");
        let body = text.strip_prefix(PREFIX).ok_or_else(bad)?;
        unbase32(body).as_deref().and_then(Ticket::from_bytes).ok_or_else(bad)
    }
}

fn base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let (mut buffer, mut bits) = (0u32, 0);
    for &byte in bytes {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn unbase32(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let (mut buffer, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let value = ALPHABET.iter().position(|&a| a == c)? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    // Fewer than 5 bits may be left over, all zero.
    (bits < 5 && buffer & ((1 << bits) - 1) == 0).then_some(out)
}

#[cfg(test)]
pub(super) fn roundtrip_base32(bytes: &[u8]) -> Option<Vec<u8>> {
    unbase32(&base32(bytes))
}
