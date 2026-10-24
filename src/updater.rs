//! Download and verify complete application updates before handing over to Inno.
use anyhow::{Context, Result, ensure};
use futures::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

const RELEASES_API: &str =
    "https://api.github.com/repos/Mustafa1p/IDG-internet-download-go/releases/latest";

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub release_notes: String,
    pub release_url: String,
    pub installer_url: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GhAsset>,
    #[serde(default)]
    body: Option<String>,
}
#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: u64,
}

pub struct PreparedUpdate {
    pub installer: PathBuf,
    pub directory: tempfile::TempDir,
}

pub async fn check_for_update(
    client: &reqwest::Client,
    current: &str,
) -> Result<Option<UpdateInfo>> {
    let release: GhRelease = client
        .get(RELEASES_API)
        .header("User-Agent", "IDG-update-checker")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    update_from_release(release, current)
}

fn update_from_release(release: GhRelease, current: &str) -> Result<Option<UpdateInfo>> {
    let version = release.tag_name.trim_start_matches('v');
    if !is_newer(version, current) {
        return Ok(None);
    }
    ensure!(
        version.bytes().all(|c| c.is_ascii_digit() || c == b'.'),
        "Invalid release version"
    );
    let name = format!("IDG-Setup-v{version}.exe");
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .context("The release installer is not available yet. Try again shortly.")?;
    let digest = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .context("This release has no SHA-256 digest; automatic installation is unavailable.")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid release checksum"
    );
    ensure!(asset.size > 0, "Empty update installer");
    let prefix = format!(
        "https://github.com/Mustafa1p/IDG-internet-download-go/releases/download/v{version}/"
    );
    ensure!(
        asset.browser_download_url.starts_with(&prefix),
        "Unexpected update source"
    );
    Ok(Some(UpdateInfo {
        version: version.into(),
        release_notes: release.body.unwrap_or_default(),
        release_url: release.html_url,
        installer_url: asset.browser_download_url.clone(),
        sha256: digest.to_ascii_lowercase(),
        size: asset.size,
    }))
}

/// Show the release's change list as plain text, with bounded fallback content.
pub fn release_highlights(notes: &str) -> Vec<String> {
    let clean = |line: &str| {
        line.replace("**", "")
            .replace('`', "")
            .chars()
            .take(500)
            .collect::<String>()
    };
    let bullets: Vec<_> = notes
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("- ")
                .or_else(|| line.trim().strip_prefix("* "))
        })
        .filter(|line| !line.trim().is_empty())
        .take(12)
        .map(clean)
        .collect();
    if !bullets.is_empty() {
        return bullets;
    }
    let text: String = notes.trim().chars().take(2400).collect();
    if text.is_empty() {
        vec!["Improvements and fixes are available. Open the release notes for details.".into()]
    } else {
        vec![clean(&text)]
    }
}

pub async fn prepare_update(
    client: &reqwest::Client,
    info: &UpdateInfo,
    progress: tokio::sync::watch::Sender<f32>,
) -> Result<PreparedUpdate> {
    let directory = tempfile::Builder::new().prefix("IDG-update-").tempdir()?;
    let installer = directory
        .path()
        .join(format!("IDG-Setup-v{}.exe", info.version));
    let response = client
        .get(&info.installer_url)
        .timeout(std::time::Duration::from_secs(900))
        .send()
        .await?
        .error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&installer).await?;
    let mut hasher = Sha256::new();
    let mut length = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        length += chunk.len() as u64;
        ensure!(
            length <= info.size,
            "Update is larger than the published installer"
        );
        file.write_all(&chunk).await?;
        hasher.update(&chunk);
        let _ = progress.send(length as f32 / info.size as f32);
    }
    file.sync_all().await?;
    ensure!(
        length == info.size,
        "Update download was interrupted. Please retry."
    );
    ensure!(
        format!("{:x}", hasher.finalize()) == info.sha256,
        "Update checksum mismatch. Please retry."
    );
    Ok(PreparedUpdate {
        installer,
        directory,
    })
}

pub fn install_update(update: PreparedUpdate, startup: bool) -> Result<()> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .context("Installation directory is unavailable")?;
    let helper = update.directory.path().join("IDG-update-helper.exe");
    std::fs::copy(directory.join("IDG-native.exe"), &helper)
        .context("The complete IDG package is required to install updates")?;
    let plan = update.directory.path().join("update-plan.json");
    std::fs::write(
        &plan,
        serde_json::to_vec(&serde_json::json!({
            "directory":directory, "installer":update.installer,
            "parent_pid":std::process::id(), "startup":startup,
        }))?,
    )?;
    let mut command = std::process::Command::new(helper);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .arg("--apply-update")
        .arg(&plan)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Could not start the update helper. Try again or open the release page.")?;
    // Confirm the helper can read the plan before closing this app.
    for _ in 0..100 {
        if plan.with_extension("ready").is_file() {
            let _ = update.directory.keep();
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            anyhow::bail!("The update helper could not start. Reinstall the complete IDG package.");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = child.kill();
    anyhow::bail!("The update helper did not respond. IDG will stay open; retry the update.")
}

fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(value: &str) -> Option<Vec<u32>> {
        value
            .split('.')
            .map(str::parse)
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()
    }
    matches!((parse(candidate), parse(current)), (Some(a), Some(b)) if a > b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_fixture(version: &str) -> GhRelease {
        serde_json::from_value(serde_json::json!({
            "tag_name": format!("v{version}"),
            "html_url": format!("https://github.com/Mustafa1p/IDG-internet-download-go/releases/tag/v{version}"),
            "body": "A new release\n\n- Fix the tiny window\n- Add update alerts",
            "assets": [{
                "name": format!("IDG-Setup-v{version}.exe"),
                "browser_download_url": format!("https://github.com/Mustafa1p/IDG-internet-download-go/releases/download/v{version}/IDG-Setup-v{version}.exe"),
                "digest": format!("sha256:{}", "a".repeat(64)),
                "size": 42
            }]
        })).unwrap()
    }

    #[test]
    fn update_alert_includes_release_fixes_and_ignores_installed_versions() {
        let info = update_from_release(release_fixture("1.4.3"), "1.4.2")
            .unwrap()
            .unwrap();
        assert_eq!(info.version, "1.4.3");
        assert_eq!(
            release_highlights(&info.release_notes),
            ["Fix the tiny window", "Add update alerts"]
        );
        assert!(
            update_from_release(release_fixture("1.4.3"), "1.4.3")
                .unwrap()
                .is_none()
        );
        assert!(
            update_from_release(release_fixture("1.4.2"), "1.4.3")
                .unwrap()
                .is_none()
        );
        let mut missing_notes = release_fixture("1.4.3");
        missing_notes.body = None;
        let info = update_from_release(missing_notes, "1.4.2")
            .unwrap()
            .unwrap();
        assert!(!release_highlights(&info.release_notes)[0].is_empty());
    }

    #[test]
    fn alert_content_is_bounded_and_does_not_bypass_installer_validation() {
        let notes = format!("- **Fixed** `window`\n{}", "- Other fix\n".repeat(1000));
        let highlights = release_highlights(&notes);
        assert_eq!(highlights.len(), 12);
        assert_eq!(highlights[0], "Fixed window");
        assert!(release_highlights(&"🦀".repeat(5000))[0].chars().count() <= 500);
        let mut release = release_fixture("1.4.3");
        release.assets[0].digest = None;
        assert!(update_from_release(release, "1.4.2").is_err());
    }
    #[test]
    fn versions_are_compared_numerically() {
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.2.0", "1.2.0"));
        assert!(!is_newer("latest", "1.0.0"));
    }
    #[tokio::test]
    async fn updater_checks_exact_size_and_checksum_before_installation() {
        use axum::{Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/setup", get(|| async { b"test installer".to_vec() })),
            )
            .await
            .unwrap();
        });
        let mut info = UpdateInfo {
            version: "1.3.0".into(),
            release_notes: String::new(),
            release_url: String::new(),
            installer_url: format!("http://{address}/setup"),
            sha256: format!("{:x}", Sha256::digest(b"test installer")),
            size: 14,
        };
        let (progress, _) = tokio::sync::watch::channel(0.0);
        let client = reqwest::Client::new();
        let ready = prepare_update(&client, &info, progress.clone())
            .await
            .unwrap();
        assert_eq!(std::fs::read(&ready.installer).unwrap(), b"test installer");
        info.sha256 = "0".repeat(64);
        assert!(
            prepare_update(&client, &info, progress.clone())
                .await
                .is_err()
        );
        info.size = 15;
        assert!(prepare_update(&client, &info, progress).await.is_err());
        task.abort();
    }
}
