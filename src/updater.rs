//! Automatic update checker.
//!
//! On startup, queries the GitHub releases API to see if a newer version is
//! available.  If so, the GUI shows a banner with a direct link to the
//! release page.

use serde::Deserialize;

/// Information about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// The new version string (e.g. `"1.1.0"`).
    pub version: String,
    /// URL to the GitHub release page.
    pub release_url: String,
    /// Direct URL to the setup installer asset, if found.
    pub installer_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

const RELEASES_API: &str =
    "https://api.github.com/repos/Mustafa1p/IDG-internet-download-go/releases/latest";

/// Check GitHub for a newer release.  Returns `None` if already up to date
/// or if the check fails (no error is surfaced to the user).
pub async fn check_for_update(
    client: &reqwest::Client,
    current_version: &str,
) -> Option<UpdateInfo> {
    let release: GhRelease = client
        .get(RELEASES_API)
        .header("User-Agent", "IDG-update-checker")
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    // Strip leading 'v' from tag (e.g. "v1.1.0" → "1.1.0").
    let latest = release.tag_name.trim_start_matches('v');

    if !is_newer(latest, current_version) {
        return None;
    }

    // Find the setup installer asset.
    let installer_url = release
        .assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().contains("setup"))
        .map(|a| a.browser_download_url.clone());

    Some(UpdateInfo {
        version: latest.to_string(),
        release_url: release.html_url,
        installer_url,
    })
}

/// Returns `true` if `candidate` is strictly newer than `current`.
///
/// Compares major.minor.patch numerically.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(s: &str) -> (u32, u32, u32) {
        let mut it = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        (
            it.next().unwrap_or(0),
            it.next().unwrap_or(0),
            it.next().unwrap_or(0),
        )
    }
    parse(candidate) > parse(current)
}
