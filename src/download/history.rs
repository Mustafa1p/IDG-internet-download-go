//! Durable transfer history. Credentials and live worker state never go to disk.
use super::types::{DownloadItem, Status};
use chrono::{DateTime, Utc};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(serde::Serialize, serde::Deserialize)]
struct Record {
    id: String,
    url: String,
    path: PathBuf,
    filename: String,
    total: Option<u64>,
    downloaded: u64,
    status: Status,
    added: String,
    completed: Option<String>,
    error: Option<String>,
}
fn default_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("IDG/download-history.json")
}
pub fn load_default() -> anyhow::Result<Vec<DownloadItem>> {
    load(&default_path())
}
pub fn save_default(items: &[DownloadItem]) -> anyhow::Result<()> {
    save(&default_path(), items)
}
fn load(path: &Path) -> anyhow::Result<Vec<DownloadItem>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let records: Vec<Record> = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(records.into_iter().map(|r| {
        let mut item = DownloadItem::new(r.url, r.path, 0);
        item.id = r.id;
        item.filename = r.filename;
        item.total_bytes = r.total;
        item.downloaded_bytes = r.downloaded;
        item.added_at = DateTime::parse_from_rfc3339(&r.added).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
        item.completed_at = r.completed.and_then(|s| DateTime::parse_from_rfc3339(&s).ok()).map(|d| d.with_timezone(&Utc));
        item.status = r.status;
        item.error = r.error;
        if !matches!(item.status, Status::Completed | Status::Failed | Status::Cancelled) {
            item.status = Status::Failed;
            item.error = Some("Interrupted when IDG closed. Add the link again to restart; the partial file is still in its folder.".into());
        }
        item
    }).collect())
}
fn save(path: &Path, items: &[DownloadItem]) -> anyhow::Result<()> {
    let records: Vec<_> = items
        .iter()
        .filter(|i| !i.awaiting_confirmation)
        .map(|i| Record {
            id: i.id.clone(),
            url: i.url.clone(),
            path: i.save_path.clone(),
            filename: i.filename.clone(),
            total: i.total_bytes,
            downloaded: i.downloaded_bytes,
            status: i.status.clone(),
            added: i.added_at.to_rfc3339(),
            completed: i.completed_at.map(|d| d.to_rfc3339()),
            error: i.error.clone(),
        })
        .collect();
    let encoded = serde_json::to_vec(&records)?;
    match std::fs::read(path) {
        Ok(existing) => {
            if existing == encoded {
                return Ok(());
            }
            // Do not replace unreadable history with an empty list after a load error.
            serde_json::from_slice::<Vec<Record>>(&existing).map_err(|_| {
                anyhow::anyhow!(
                    "Saved download history is unreadable; the original file has been preserved"
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("No history directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&encoded)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_survives_restart_without_cookies_or_live_workers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let output = dir.path().join("file.zip");
        std::fs::write(&output, b"partial bytes").unwrap();
        let mut item = DownloadItem::new("https://example.com/file.zip".into(), output.clone(), 4);
        item.media_cookies = Some("secret session cookie".into());
        item.post_data = Some("token=secret-form-token".into());
        item.headers
            .insert("authorization", "secret-token".parse().unwrap());
        item.status = Status::Downloading;
        item.downloaded_bytes = 13;
        save(&path, &[item.clone()]).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("secret"));
        let loaded = load(&path).unwrap();
        assert_eq!(loaded[0].id, item.id);
        assert_eq!(loaded[0].status, Status::Failed);
        assert_eq!(loaded[0].downloaded_bytes, 13);
        assert_eq!(std::fs::read(output).unwrap(), b"partial bytes");
        item.status = Status::Completed;
        save(&path, &[item]).unwrap();
        assert_eq!(load(&path).unwrap()[0].status, Status::Completed);
        save(&path, &[]).unwrap();
        assert!(load(&path).unwrap().is_empty());
        std::fs::write(&path, b"unreadable existing history").unwrap();
        assert!(save(&path, &[]).is_err());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"unreadable existing history"
        );
    }
}
