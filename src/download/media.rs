//! YouTube page downloads through the bundled yt-dlp/FFmpeg/Deno toolchain.
use super::types::{Downloads, Status, safe_filename, with_item};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

/// Strip playlists/radio/tracking parameters. Only supported video pages route to yt-dlp.
pub fn video_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    let host = url.host_str()?;
    let id = if host == "youtu.be" {
        url.path().trim_matches('/').to_owned()
    } else if matches!(
        host,
        "youtube.com"
            | "www.youtube.com"
            | "m.youtube.com"
            | "music.youtube.com"
            | "www.youtube-nocookie.com"
    ) {
        if url.path() == "/watch" {
            url.query_pairs()
                .find(|(key, _)| key == "v")?
                .1
                .into_owned()
        } else {
            let mut segments = url.path_segments()?;
            if !matches!(segments.next()?, "shorts" | "embed" | "live") {
                return None;
            }
            segments.next()?.to_owned()
        }
    } else {
        return None;
    };
    if id.len() != 11
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    Some(format!("https://www.youtube.com/watch?v={id}"))
}

pub fn default_filename(url: &str) -> Option<String> {
    let canonical = video_url(url)?;
    Some(format!("YouTube-{}.mp4", canonical.rsplit('=').next()?))
}

fn tools_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("tools")
}

pub fn tools_available() -> bool {
    available_at(&tools_dir())
}

fn available_at(dir: &Path) -> bool {
    ["yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe", "deno.exe"]
        .iter()
        .all(|file| dir.join(file).is_file())
}

fn command(
    tools: &Path,
    url: &str,
    output: &Path,
    connections: usize,
    cookies: Option<&Path>,
) -> Command {
    let mut command = Command::new(tools.join("yt-dlp.exe"));
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command.args([
        "--ignore-config",
        "--no-cache-dir",
        "--no-playlist",
        "--no-colors",
        "--newline",
        "--progress",
        "--no-simulate",
        "--no-overwrites",
        "--socket-timeout",
        "20",
        "--retries",
        "3",
        "--fragment-retries",
        "3",
        "--progress-delta",
        "0.25",
    ]);
    command.arg("--ffmpeg-location").arg(tools);
    command
        .arg("--js-runtimes")
        .arg(format!("deno:{}", tools.join("deno.exe").display()));
    command.args([
        "--format",
        "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b",
        "--merge-output-format",
        "mp4",
        "--remux-video",
        "mp4",
    ]);
    command.arg("--concurrent-fragments").arg(
        if connections == 0 {
            8
        } else {
            connections.clamp(1, 32)
        }
        .to_string(),
    );
    command.args([
        "--progress-template",
        "download:IDG_PROGRESS %(progress)j",
        "--print",
        "after_move:IDG_FILE %(filepath)j",
    ]);
    let stem = output
        .with_extension("")
        .to_string_lossy()
        .replace('%', "%%");
    command.arg("--output").arg(format!("{stem}.%(ext)s"));
    if let Some(cookies) = cookies {
        command.arg("--cookies").arg(cookies);
    }
    command
        .arg("--")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

fn progress(downloads: &Downloads, id: &str, line: &str) {
    if let Some(value) = line
        .strip_prefix("IDG_PROGRESS ")
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
    {
        with_item(downloads, id, |item| {
            item.status = Status::Downloading;
            item.downloaded_bytes = value["downloaded_bytes"].as_u64().unwrap_or(0);
            item.total_bytes = value["total_bytes"]
                .as_u64()
                .or_else(|| value["total_bytes_estimate"].as_u64());
            item.speed_bps = value["speed"].as_f64().unwrap_or(0.0);
        });
    }
}

pub async fn download(downloads: Downloads, id: String) -> Result<()> {
    download_with_tools(downloads, id, tools_dir()).await
}

async fn download_with_tools(downloads: Downloads, id: String, tools: PathBuf) -> Result<()> {
    let item = downloads
        .lock()
        .unwrap()
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .context("Download was removed")?;
    let url = video_url(&item.url).context("Unsupported YouTube video URL")?;
    anyhow::ensure!(
        available_at(&tools),
        "YouTube tools are missing. Install the full IDG package, then retry."
    );
    let directory = item.save_path.parent().unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(directory).await?;
    let directory = tokio::fs::canonicalize(directory).await?;
    let filename = safe_filename(&item.filename);
    let filename = Path::new(&filename).with_extension("mp4");
    let output = directory.join(filename);
    anyhow::ensure!(
        !output.exists(),
        "A file with this name already exists. Choose another filename."
    );

    // Close the handle before yt-dlp opens it. TempPath removes session cookies on every exit path.
    let cookies = if let Some(contents) = &item.media_cookies {
        use std::io::Write;
        let mut file = tempfile::Builder::new()
            .prefix("idg-youtube-")
            .suffix(".txt")
            .tempfile()?;
        file.write_all(contents.as_bytes())?;
        Some(file.into_temp_path())
    } else {
        None
    };
    with_item(&downloads, &id, |item| {
        item.status = Status::Probing;
        item.num_segments = if item.num_segments == 0 {
            8
        } else {
            item.num_segments.clamp(1, 32)
        };
        item.save_path = output.clone();
        item.filename = output.file_name().unwrap().to_string_lossy().into_owned();
    });
    let mut child = command(&tools, &url, &output, item.num_segments, cookies.as_deref())
        .spawn()
        .context("Could not start the bundled YouTube downloader")?;
    let process_id = child.id();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let errors = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let captured = errors.clone();
    let stderr = child.stderr.take().unwrap();
    let error_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut errors = captured.lock().unwrap();
            if errors.len() == 8 {
                errors.pop_front();
            }
            errors.push_back(line.chars().take(1000).collect());
        }
    });
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut reported_file = None;
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break; };
                progress(&downloads, &id, &line);
                if let Some(file) = line.strip_prefix("IDG_FILE ").and_then(|value| serde_json::from_str::<String>(value).ok()) {
                    reported_file = Some(PathBuf::from(file));
                }
            }
            _ = tick.tick() => {
                if item.cancel_flag.load(Ordering::Relaxed) {
                    #[cfg(windows)]
                    if let Some(pid) = process_id {
                        let mut kill = Command::new("taskkill");
                        kill.creation_flags(0x08000000);
                        let _ = kill.args(["/PID", &pid.to_string(), "/T", "/F"]).output().await;
                    }
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    error_task.abort();
                    with_item(&downloads, &id, |item| { item.status = Status::Cancelled; item.speed_bps = 0.0; });
                    return Ok(());
                }
            }
        }
    }
    let status = child.wait().await?;
    let _ = error_task.await;
    if !status.success() {
        let errors = errors.lock().unwrap();
        let message = errors
            .iter()
            .rev()
            .find(|line| line.contains("ERROR:"))
            .or_else(|| errors.back())
            .map(String::as_str)
            .unwrap_or("YouTube download failed");
        bail!("{message}");
    }
    let reported_file =
        reported_file.context("YouTube downloader did not report a completed file")?;
    anyhow::ensure!(
        tokio::fs::canonicalize(&reported_file).await? == tokio::fs::canonicalize(&output).await?,
        "Unexpected YouTube output path"
    );
    let bytes = tokio::fs::metadata(&output).await?.len();
    anyhow::ensure!(bytes > 0, "YouTube produced an empty file");
    with_item(&downloads, &id, |item| {
        item.status = Status::Completed;
        item.downloaded_bytes = bytes;
        item.total_bytes = Some(bytes);
        item.speed_bps = 0.0;
        item.completed_at = Some(chrono::Utc::now());
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Live YouTube/network test; run after fetching the pinned media tools"]
    async fn supplied_youtube_video_downloads_with_audio_and_video() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let directory = root
            .join("target/youtube-smoke")
            .join(uuid::Uuid::new_v4().to_string());
        let output = directory.join("provided-video.mp4");
        let item = super::super::types::DownloadItem::new(
            "https://www.youtube.com/watch?v=5TBlPQrcweQ&list=RD5TBlPQrcweQ&start_radio=1".into(),
            output.clone(),
            0,
        );
        let id = item.id.clone();
        let downloads = Arc::new(Mutex::new(vec![item]));
        let tools = root.join("target/release/tools");
        download_with_tools(downloads.clone(), id, tools.clone())
            .await
            .unwrap();
        assert_eq!(downloads.lock().unwrap()[0].status, Status::Completed);
        let mut command = Command::new(tools.join("ffprobe.exe"));
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let probe = command
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "json",
            ])
            .arg(&output)
            .output()
            .await
            .unwrap();
        assert!(probe.status.success());
        let result: Value = serde_json::from_slice(&probe.stdout).unwrap();
        let streams = result["streams"].as_array().unwrap();
        assert!(streams.iter().any(|stream| stream["codec_type"] == "video"));
        assert!(streams.iter().any(|stream| stream["codec_type"] == "audio"));
        println!("Verified audio/video file: {}", output.display());
    }

    #[test]
    fn youtube_pages_are_canonicalized_without_playlists_or_host_spoofing() {
        let expected = "https://www.youtube.com/watch?v=5TBlPQrcweQ";
        assert_eq!(
            video_url(
                "https://www.youtube.com/watch?v=5TBlPQrcweQ&list=RD5TBlPQrcweQ&start_radio=1"
            )
            .as_deref(),
            Some(expected)
        );
        assert_eq!(
            video_url("https://youtu.be/5TBlPQrcweQ?t=2").as_deref(),
            Some(expected)
        );
        assert_eq!(
            video_url("https://www.youtube.com/shorts/5TBlPQrcweQ").as_deref(),
            Some(expected)
        );
        assert!(video_url("https://youtube.com.evil.test/watch?v=5TBlPQrcweQ").is_none());
        assert!(video_url("https://www.youtube.com/playlist?list=abc").is_none());
        assert!(video_url("https://www.youtube.com/watch?v=invalid").is_none());
    }
    #[test]
    fn helper_command_keeps_arguments_separate_and_progress_updates_the_queue() {
        let cmd = command(
            Path::new(r"C:\IDG Tools"),
            "https://www.youtube.com/watch?v=5TBlPQrcweQ",
            Path::new(r"C:\Downloads\100% video.mp4"),
            0,
            None,
        );
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|arg| arg == "--no-playlist"));
        assert!(args.iter().any(|arg| arg.contains("100%% video")));
        let item = super::super::types::DownloadItem::new(
            "https://youtu.be/5TBlPQrcweQ".into(),
            "video.mp4".into(),
            0,
        );
        let id = item.id.clone();
        let downloads = Arc::new(Mutex::new(vec![item]));
        progress(
            &downloads,
            &id,
            r#"IDG_PROGRESS {"downloaded_bytes":125000,"total_bytes":500000,"speed":62500}"#,
        );
        let item = downloads.lock().unwrap()[0].clone();
        assert_eq!(item.downloaded_bytes, 125000);
        assert_eq!(item.total_bytes, Some(500000));
        assert_eq!(item.speed_bps, 62500.0);
    }
}
