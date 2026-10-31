//! Frames and messages of `recall/sync/1` and `recall/pair/1`: a big-endian `u32` length, then that many bytes of
//! JSON.

use iroh::endpoint::{ConnectionError, VarInt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{Error, Result};

/// The largest sync request or response: a first sync carries the whole library.
pub const MAX_SYNC_FRAME: usize = 128 * 1024 * 1024;
/// The largest pairing message or event.
pub const MAX_SMALL_FRAME: usize = 16 * 1024;

/// Close codes the hub uses, so a device can tell why it was cut off.
pub mod close {
    use super::VarInt;

    pub const DONE: VarInt = VarInt::from_u32(0);
    pub const PROTOCOL: VarInt = VarInt::from_u32(400);
    /// The device isn't in the hub's device list (never paired, or removed).
    pub const NOT_PAIRED: VarInt = VarInt::from_u32(403);
    pub const BUSY: VarInt = VarInt::from_u32(503);
}

/// What a device sends on `recall/pair/1`: the ticket's secret (32 hex characters) and a name for the device list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequest {
    pub secret: String,
    pub name: String,
}

/// The hub's answer to a pairing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paired {
    pub library_id: String,
}

/// `{ "event": "changed", "cursor": n }`, sent by the hub after another device's write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    pub cursor: i64,
}

impl Event {
    pub fn changed(cursor: i64) -> Event {
        Event { event: "changed".to_string(), cursor }
    }
}

/// A failed request on either protocol is answered with `{ "error": { code, message } }`.
#[derive(Serialize)]
pub struct Failure<'a> {
    pub error: &'a Error,
}

pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, value: &impl Serialize) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    let len = u32::try_from(body.len()).map_err(|_| Error::invalid("frame too large"))?;
    w.write_all(&len.to_be_bytes()).await.map_err(network)?;
    w.write_all(&body).await.map_err(network)?;
    Ok(())
}

/// Reads one frame of at most `max` bytes. `Ok(None)` when the stream ended cleanly before a frame.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R, max: usize) -> Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(network(e)),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(Error::new("too_large", format!("a {len}-byte frame is over the {max}-byte limit")));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await.map_err(network)?;
    Ok(Some(body))
}

/// Reads one frame and parses it as `T`.
pub async fn read_message<R: AsyncRead + Unpin, T: DeserializeOwned>(r: &mut R, max: usize) -> Result<T> {
    let body = read_frame(r, max).await?.ok_or_else(|| Error::new("network", "the stream ended early"))?;
    serde_json::from_slice(&body).map_err(|e| Error::new("bad_request", e.to_string()))
}

/// Reads an answer: `T`, or the error the other side sent.
pub async fn read_reply<R: AsyncRead + Unpin, T: DeserializeOwned>(r: &mut R, max: usize) -> Result<T> {
    let mut value: Value = read_message(r, max).await?;
    if let Some(error) = value.get_mut("error") {
        return Err(serde_json::from_value(error.take()).map_err(|e| Error::new("bad_response", e.to_string()))?);
    }
    serde_json::from_value(value).map_err(|e| Error::new("bad_response", e.to_string()))
}

fn network(e: impl std::fmt::Display) -> Error {
    Error::new("network", e.to_string())
}

/// Why a connection to a hub closed, as an [`Error`]: `unauthorized` when the hub doesn't know this device.
pub fn closed_error(reason: &ConnectionError) -> Error {
    match reason {
        ConnectionError::ApplicationClosed(close) if close.error_code == close::NOT_PAIRED => Error::new(
            "unauthorized",
            "the hub doesn't know this device (never paired, or removed); pair it again with a new ticket",
        ),
        ConnectionError::ApplicationClosed(close) if close.error_code == close::BUSY => {
            Error::new("busy", "the hub has too many connections; try again soon")
        }
        other => Error::new("network", format!("lost the hub: {other}")),
    }
}
