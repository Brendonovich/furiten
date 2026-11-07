//! The control socket, `hub.sock` in the data directory (mode 0600): how `recall-hub pair`, `status` and
//! `devices` reach a running hub. One JSON line in (`{ method, params }`), one out (`{ result }` or
//! `{ error: { code, message } }`).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use recall_core::hub::host::Host;
use recall_core::{Error, Result, Store};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

use crate::data;

#[derive(Deserialize)]
struct Request {
    method: String,
    #[serde(default)]
    params: Value,
}

/// Binds the socket (replacing a stale one; the caller holds the data directory's lock) and serves it.
pub(crate) async fn serve(data: &Path, host: Arc<Host>, store: Arc<Mutex<Store>>) -> Result<JoinHandle<()>> {
    let path = data.join(data::SOCKET);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    let data = data.to_path_buf();
    Ok(tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let (host, store, data) = (host.clone(), store.clone(), data.clone());
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                if tokio::io::BufReader::new(read).take(64 * 1024).read_line(&mut line).await.is_err() {
                    return;
                }
                let reply = match serde_json::from_str::<Request>(&line) {
                    Ok(request) => match dispatch(&host, &store, &data, &request.method, request.params).await {
                        Ok(result) => json!({ "result": result }),
                        Err(error) => json!({ "error": error }),
                    },
                    Err(e) => json!({ "error": Error::new("bad_request", e.to_string()) }),
                };
                let _ = write.write_all(format!("{reply}\n").as_bytes()).await;
            });
        }
    }))
}

fn value(result: Result<impl serde::Serialize>) -> Result<Value> {
    result.and_then(|v| serde_json::to_value(v).map_err(Error::internal))
}

async fn dispatch(host: &Host, store: &Mutex<Store>, data: &Path, method: &str, params: Value) -> Result<Value> {
    match method {
        "status" => value(crate::status(host, store, data)),
        "pair" => value(host.pair().await),
        "devices.list" => value(Ok(host.devices())),
        "devices.rm" => {
            let device =
                params.get("device").and_then(Value::as_str).ok_or_else(|| Error::invalid("device is required"))?;
            value(host.remove_device(device))
        }
        _ => Err(Error::new("unknown_method", format!("unknown method {method}"))),
    }
}

/// Calls the running hub for `data`. `Ok(None)` when no hub is running there.
pub fn call(data: &Path, method: &str, params: Value) -> Result<Option<Value>> {
    let Ok(mut stream) = std::os::unix::net::UnixStream::connect(data.join(data::SOCKET)) else {
        return Ok(None);
    };
    let line = json!({ "method": method, "params": params }).to_string() + "\n";
    stream.write_all(line.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    let mut reply: Value =
        serde_json::from_str(&reply).map_err(|e| Error::internal(format!("bad answer from the hub: {e}")))?;
    if let Some(error) = reply.get_mut("error") {
        return Err(serde_json::from_value(error.take()).map_err(Error::internal)?);
    }
    Ok(Some(reply.get_mut("result").map(Value::take).unwrap_or(Value::Null)))
}
