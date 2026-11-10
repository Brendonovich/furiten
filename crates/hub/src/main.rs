//! `furiten-hub`: serve one Furiten library to paired devices over Iroh.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use furiten_core::hub::Net;
use furiten_core::{Error, Result, now_ms};
use furiten_hub::data::Device;
use furiten_hub::{Config, DeviceInfo, Hub, PairTicket, Status, control};
use qrcode::render::unicode::Dense1x2;
use qrcode::{EcLevel, QrCode};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "furiten-hub",
    version,
    about = "Furiten's self-hosted hub: one library, synced to paired devices over Iroh."
)]
struct Cli {
    /// The hub's data directory [default: $XDG_DATA_HOME/furiten-hub, or ~/Library/Application Support/furiten-hub]
    #[arg(long, global = true)]
    data: Option<PathBuf>,
    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the hub in the foreground.
    Serve {
        /// Use this self-hosted iroh-relay instead of n0's public relays (also FURITEN_RELAY). FURITEN_NET=local
        /// serves on loopback only, with no relay, for trying it out on one machine.
        #[arg(long)]
        relay: Option<String>,
    },
    /// Make a one-time pairing ticket (valid 10 minutes) for `furiten sync login --hub <ticket>`.
    Pair,
    /// List paired devices, or remove one.
    Devices {
        #[command(subcommand)]
        command: Option<DevicesCommand>,
    },
    /// Show the hub's endpoint id, library, network and devices.
    Status,
}

#[derive(Subcommand)]
enum DevicesCommand {
    /// List paired devices (the default).
    List,
    /// Remove a device by id, id prefix or name, closing its connections. It keeps what it already has.
    Rm { device: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                println!("{}", json!({ "error": error }));
            } else {
                eprintln!("furiten-hub: {}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let data = cli.data.unwrap_or_else(furiten_hub::data::default_data_dir);
    let json = cli.json;
    match cli.command {
        Command::Serve { relay } => {
            let net = match relay {
                Some(url) => Net::Relay(url),
                None => Net::from_env(),
            };
            serve(Config::new(&data, net), json)
        }
        Command::Pair => {
            let ticket: PairTicket = running(&data, "pair", json!({}))?;
            emit(json, &ticket, |t| {
                let mut out = String::new();
                if std::io::stdout().is_terminal()
                    && let Ok(code) = QrCode::with_error_correction_level(t.ticket.to_uppercase(), EcLevel::L)
                {
                    out.push_str(&code.render::<Dense1x2>().quiet_zone(true).build());
                    out.push('\n');
                }
                let minutes = ((t.expires_at - now_ms()) as f64 / 60_000.0).round();
                out.push_str(&format!(
                    "{}\n\nOn the device to pair (valid {minutes} minutes, once):\n  furiten sync login --hub {}",
                    t.ticket, t.ticket
                ));
                out
            })
        }
        Command::Devices { command } => match command.unwrap_or(DevicesCommand::List) {
            DevicesCommand::List => {
                let devices: Vec<DeviceInfo> = match control::call(&data, "devices.list", json!({}))? {
                    Some(value) => from_value(value)?,
                    None => furiten_hub::offline_status(&data)?.1,
                };
                emit(json, &devices, |d| devices_text(d))
            }
            DevicesCommand::Rm { device } => {
                let removed: Device = match control::call(&data, "devices.rm", json!({ "device": device }))? {
                    Some(value) => from_value(value)?,
                    None => furiten_hub::offline_remove_device(&data, &device)?,
                };
                emit(json, &removed, |d| format!("Removed {} ({})", d.name, d.id))
            }
        },
        Command::Status => {
            let (status, devices): (Status, Vec<DeviceInfo>) = match control::call(&data, "status", json!({}))? {
                Some(value) => {
                    let devices = control::call(&data, "devices.list", json!({}))?.unwrap_or(Value::Null);
                    (from_value(value)?, from_value(devices)?)
                }
                None => furiten_hub::offline_status(&data)?,
            };
            emit(json, &status, |s| status_text(s, &devices))
        }
    }
}

fn serve(config: Config, json: bool) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async {
        let hub = Hub::start(config).await?;
        let status = hub.status()?;
        emit(json, &status, |s| {
            format!(
                "furiten-hub {} serving library {}\n  endpoint {}\n  network  {}\n  data     {}\n  devices  {} paired\nPair a \
                 device with: furiten-hub pair",
                s.version, s.library_id, s.endpoint_id, s.net, s.data, s.devices
            )
        })?;
        wait_for_signal().await;
        eprintln!("furiten-hub: stopping");
        hub.shutdown().await;
        Ok(())
    })
}

async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

/// Calls the running hub, or explains that it has to be running.
fn running<T: serde::de::DeserializeOwned>(data: &std::path::Path, method: &str, params: Value) -> Result<T> {
    match control::call(data, method, params)? {
        Some(value) => from_value(value),
        None => Err(Error::new(
            "not_running",
            format!("no hub is running for {}; start it with furiten-hub serve", data.display()),
        )),
    }
}

fn from_value<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::internal(format!("bad answer from the hub: {e}")))
}

fn emit<T: Serialize>(json: bool, value: &T, human: impl FnOnce(&T) -> String) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value).map_err(Error::internal)?);
    } else {
        println!("{}", human(value));
    }
    Ok(())
}

fn status_text(s: &Status, devices: &[DeviceInfo]) -> String {
    let state = if s.running { "running" } else { "not running" };
    let mut out = format!(
        "furiten-hub {} ({state})\n  endpoint {}\n  library  {}\n  data     {}\n  cursor   {}",
        s.version, s.endpoint_id, s.library_id, s.data, s.cursor
    );
    if s.running {
        out.push_str(&format!("\n  network  {}", s.net));
        if let Some(relay) = s.addr.as_ref().and_then(|a| a.relay_url.as_ref()) {
            out.push_str(&format!("\n  relay    {relay}"));
        }
    }
    out.push_str(&format!("\n  devices  {} paired, {} connected", s.devices, s.connected));
    if !devices.is_empty() {
        out.push('\n');
        out.push_str(&devices_text(devices));
    }
    out
}

fn devices_text(devices: &[DeviceInfo]) -> String {
    if devices.is_empty() {
        return "No devices. Pair one with: furiten-hub pair".to_string();
    }
    let width = devices.iter().map(|d| d.device.name.chars().count()).max().unwrap_or(0);
    devices
        .iter()
        .map(|d| {
            let state = if d.connected { "connected" } else { "offline" };
            format!("  {:width$}  {}  {state}", d.device.name, d.device.id)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
