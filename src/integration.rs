//! Synchronize only IDG's own unpacked extension; never edit browser preferences.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const EXTENSION_ID: &str = "blmjnmockbofpnipcniflmalfmiaolcj";
const FILES: &[(&str, &[u8])] = &[
    (
        "background.js",
        include_bytes!("../extension/background.js"),
    ),
    ("content.js", include_bytes!("../extension/content.js")),
    ("popup.js", include_bytes!("../extension/popup.js")),
    ("popup.html", include_bytes!("../extension/popup.html")),
    (
        "images/icon16.png",
        include_bytes!("../extension/images/icon16.png"),
    ),
    (
        "images/icon32.png",
        include_bytes!("../extension/images/icon32.png"),
    ),
    (
        "images/icon48.png",
        include_bytes!("../extension/images/icon48.png"),
    ),
    (
        "images/icon128.png",
        include_bytes!("../extension/images/icon128.png"),
    ),
    // Commit manifest last so the extension never reloads an incomplete update.
    (
        "manifest.json",
        include_bytes!("../extension/manifest.json"),
    ),
];
fn manifest(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(
        path.join("manifest.json"),
    )?)?)
}

pub fn sync_folder(path: &Path) -> Result<bool> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("Extension update is busy; retry."))?;
    let target = manifest(path).context("Select the existing IDG Integration Module folder")?;
    let source: Value = serde_json::from_slice(include_bytes!("../extension/manifest.json"))?;
    ensure!(
        target["key"] == source["key"],
        "This folder does not contain IDG Integration Module"
    );
    if target["version"] == source["version"] {
        return Ok(false);
    }
    let old = target["version"]
        .as_str()
        .context("Invalid extension version")?;
    let parse = |s: &str| {
        s.split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    ensure!(
        parse(old) < parse(crate::APP_VERSION),
        "The extension is newer than this IDG app; update IDG first"
    );
    let mut previous = Vec::new();
    for (relative, data) in FILES {
        let file = path.join(relative);
        previous.push((file.clone(), std::fs::read(&file).ok()));
        let write = (|| -> Result<()> {
            std::fs::create_dir_all(file.parent().unwrap())?;
            let mut temp = tempfile::NamedTempFile::new_in(file.parent().unwrap())?;
            use std::io::Write;
            temp.write_all(data)?;
            temp.as_file().sync_all()?;
            temp.persist(&file)?;
            Ok(())
        })();
        if let Err(error) = write {
            for (file, bytes) in previous.into_iter().rev() {
                if let Some(bytes) = bytes {
                    let _ = std::fs::write(file, bytes);
                } else {
                    let _ = std::fs::remove_file(file);
                }
            }
            return Err(error
                .context("Could not update extension files. Check folder permissions and retry."));
        }
    }
    Ok(true)
}

fn paths_in_profile(profile: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for name in ["Preferences", "Secure Preferences"] {
        let Some(data) = std::fs::read(profile.join(name))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        else {
            continue;
        };
        let settings = &data["extensions"]["settings"][EXTENSION_ID];
        // Chrome's unpacked location = 4. Store/policy extensions must use Chrome's updater.
        if settings["location"].as_u64() != Some(4) {
            continue;
        }
        if let Some(path) = settings["path"].as_str() {
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                profile.join("Extensions").join(path)
            };
            if path.is_dir() && !result.contains(&path) {
                result.push(path);
            }
        }
    }
    result
}

pub fn remember_folder(path: &Path) -> Result<()> {
    let folder = path.canonicalize()?;
    let location = dirs::config_dir()
        .context("AppData folder unavailable")?
        .join("IDG");
    std::fs::create_dir_all(&location)?;
    std::fs::write(
        location.join("extension-folder.json"),
        serde_json::to_vec(&folder)?,
    )?;
    Ok(())
}

pub fn detected_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(config) = dirs::config_dir()
        && let Ok(bytes) = std::fs::read(config.join("IDG/extension-folder.json"))
        && let Ok(path) = serde_json::from_slice::<PathBuf>(&bytes)
        && path.is_dir()
    {
        folders.push(path);
    }
    if let Some(local) = dirs::data_local_dir() {
        for browser in [
            "Google/Chrome/User Data",
            "Microsoft/Edge/User Data",
            "Google/Chrome Beta/User Data",
            "BraveSoftware/Brave-Browser/User Data",
        ] {
            if let Ok(profiles) = std::fs::read_dir(local.join(browser)) {
                for profile in profiles.flatten().filter(|entry| entry.path().is_dir()) {
                    for folder in paths_in_profile(&profile.path()) {
                        if !folders.contains(&folder) {
                            folders.push(folder);
                        }
                    }
                }
            }
        }
    }
    folders
}

pub fn synchronize() -> Result<usize> {
    let mut count = 0;
    for folder in detected_folders() {
        if sync_folder(&folder)? {
            count += 1;
        }
    }
    Ok(count)
}

#[derive(Clone)]
pub struct Connection {
    pub version: String,
    pub seen: std::time::Instant,
    pub install_type: String,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_only_idg_unpacked_paths_and_updates_without_changing_preferences() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("IDG extension");
        std::fs::create_dir(&folder).unwrap();
        let mut m: Value =
            serde_json::from_slice(include_bytes!("../extension/manifest.json")).unwrap();
        m["version"] = "1.2.1".into();
        std::fs::write(
            folder.join("manifest.json"),
            serde_json::to_vec(&m).unwrap(),
        )
        .unwrap();
        let preferences = serde_json::to_vec(&serde_json::json!({"extensions":{"settings":{EXTENSION_ID:{"location":4,"path":folder}}}})).unwrap();
        std::fs::write(root.path().join("Preferences"), &preferences).unwrap();
        assert_eq!(paths_in_profile(root.path()), vec![folder.clone()]);
        assert!(sync_folder(&folder).unwrap());
        assert_eq!(manifest(&folder).unwrap()["version"], crate::APP_VERSION);
        assert!(!sync_folder(&folder).unwrap());
        assert_eq!(
            std::fs::read(root.path().join("Preferences")).unwrap(),
            preferences
        );
        m["key"] = "another-extension".into();
        std::fs::write(
            folder.join("manifest.json"),
            serde_json::to_vec(&m).unwrap(),
        )
        .unwrap();
        assert!(sync_folder(&folder).is_err());
    }
}
