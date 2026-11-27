use super::{
    request::{Control, RequestContext, Spec},
    types::{DownloadItem, Downloads, Status, safe_filename, with_item},
};
use anyhow::{Context, Result, bail};
use reqwest::Url;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

const MAX_METAINFO: usize = 32 * 1024 * 1024;

pub fn magnet(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "magnet"
            && url.query_pairs().any(|(key, value)| {
                key == "xt" && (value.starts_with("urn:btih:") || value.starts_with("urn:btmh:"))
            })
    })
}

fn has_torrent_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
}

pub fn local_file(value: &str) -> Option<PathBuf> {
    let url = Url::parse(value).ok()?;
    let path = url.to_file_path().ok()?;
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
        .then_some(path)
}

pub fn matches(url: &str, filename: &str) -> bool {
    magnet(url)
        || local_file(url).is_some()
        || Url::parse(url).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && (has_torrent_extension(url.path()) || has_torrent_extension(filename))
        })
}

pub fn magnet_filename(value: &str) -> Option<String> {
    if !magnet(value) {
        return None;
    }
    Some(
        Url::parse(value)
            .ok()?
            .query_pairs()
            .find(|(key, value)| key == "dn" && !value.trim().is_empty())
            .map(|(_, value)| safe_filename(&value))
            .unwrap_or_else(|| "Torrent".into()),
    )
}

fn tools_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .with_file_name("tools")
        .join("torrent")
}

pub fn tools_available() -> bool {
    let tools = tools_dir();
    tools.join("python.exe").is_file() && tools.join("worker.py").is_file()
}

async fn metainfo(client: Arc<reqwest::Client>, item: &DownloadItem) -> Result<Vec<u8>> {
    if let Some(path) = local_file(&item.url) {
        let mut bytes = Vec::new();
        tokio::fs::File::open(path)
            .await?
            .take((MAX_METAINFO + 1) as u64)
            .read_to_end(&mut bytes)
            .await?;
        anyhow::ensure!(
            bytes.len() <= MAX_METAINFO,
            "Torrent metadata exceeds 32 MiB"
        );
        return Ok(bytes);
    }
    let request = RequestContext::new(client, &item.url, item.headers.clone())?;
    let spec = match &item.post_data {
        Some(body) => Spec::form(&item.url, body.clone()),
        None => Spec::get(&item.url),
    };
    let mut response = request
        .send(
            spec,
            Control {
                cancel: Some(&item.cancel_flag),
                total_timeout: Some(Duration::from_secs(60)),
                ..Default::default()
            },
        )
        .await?
        .leased
        .response
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= MAX_METAINFO,
            "Torrent metadata exceeds 32 MiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Event {
    Progress { done: u64, total: u64, rate: f64 },
    Complete { total: u64 },
    Error { message: String },
}

pub async fn download(
    client: Arc<reqwest::Client>,
    downloads: Downloads,
    id: String,
) -> Result<()> {
    let item = downloads
        .lock()
        .unwrap()
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .context("Torrent was removed")?;
    anyhow::ensure!(
        tools_available(),
        "Torrent tools are missing. Install the complete IDG package."
    );
    let directory = item
        .save_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(directory).await?;
    let working = tempfile::Builder::new()
        .prefix(".idg-torrent-")
        .tempdir_in(directory)?;
    let payload = working.path().join("files");
    tokio::fs::create_dir(&payload).await?;

    with_item(&downloads, &id, |item| {
        item.status = Status::Probing;
        item.downloaded_bytes = 0;
        item.total_bytes = None;
        item.num_segments = 0;
    });
    let metadata = async {
        if magnet(&item.url) {
            return Ok::<_, anyhow::Error>(serde_json::json!({ "magnet": item.url }));
        }
        let file = std::path::absolute(working.path().join("input.torrent"))?;
        tokio::fs::write(&file, metainfo(client, &item).await?).await?;
        Ok(serde_json::json!({ "file": file }))
    };
    let source = tokio::select! {
        biased;
        _ = super::cancelled(&item.cancel_flag) => {
            mark_cancelled(&downloads, &id);
            return Ok(());
        }
        result = metadata => result?,
    };
    let config = serde_json::json!({
        "source": source,
        "output": std::path::absolute(&payload)?,
        "download_limit": item.bandwidth.rate().min(i32::MAX as u64),
    });
    let Some(total) = run_worker(&downloads, &item, config).await? else {
        mark_cancelled(&downloads, &id);
        return Ok(());
    };
    let chosen = safe_filename(&item.filename);
    let name = if has_torrent_extension(&chosen) {
        Path::new(&chosen)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        chosen
    };
    let output = publish_folder(&payload, directory, &safe_filename(&name)).await?;
    with_item(&downloads, &id, |item| {
        item.filename = output.file_name().unwrap().to_string_lossy().into_owned();
        item.save_path = output;
        item.status = Status::Completed;
        item.downloaded_bytes = total;
        item.total_bytes = Some(total);
        item.speed_bps = 0.0;
        item.completed_at = Some(chrono::Utc::now());
    });
    Ok(())
}

fn mark_cancelled(downloads: &Downloads, id: &str) {
    with_item(downloads, id, |item| {
        item.status = Status::Cancelled;
        item.speed_bps = 0.0;
    });
}

async fn run_worker(
    downloads: &Downloads,
    item: &DownloadItem,
    config: serde_json::Value,
) -> Result<Option<u64>> {
    let tools = tools_dir();
    let mut command = Command::new(tools.join("python.exe"));
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .args(["-I", "-X", "utf8", "-u"])
        .arg(tools.join("worker.py"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Could not start the bundled BitTorrent engine")?;
    let mut input = child.stdin.take().unwrap();
    input.write_all(format!("{config}\n").as_bytes()).await?;
    let stderr = child.stderr.take().unwrap();
    let read_errors = tokio::spawn(async move {
        let mut errors = String::new();
        let _ = stderr.take(8192).read_to_string(&mut errors).await;
        errors
    });
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut complete = None;
    let mut error = None;
    loop {
        let line = tokio::select! {
            biased;
            _ = super::cancelled(&item.cancel_flag) => {
                drop(input);
                if tokio::time::timeout(Duration::from_secs(3), child.wait()).await.is_err() {
                    child.kill().await?;
                }
                return Ok(None);
            }
            line = lines.next_line() => line?,
        };
        let Some(line) = line else { break };
        match serde_json::from_str(&line).context("Invalid response from the BitTorrent engine")? {
            Event::Progress { done, total, rate } => with_item(downloads, &item.id, |item| {
                item.status = Status::Downloading;
                item.downloaded_bytes = done;
                item.total_bytes = Some(total);
                item.speed_bps = rate;
            }),
            Event::Complete { total } => complete = Some(total),
            Event::Error { message } => error = Some(message),
        }
    }
    let status = child.wait().await?;
    drop(input);
    let stderr = read_errors.await.unwrap_or_default();
    if let Some(error) = error {
        bail!("{error}");
    }
    anyhow::ensure!(
        status.success(),
        "BitTorrent engine exited unexpectedly. {}",
        stderr.trim()
    );
    complete
        .map(Some)
        .context("BitTorrent engine did not confirm completion")
}

async fn publish_folder(payload: &Path, directory: &Path, name: &str) -> Result<PathBuf> {
    for number in 0..1000 {
        let folder = directory.join(if number == 0 {
            name.to_owned()
        } else {
            format!("{name} ({number})")
        });
        // On Windows the rename does not replace an existing directory.
        if tokio::fs::try_exists(&folder).await? {
            continue;
        }
        match tokio::fs::rename(payload, &folder).await {
            Ok(()) => return Ok(folder),
            Err(_) if tokio::fs::try_exists(&folder).await? => continue,
            Err(error) => {
                return Err(error)
                    .context("Could not move the completed torrent to its destination");
            }
        }
    }
    bail!("Too many torrent folders with the same name")
}
