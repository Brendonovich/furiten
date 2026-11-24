//! HTTPS through ureq (the system's proxy and trusted roots): the polled manifest with `If-None-Match`, and
//! resumable downloads checked against the manifest's size and SHA-256 before any byte is used. Requests carry
//! nothing about the install beyond `User-Agent: furiten/<channel>/<version>`. Ported from Hex's `net.rs`,
//! `net/download.rs` and `updater/transport.rs`, which the same release service proved out on.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use eyre::{Result, bail, eyre};
use sha2::{Digest, Sha256};
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use super::protocol::Target;
use super::proxy::proxy_for;

pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// What every request names itself to the release service: the app, the channel this build belongs to and its
/// version, e.g. `furiten/stable/0.1.0`.
pub fn user_agent(channel: &str, version: &str) -> String {
    format!("furiten/{channel}/{version}")
}

/// What a caller adds to [`agent`]'s settings.
type Config = ureq::config::ConfigBuilder<ureq::typestate::AgentScope>;

/// An agent for requests to `url`: platform TLS and roots, and the proxy the computer uses for it.
fn agent(url: &str, configure: impl FnOnce(Config) -> Config) -> ureq::Agent {
    configure(base().proxy(proxy_for(url))).build().into()
}

fn base() -> Config {
    ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build(),
        )
        .timeout_connect(Some(Duration::from_secs(15)))
}

/// The host of `url`, for messages and logs (never the path or query).
fn host(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("the server")
        .to_owned()
}

/// Why a request failed, in a person's words, without the URL.
fn reason(error: &ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(407) => "the proxy needs a sign-in Furiten can't give".into(),
        ureq::Error::StatusCode(code) => format!("the server answered HTTP {code}"),
        ureq::Error::HostNotFound => "the name couldn't be looked up (DNS)".into(),
        ureq::Error::ConnectProxyFailed(detail) => format!("the proxy refused ({detail})"),
        ureq::Error::Tls(detail) => tls(detail),
        ureq::Error::NativeTls(detail) => tls(detail),
        // Security.framework can also report a rejected certificate as an I/O error naming it.
        ureq::Error::Io(detail) if looks_like_tls(&detail.to_string()) => tls(detail),
        ureq::Error::Timeout(_) => "the connection timed out".into(),
        ureq::Error::ConnectionFailed => "the connection failed".into(),
        ureq::Error::Io(detail) => format!("the connection failed ({detail})"),
        error => error.to_string(),
    }
}

/// A certificate or TLS failure: on a managed network, usually a firewall inspecting TLS whose root certificate
/// this computer doesn't trust.
fn tls(detail: impl std::fmt::Display) -> String {
    format!("the certificate isn't trusted or the secure connection failed ({detail})")
}

fn looks_like_tls(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    ["certificate", "tls", "ssl", "handshake", "trust", "x509", "schannel"].iter().any(|word| detail.contains(word))
}

/// "Couldn't reach `host`: `reason`".
fn unreachable(url: &str, error: &ureq::Error) -> String {
    format!("Couldn't reach {}: {}", host(url), reason(error))
}

/// The last manifest the service sent, revalidated with its ETag.
#[derive(Default)]
pub struct ManifestCache {
    /// The manifest the ETag and body belong to; switching channels changes it.
    url: Option<String>,
    etag: Option<String>,
    body: Option<String>,
}

impl ManifestCache {
    /// Fetches `url`, returning the manifest text: the cached one when the service answers 304.
    pub fn fetch(&mut self, url: &str, user_agent: &str, max_time: Duration) -> Result<String> {
        if self.url.as_deref() != Some(url) {
            *self = Self { url: Some(url.to_owned()), ..Self::default() };
        }
        let agent = agent(url, |config| {
            config
                .https_only(true)
                .max_redirects(10)
                .http_status_as_error(false)
                .timeout_global(Some(max_time.max(Duration::from_secs(1))))
        });
        let mut request = agent.get(url).header("User-Agent", user_agent);
        if let (Some(etag), Some(_)) = (&self.etag, &self.body) {
            request = request.header("If-None-Match", etag.as_str());
        }
        let mut response = request.call().map_err(|error| eyre!("{}", unreachable(url, &error)))?;
        match response.status().as_u16() {
            304 => self.body.clone().ok_or_else(|| eyre!("the update service sent 304 for nothing cached")),
            200 => {
                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|value| value.to_str().ok())
                    .map(|value| value.trim().to_owned())
                    .filter(|etag| valid_etag(etag));
                let mut text = String::new();
                response
                    .body_mut()
                    .with_config()
                    .limit(MAX_MANIFEST_BYTES + 1)
                    .reader()
                    .take(MAX_MANIFEST_BYTES + 1)
                    .read_to_string(&mut text)?;
                if text.len() as u64 > MAX_MANIFEST_BYTES {
                    bail!("the update manifest is too large");
                }
                self.etag = etag;
                self.body = Some(text.clone());
                Ok(text)
            }
            status => bail!("the update manifest request failed with HTTP {status}"),
        }
    }
}

fn valid_etag(etag: &str) -> bool {
    !etag.is_empty() && !etag.chars().any(char::is_control)
}

/// A file's SHA-256, as the manifest's `sha256` field writes it.
fn sha256_hex(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Downloads `target` from `url` into `directory`, resuming an earlier partial download of the same bytes, and
/// returns the verified file. The file is named after the target URL's last segment.
pub fn download(url: &str, user_agent: &str, target: &Target, directory: &Path) -> Result<PathBuf> {
    let name = url
        .rsplit('/')
        .next()
        .filter(|name| valid_file_name(name))
        .ok_or_else(|| eyre!("the update file name is not usable"))?;
    let folder = directory.join(&target.sha256);
    let file = folder.join(name);
    if file.is_file() && verify(&file, target).is_ok() {
        return Ok(file);
    }
    fs::create_dir_all(&folder)?;
    let partial = folder.join(format!("{name}.partial"));
    if fs::metadata(&partial).is_ok_and(|metadata| metadata.len() > target.size) {
        fs::remove_file(&partial)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    drop(options.open(&partial)?);
    let complete = fs::metadata(&partial)?.len() == target.size;
    if !complete {
        fetch(url, user_agent, Some(target.size), &partial)?;
    }
    if let Err(error) = verify(&partial, target) {
        // A mismatch at full size can never resume into the right bytes.
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    fs::rename(&partial, &file)?;
    Ok(file)
}

fn valid_file_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 200
        && bytes[0].is_ascii_alphanumeric()
        && bytes.iter().all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(byte))
}

/// Bytes asked for per request.
const PIECE: u64 = 8 << 20;
/// How long one piece may take: a slower link than about 27 KB/s fails.
const PIECE_TIME: Duration = Duration::from_secs(300);
const ATTEMPTS: u32 = 3;

/// Fetches `url` into `partial`, appending to what it already holds: one ranged request per piece, so a stalled
/// connection fails within a bounded time instead of hanging, and every piece resumes where the file ends.
fn fetch(url: &str, user_agent: &str, size: Option<u64>, partial: &Path) -> Result<()> {
    // The release service redirects latest links to the current file; later pieces go there directly.
    let mut location = url.to_owned();
    let mut failures = 0;
    loop {
        let have = fs::metadata(partial).map_or(0, |metadata| metadata.len());
        if size.is_some_and(|size| have >= size) {
            return Ok(());
        }
        match piece(&location, user_agent, size, partial, have) {
            Ok(Piece::More(final_url)) => {
                location = final_url;
                failures = 0;
            }
            Ok(Piece::Done) => return Ok(()),
            Err(error) => {
                failures += 1;
                eprintln!(
                    "furiten-desktop: an update download piece failed ({error:#}); attempt {failures} of {ATTEMPTS}"
                );
                if failures >= ATTEMPTS {
                    return Err(error);
                }
                location = url.to_owned();
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

enum Piece {
    /// More to fetch, from this URL.
    More(String),
    /// The server sent the whole rest of the file.
    Done,
}

fn piece(url: &str, user_agent: &str, size: Option<u64>, partial: &Path, have: u64) -> Result<Piece> {
    let end = size.map_or(have + PIECE, |size| size.min(have + PIECE)).saturating_sub(1);
    let agent = agent(url, |config| {
        config
            .https_only(true)
            .max_redirects(10)
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .timeout_recv_body(Some(PIECE_TIME))
    });
    let response =
        agent.get(url).header("User-Agent", user_agent).header("Range", format!("bytes={have}-{end}")).call();
    let mut response = match response {
        Ok(response) => response,
        // A range past the end: the file is already whole.
        Err(ureq::Error::StatusCode(416)) if have > 0 && size.is_none() => {
            return Ok(Piece::Done);
        }
        Err(error) => return Err(eyre!("{}", unreachable(url, &error))),
    };
    let final_url = ureq::ResponseExt::get_uri(&response).to_string();
    let status = response.status().as_u16();
    let mut file = match status {
        206 => {
            let start = response
                .headers()
                .get("content-range")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("bytes "))
                .and_then(|value| value.split_once('-'))
                .and_then(|(start, _)| start.parse::<u64>().ok());
            if start != Some(have) {
                bail!("the server sent a different part of the file than asked for");
            }
            OpenOptions::new().append(true).create(true).open(partial)?
        }
        // No ranges: the whole file, from the start.
        200 => OpenOptions::new().write(true).truncate(true).create(true).open(partial)?,
        status => bail!("{}", unreachable(url, &ureq::Error::StatusCode(status))),
    };
    let whole = status == 200;
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut buffer = vec![0; 64 * 1024];
    let mut written = if whole { 0 } else { have };
    loop {
        let read = reader.read(&mut buffer).map_err(|error| eyre!("{}", unreachable(url, &ureq::Error::Io(error))))?;
        if read == 0 {
            break;
        }
        if size.is_some_and(|size| written + read as u64 > size) {
            bail!("the server sent more than the file's size");
        }
        file.write_all(&buffer[..read])?;
        written += read as u64;
    }
    file.flush()?;
    if whole || size.is_some_and(|size| written >= size) {
        return Ok(Piece::Done);
    }
    // A short piece ends the file when its size isn't known.
    if size.is_none() && written < end + 1 {
        return Ok(Piece::Done);
    }
    Ok(Piece::More(final_url))
}

/// Checks a downloaded file against the manifest's size and SHA-256.
pub fn verify(path: &Path, target: &Target) -> Result<()> {
    let size = fs::metadata(path)?.len();
    if size != target.size {
        bail!("the update has {size} bytes, expected {}", target.size);
    }
    if sha256_hex(path)? != target.sha256 {
        bail!("the update's SHA-256 does not match the manifest");
    }
    Ok(())
}

/// Removes downloads other than `keep` (a target's SHA-256).
pub fn prune(directory: &Path, keep: Option<&str>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if keep != name.to_str()
            && name.len() == 64
            && entry.path().is_dir()
            && let Err(error) = fs::remove_dir_all(entry.path())
        {
            eprintln!(
                "furiten-desktop: could not remove an old update download at {}: {error}",
                entry.path().display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_name_the_app_channel_and_version() {
        assert_eq!(user_agent("stable", "0.1.0"), "furiten/stable/0.1.0");
    }

    #[test]
    fn etags_with_control_characters_are_dropped() {
        assert!(valid_etag("\"new\""));
        assert!(!valid_etag(""));
        assert!(!valid_etag("\"a\nb\""));
    }

    #[test]
    fn downloads_are_checked_against_size_and_sha256() {
        let directory = std::env::temp_dir().join(format!("furiten-update-verify-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("file");
        fs::write(&path, b"new").unwrap();
        let mut target = Target { url: "files/x/file".into(), size: 3, sha256: sha256_hex(&path).unwrap() };
        assert!(verify(&path, &target).is_ok());
        target.size = 4;
        assert!(verify(&path, &target).is_err());
        target.size = 3;
        target.sha256 = "a".repeat(64);
        assert!(verify(&path, &target).is_err());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn a_finished_download_is_reused_without_the_network() {
        let directory = std::env::temp_dir().join(format!("furiten-update-reuse-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let source = directory.join("source");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&source, b"bytes").unwrap();
        let sha256 = sha256_hex(&source).unwrap();
        let target = Target { url: format!("files/{sha256}/Furiten.app.zip"), size: 5, sha256: sha256.clone() };
        // A complete partial download needs no request.
        fs::create_dir_all(directory.join(&sha256)).unwrap();
        fs::copy(&source, directory.join(&sha256).join("Furiten.app.zip.partial")).unwrap();
        let file =
            download("https://invalid.invalid/Furiten.app.zip", "furiten/stable/0.1.0", &target, &directory).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"bytes");
        assert_eq!(
            download("https://invalid.invalid/Furiten.app.zip", "furiten/stable/0.1.0", &target, &directory).unwrap(),
            file
        );
        prune(&directory, None);
        assert!(!directory.join(&sha256).exists());
        assert!(
            !valid_file_name("../x") && !valid_file_name(".hidden") && valid_file_name("Furiten-0.2.0-macos-arm64.zip")
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn hosts_come_out_of_urls_for_messages() {
        assert_eq!(host("https://anoma.ly/release/furiten/desktop/stable.json"), "anoma.ly");
        assert_eq!(host("https://anoma.ly"), "anoma.ly");
        assert_eq!(host("not-a-url"), "the server");
    }
}
