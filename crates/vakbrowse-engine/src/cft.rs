//! Chrome-for-Testing (`chrome-headless-shell`) discovery, download and pin.
//!
//! Cache layout: `{cache_dir}/vakbrowse/cft/{version}/{platform}/...`
//! plus a `{channel}.version` marker enabling offline reuse of the last
//! resolved channel version.
//!
//! Manifests: https://googlechromelabs.github.io/chrome-for-testing/

use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use vakbrowse_core::{Result, VakError};

const LKG_MANIFEST: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";
const KNOWN_GOOD_MANIFEST: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json";

pub const DEFAULT_CHANNEL: &str = "Stable";
const ARTIFACT: &str = "chrome-headless-shell";

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    channels: HashMap<String, ChannelEntry>,
    #[serde(default)]
    versions: Vec<ChannelEntry>,
}

#[derive(Debug, Deserialize)]
struct ChannelEntry {
    version: String,
    downloads: HashMap<String, Vec<Download>>,
}

#[derive(Debug, Deserialize)]
struct Download {
    platform: String,
    url: String,
}

/// A resolved engine binary on disk.
#[derive(Debug, Clone)]
pub struct CftArtifact {
    pub version: String,
    pub executable: PathBuf,
}

/// Map this host to a CfT platform key.
pub fn platform_key() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("mac-arm64"),
        ("macos", "x86_64") => Ok("mac-x64"),
        ("linux", "x86_64") => Ok("linux64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("windows", _) => Ok("win64"),
        (os, arch) => Err(VakError::Unsupported(format!("{os}/{arch}"))),
    }
}

/// Default cache root: `$XDG_CACHE_HOME|~/Library/Caches|…/vakbrowse`.
pub fn cache_root() -> Result<PathBuf> {
    let base =
        dirs::cache_dir().ok_or_else(|| VakError::Engine("no cache dir available".into()))?;
    Ok(base.join("vakbrowse").join("cft"))
}

fn artifact_rel_path(platform: &str) -> String {
    if cfg!(windows) {
        format!("{ARTIFACT}-{platform}/{ARTIFACT}.exe")
    } else {
        format!("{ARTIFACT}-{platform}/{ARTIFACT}")
    }
}

#[derive(Debug, Clone)]
pub struct CftConfig {
    /// Channel used when no version is pinned ("Stable" default).
    pub channel: String,
    /// Pin an exact version (e.g. "141.0.7390.78"); None = channel latest.
    pub pin_version: Option<String>,
    /// Override cache location (tests).
    pub cache_dir: Option<PathBuf>,
}

impl Default for CftConfig {
    fn default() -> Self {
        Self {
            channel: DEFAULT_CHANNEL.into(),
            pin_version: None,
            cache_dir: None,
        }
    }
}

/// Ensure a chrome-headless-shell binary exists locally and return it.
///
/// Resolution order:
/// 1. pinned version already in cache,
/// 2. channel marker file pointing at a cached version (offline fast path),
/// 3. fetch manifest -> resolve URL -> cache hit? -> else download+extract.
pub async fn ensure_headless_shell(config: &CftConfig) -> Result<CftArtifact> {
    let platform = platform_key()?;
    let rel = artifact_rel_path(platform);
    let root = match &config.cache_dir {
        Some(dir) => dir.clone(),
        None => cache_root()?,
    };

    // 1. pinned + cached
    if let Some(pin) = &config.pin_version {
        let exe = root.join(pin).join(platform).join(&rel);
        if exe.is_file() {
            return Ok(CftArtifact {
                version: pin.clone(),
                executable: exe,
            });
        }
    } else {
        // 2. channel marker fast path
        let marker = root.join(format!("{}.version", config.channel));
        if let Ok(version) = std::fs::read_to_string(&marker) {
            let version = version.trim();
            let exe = root.join(version).join(platform).join(&rel);
            if exe.is_file() {
                tracing::debug!(%version, "cft artifact cached (channel marker)");
                return Ok(CftArtifact {
                    version: version.to_string(),
                    executable: exe,
                });
            }
        }
    }

    // 3. manifest resolution
    let (version, url) = resolve_download_url(config).await?;
    let version_dir = root.join(&version).join(platform);
    let executable = version_dir.join(&rel);

    if !executable.is_file() {
        download_and_extract(&url, &root, &version, platform).await?;
        if !executable.is_file() {
            return Err(VakError::Engine(format!(
                "downloaded archive did not contain {rel}"
            )));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&executable)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&executable, perms)?;
        }
    }

    if config.pin_version.is_none() {
        let marker = root.join(format!("{}.version", config.channel));
        let _ = std::fs::create_dir_all(root);
        let _ = std::fs::write(marker, format!("{version}\n"));
    }

    tracing::info!(%version, path = %executable.display(), "chrome-headless-shell ready");
    Ok(CftArtifact { version, executable })
}

async fn resolve_download_url(config: &CftConfig) -> Result<(String, String)> {
    let platform = platform_key()?;
    let manifest_url = match &config.pin_version {
        Some(_) => KNOWN_GOOD_MANIFEST,
        None => LKG_MANIFEST,
    };
    let body = tokio::task::spawn_blocking(move || http_get(manifest_url))
        .await
        .map_err(|e| VakError::Engine(format!("manifest join: {e}")))??;

    let manifest: Manifest =
        serde_json::from_slice(&body).map_err(|e| VakError::Engine(format!("bad manifest: {e}")))?;

    let entry = pick_entry(config, &manifest)?;
    let dl = entry
        .downloads
        .get(ARTIFACT)
        .and_then(|list| list.iter().find(|d| d.platform == platform))
        .ok_or_else(|| {
            VakError::NotFound(format!(
                "{ARTIFACT} for {platform} in version {}",
                entry.version
            ))
        })?;

    Ok((entry.version.clone(), dl.url.clone()))
}

fn pick_entry<'m>(config: &CftConfig, manifest: &'m Manifest) -> Result<&'m ChannelEntry> {
    if let Some(pin) = &config.pin_version {
        return manifest
            .versions
            .iter()
            .find(|v| v.version == *pin)
            .ok_or_else(|| VakError::NotFound(format!("pinned cft version {pin}")));
    }
    manifest
        .channels
        .get(&config.channel)
        .ok_or_else(|| VakError::NotFound(format!("channel {}", config.channel)))
}

fn blocking_download(url: String) -> Result<Vec<u8>> {
    let resp = ureq::get(&url)
        .call()
        .map_err(|e| VakError::Http(format!("{url}: {e}")))?;
    let len = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let mut reader = resp.into_body().into_reader();
    let mut buf = Vec::with_capacity(len.min(u32::MAX as u64) as usize);
    std::io::copy(&mut reader, &mut buf)?;
    Ok(buf)
}

async fn download_and_extract(
    zip_url: &str,
    root: &Path,
    version: &str,
    platform: &str,
) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let tmp = tempfile::tempdir_in(root)?;
    let zip_path = tmp.path().join("artifact.zip");

    let bytes = {
        let url = zip_url.to_string();
        tokio::task::spawn_blocking(move || blocking_download(url))
            .await
            .map_err(|e| VakError::Engine(format!("download join: {e}")))??
    };
    if bytes.is_empty() {
        return Err(VakError::Http(format!("empty body from {zip_url}")));
    }
    {
        let mut f = std::fs::File::create(&zip_path)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }

    let dest = root.join(version).join(platform);
    std::fs::create_dir_all(&dest)?;
    let file = std::fs::File::open(&zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| VakError::Engine(format!("bad zip: {e}")))?;
    archive
        .extract(&dest)
        .map_err(|e| VakError::Engine(format!("zip extract: {e}")))?;

    tracing::debug!(dest = %dest.display(), "extracted");
    Ok(())
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| VakError::Http(format!("{url}: {e}")))?;
    let mut buf = Vec::new();
    resp.into_body().into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

/// Best-effort discovery of a system Chrome/Chromium as an offline fallback.
pub fn find_system_chrome() -> Option<PathBuf> {
    let candidates: &[&str] = match std::env::consts::OS {
        "macos" => &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        ],
        "linux" => &[
            "/usr/bin/google-chrome-stable",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
        ],
        "windows" => &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ],
        _ => &[],
    };
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_key_is_known() {
        assert!(platform_key().is_ok());
    }

    #[test]
    fn artifact_rel_path_shape() {
        assert!(artifact_rel_path("mac-arm64").starts_with("chrome-headless-shell-mac-arm64/"));
    }
}
