//! YouTube and X page downloads through the bundled yt-dlp/FFmpeg/Deno toolchain.
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaFormat {
    #[default]
    Mp4,
    Mkv,
    Webm,
    Mp3,
    M4a,
    Opus,
    Wav,
    Flac,
}
impl MediaFormat {
    pub const ALL: [Self; 8] = [
        Self::Mp4,
        Self::Mkv,
        Self::Webm,
        Self::Mp3,
        Self::M4a,
        Self::Opus,
        Self::Wav,
        Self::Flac,
    ];
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
            Self::Mp3 => "mp3",
            Self::M4a => "m4a",
            Self::Opus => "opus",
            Self::Wav => "wav",
            Self::Flac => "flac",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4 video",
            Self::Mkv => "MKV video",
            Self::Webm => "WebM video",
            Self::Mp3 => "MP3 audio",
            Self::M4a => "M4A audio",
            Self::Opus => "Opus audio",
            Self::Wav => "WAV audio",
            Self::Flac => "FLAC audio",
        }
    }
    pub fn audio(self) -> bool {
        !matches!(self, Self::Mp4 | Self::Mkv | Self::Webm)
    }
}
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MediaOptions {
    pub format: MediaFormat,
    pub height: Option<u32>,
    pub audio_bitrate: Option<u32>,
}
#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub title: String,
    pub heights: Vec<u32>,
}
fn info_from_json(value: &Value) -> MediaInfo {
    let value = value["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .unwrap_or(value);
    let mut heights: Vec<u32> = value["formats"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|format| {
            format["vcodec"].as_str().is_some_and(|v| v != "none")
                && format["has_drm"].as_bool() != Some(true)
        })
        .filter_map(|format| {
            format["height"]
                .as_u64()
                .filter(|h| *h > 0 && *h <= 16384)
                .map(|h| h as u32)
        })
        .collect();
    heights.sort_unstable();
    heights.dedup();
    heights.reverse();
    MediaInfo {
        title: value["title"].as_str().unwrap_or("Video").into(),
        heights,
    }
}
fn cookie_file(contents: Option<&str>) -> Result<Option<tempfile::TempPath>> {
    use std::io::Write;
    if let Some(contents) = contents {
        let mut file = tempfile::Builder::new()
            .prefix("idg-youtube-")
            .suffix(".txt")
            .tempfile()?;
        file.write_all(contents.as_bytes())?;
        Ok(Some(file.into_temp_path()))
    } else {
        Ok(None)
    }
}
pub async fn inspect(url: &str, cookies: Option<&str>) -> Result<MediaInfo> {
    let tools = tools_dir();
    inspect_with_tools(url, cookies, &tools).await
}
async fn inspect_with_tools(url: &str, cookies: Option<&str>, tools: &Path) -> Result<MediaInfo> {
    anyhow::ensure!(
        available_at(tools),
        "Video tools are missing. Install the complete IDG package."
    );
    let url = video_url(url).context("Invalid video URL")?;
    let cookies = cookie_file(cookies)?;
    let mut command = Command::new(tools.join("yt-dlp.exe"));
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
        .args([
            "--ignore-config",
            "--no-cache-dir",
            "--no-playlist",
            "--playlist-items",
            "1",
            "--dump-single-json",
            "--skip-download",
            "--socket-timeout",
            "15",
            "--retries",
            "1",
            "--js-runtimes",
        ])
        .arg(js_runtime(tools));
    if let Some(cookies) = cookies.as_deref() {
        command.arg("--cookies").arg(cookies);
    }
    command
        .arg("--")
        .arg(url)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .context("Format check timed out. Retry or choose a format below.")??;
    anyhow::ensure!(
        output.status.success(),
        "Could not list formats: {}",
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(700)
            .collect::<String>()
    );
    Ok(info_from_json(&serde_json::from_slice(&output.stdout)?))
}
fn format_args(options: &MediaOptions) -> Vec<String> {
    let extension = options.format.extension();
    if options.format.audio() {
        let mut args: Vec<String> = [
            "--format",
            "ba/b",
            "--extract-audio",
            "--audio-format",
            extension,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        args.extend([
            "--audio-quality".into(),
            options
                .audio_bitrate
                .map(|n| format!("{}K", n.clamp(64, 320)))
                .unwrap_or_else(|| "0".into()),
        ]);
        return args;
    }
    let limit = options
        .height
        .map(|height| format!("[height<={}]", height.clamp(144, 16384)))
        .unwrap_or_default();
    let selector = if options.format == MediaFormat::Webm {
        format!("bv*[ext=webm]{limit}+ba[ext=webm]/b[ext=webm]{limit}")
    } else if options.format == MediaFormat::Mp4 {
        format!("bv*[ext=mp4]{limit}+ba[ext=m4a]/b[ext=mp4]{limit}/bv*{limit}+ba/b{limit}")
    } else {
        format!("bv*{limit}+ba/b{limit}")
    };
    vec![
        "--format".into(),
        selector,
        "--merge-output-format".into(),
        extension.into(),
        "--remux-video".into(),
        extension.into(),
    ]
}

fn source_format_args(url: &str, options: &MediaOptions) -> Vec<String> {
    let mut args = format_args(options);
    if reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "tiktok.com" || host.ends_with(".tiktok.com"))
    {
        // Missing metadata is allowed; explicitly watermarked source variants are not.
        args[1] = args[1]
            .split('/')
            .map(|alternative| {
                alternative
                    .split('+')
                    .map(|selector| format!("{selector}[format_note!*=?watermarked]"))
                    .collect::<Vec<_>>()
                    .join("+")
            })
            .collect::<Vec<_>>()
            .join("/");
    }
    args
}

/// Strip tracking parameters. Only supported media pages route to yt-dlp.
pub fn video_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    let host = url.host_str()?;
    let path = url.path().trim_matches('/');
    let parts: Vec<_> = path.split('/').collect();
    let safe_id = |id: &str| {
        !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    };
    if matches!(
        host,
        "instagram.com" | "www.instagram.com" | "m.instagram.com"
    ) {
        if let [owner, kind @ ("p" | "reel" | "reels" | "tv"), id] = parts.as_slice()
            && !owner.is_empty()
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
            && safe_id(id)
        {
            return Some(format!("https://www.instagram.com/{kind}/{id}/"));
        }
        if let [kind @ ("p" | "reel" | "reels" | "tv"), id] = parts.as_slice()
            && safe_id(id)
        {
            return Some(format!("https://www.instagram.com/{kind}/{id}/"));
        }
        return None;
    }
    if matches!(host, "vm.tiktok.com" | "vt.tiktok.com") {
        return (parts.len() == 1 && safe_id(parts[0])).then(|| format!("https://{host}/{path}/"));
    }
    if matches!(host, "tiktok.com" | "www.tiktok.com" | "m.tiktok.com") {
        match parts.as_slice() {
            [owner, "video", id]
                if owner.starts_with('@')
                    && !id.is_empty()
                    && id.bytes().all(|b| b.is_ascii_digit()) =>
            {
                return Some(format!("https://www.tiktok.com/{owner}/video/{id}"));
            }
            ["t", id] if safe_id(id) => return Some(format!("https://www.tiktok.com/t/{id}/")),
            _ => return None,
        }
    }
    if matches!(
        host,
        "x.com"
            | "www.x.com"
            | "twitter.com"
            | "www.twitter.com"
            | "mobile.twitter.com"
            | "m.twitter.com"
    ) {
        let segments: Vec<_> = url
            .path()
            .trim_end_matches('/')
            .trim_start_matches('/')
            .split('/')
            .collect();
        let (owner, id, rest) = match segments.as_slice() {
            ["i", "web", "status", id, rest @ ..] => ("i/web", *id, rest),
            [owner, "status", id, rest @ ..] => (*owner, *id, rest),
            _ => return None,
        };
        if owner.is_empty()
            || !owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'/'))
            || id.is_empty()
            || !id.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let suffix = match rest {
            [] | [""] => String::new(),
            ["video", index]
                if !index.is_empty()
                    && index.bytes().all(|b| b.is_ascii_digit())
                    && index.parse::<u32>().ok()? > 0 =>
            {
                format!("/video/{index}")
            }
            _ => return None,
        };
        return Some(format!("https://x.com/{owner}/status/{id}{suffix}"));
    }
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
    let parsed = reqwest::Url::parse(&canonical).ok()?;
    if parsed.host_str()?.ends_with("instagram.com") || parsed.host_str()?.ends_with("tiktok.com") {
        let site = if parsed.host_str()?.ends_with("instagram.com") {
            "Instagram"
        } else {
            "TikTok"
        };
        return Some(format!(
            "{site}-{}.mp4",
            parsed.path().trim_end_matches('/').rsplit('/').next()?
        ));
    }
    if canonical.starts_with("https://x.com/") {
        let id = canonical.split("/status/").nth(1)?.replace('/', "-");
        return Some(format!("X-{id}.mp4"));
    }
    Some(format!("YouTube-{}.mp4", canonical.rsplit('=').next()?))
}

pub fn unsupported_reason(value: &str) -> Option<&'static str> {
    if super::music::source(value).is_some() {
        return (!super::music::ready()).then_some("Install the optional original-service music backend in Settings > Music, then connect the service account.");
    }
    let url = reqwest::Url::parse(value).ok()?;
    matches!(
        url.host_str()?,
        "open.spotify.com" | "play.spotify.com" | "spotify.com" | "www.spotify.com"
    )
    .then_some(
        "Use a Spotify track, album or playlist URL, then connect the account in Settings > Music.",
    )
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

fn js_runtime(tools: &Path) -> String {
    let name = if tools.join("deno.exe").is_file() {
        "deno"
    } else {
        "node"
    };
    format!("{name}:{}", tools.join(format!("{name}.exe")).display())
}

fn available_at(dir: &Path) -> bool {
    ["yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe"]
        .iter()
        .all(|file| dir.join(file).is_file())
        && (dir.join("deno.exe").is_file() || dir.join("node.exe").is_file())
}

fn command(
    tools: &Path,
    url: &str,
    output: &Path,
    connections: usize,
    cookies: Option<&Path>,
    options: &MediaOptions,
    speed_limit_bps: u64,
) -> Command {
    let mut command = Command::new(tools.join("yt-dlp.exe"));
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command.args([
        "--ignore-config",
        "--no-cache-dir",
        "--no-playlist",
        "--playlist-items",
        "1",
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
    command.arg("--js-runtimes").arg(js_runtime(tools));
    command.args(source_format_args(url, options));
    command.arg("--concurrent-fragments").arg(
        if speed_limit_bps > 0 {
            1
        } else if connections == 0 {
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
    if speed_limit_bps > 0 {
        command.arg("--limit-rate").arg(speed_limit_bps.to_string());
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
    let url = video_url(&item.url).context("Unsupported video URL")?;
    anyhow::ensure!(
        available_at(&tools),
        "Video tools are missing. Install the full IDG package, then retry."
    );
    let directory = item.save_path.parent().unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(directory).await?;
    let directory = tokio::fs::canonicalize(directory).await?;
    let filename = safe_filename(&item.filename);
    let filename = Path::new(&filename).with_extension(item.media_options.format.extension());
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
        item.num_segments = if item.bandwidth.rate() > 0 {
            1
        } else if item.num_segments == 0 {
            8
        } else {
            item.num_segments.clamp(1, 32)
        };
        item.save_path = output.clone();
        item.filename = output.file_name().unwrap().to_string_lossy().into_owned();
    });
    let mut downloader = command(
        &tools,
        &url,
        &output,
        item.num_segments,
        cookies.as_deref(),
        &item.media_options,
        item.bandwidth.rate(),
    );
    let mut child = downloader
        .spawn()
        .context("Could not start the bundled video downloader")?;
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
            .unwrap_or("Video download failed");
        bail!("{message}");
    }
    let reported_file =
        reported_file.context("Video downloader did not report a completed file")?;
    anyhow::ensure!(
        tokio::fs::canonicalize(&reported_file).await? == tokio::fs::canonicalize(&output).await?,
        "Unexpected video output path"
    );
    let bytes = tokio::fs::metadata(&output).await?.len();
    anyhow::ensure!(bytes > 0, "Video downloader produced an empty file");
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
    #[test]
    fn social_urls_and_watermark_selection_stay_scoped_to_supported_sources() {
        for (input, expected) in [
            (
                "https://instagram.com/reel/Chunk8-jurw/?igsh=abc",
                "https://www.instagram.com/reel/Chunk8-jurw/",
            ),
            (
                "https://www.instagram.com/p/BQ0eAlwhDrw/",
                "https://www.instagram.com/p/BQ0eAlwhDrw/",
            ),
            (
                "https://m.tiktok.com/@patroxofficial/video/6742501081818877190?lang=en",
                "https://www.tiktok.com/@patroxofficial/video/6742501081818877190",
            ),
            (
                "https://vm.tiktok.com/ZMabc123/?share=1",
                "https://vm.tiktok.com/ZMabc123/",
            ),
            (
                "https://www.tiktok.com/t/ZMabc123/",
                "https://www.tiktok.com/t/ZMabc123/",
            ),
        ] {
            assert_eq!(video_url(input).as_deref(), Some(expected));
        }
        for url in [
            "https://instagram.com.evil.test/reel/abc",
            "https://www.instagram.com/accounts/login/",
            "https://www.tiktok.com/@user",
            "https://www.tiktok.com/@user/video/bad",
            "ftp://vm.tiktok.com/abc",
        ] {
            assert!(video_url(url).is_none(), "{url}");
        }
        for format in MediaFormat::ALL {
            let args = source_format_args(
                "https://www.tiktok.com/@user/video/123",
                &MediaOptions {
                    format,
                    ..Default::default()
                },
            );
            assert!(
                args[1]
                    .split('/')
                    .flat_map(|a| a.split('+'))
                    .all(|s| s.ends_with("[format_note!*=?watermarked]"))
            );
        }
        assert_eq!(
            source_format_args(
                "https://www.youtube.com/watch?v=5TBlPQrcweQ",
                &MediaOptions::default()
            ),
            format_args(&MediaOptions::default())
        );
        assert!(
            super::super::music::source("https://open.spotify.com/track/3uCdelPvIjIA0q0V6KRWFY")
                .is_some()
        );
        assert!(unsupported_reason("https://open.spotify.com.evil.test/file.mp3").is_none());
        let info = info_from_json(
            &serde_json::json!({"entries":[{"title":"First video","formats":[{"height":720,"vcodec":"avc1"}]}]}),
        );
        assert_eq!(info.title, "First video");
        assert_eq!(info.heights, vec![720]);
    }

    #[tokio::test]
    #[ignore = "Live Instagram/TikTok extraction, source-selection, download and audio/video probe"]
    async fn public_social_videos_preserve_source_streams() {
        let tools = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/tools");
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/social-smoke")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&directory).unwrap();
        for (site, url) in [
            ("instagram", "https://www.instagram.com/reel/Chunk8-jurw/"),
            (
                "tiktok",
                "https://www.tiktok.com/@patroxofficial/video/6742501081818877190",
            ),
        ] {
            let info = inspect_with_tools(url, None, &tools).await.unwrap();
            assert!(!info.heights.is_empty());
            if site == "tiktok" {
                let mut inspect = Command::new(tools.join("yt-dlp.exe"));
                #[cfg(windows)]
                inspect.creation_flags(0x08000000);
                let selector = source_format_args(url, &MediaOptions::default());
                let fixture = directory.join("watermarked-only.json");
                std::fs::write(&fixture, serde_json::to_vec(&serde_json::json!({
                    "id":"fixture", "title":"Watermarked only", "extractor":"TikTok",
                    "formats":[{"format_id":"download", "format_note":"watermarked", "url":"https://example.test/unused.mp4", "ext":"mp4", "height":720, "vcodec":"h264", "acodec":"aac"}]
                })).unwrap()).unwrap();
                let rejected = Command::new(tools.join("yt-dlp.exe"))
                    .args([
                        "--ignore-config",
                        "--simulate",
                        "--no-check-formats",
                        "--format",
                        &selector[1],
                        "--load-info-json",
                    ])
                    .arg(fixture)
                    .output()
                    .await
                    .unwrap();
                assert!(!rejected.status.success());
                assert!(
                    String::from_utf8_lossy(&rejected.stderr)
                        .contains("Requested format is not available")
                );
                let result = inspect
                    .args([
                        "--ignore-config",
                        "--no-cache-dir",
                        "--dump-single-json",
                        "--skip-download",
                        "--socket-timeout",
                        "15",
                        "--retries",
                        "0",
                        "--format",
                        &selector[1],
                        "--",
                        url,
                    ])
                    .output()
                    .await
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let selected: Value = serde_json::from_slice(&result.stdout).unwrap();
                assert!(
                    !selected["format_note"]
                        .as_str()
                        .unwrap_or("")
                        .contains("watermarked")
                );
                assert_ne!(selected["format_id"], "download");
                println!("TikTok selected source format: {}", selected["format_id"]);
            }
            let output = directory.join(format!("{site}.mp4"));
            let item = super::super::types::DownloadItem::new(url.into(), output.clone(), 0);
            let id = item.id.clone();
            let downloads = Arc::new(Mutex::new(vec![item]));
            download_with_tools(downloads.clone(), id, tools.clone())
                .await
                .unwrap();
            assert_eq!(downloads.lock().unwrap()[0].status, Status::Completed);
            let mut probe = Command::new(tools.join("ffprobe.exe"));
            #[cfg(windows)]
            probe.creation_flags(0x08000000);
            let result = probe
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=codec_type,width,height",
                    "-of",
                    "json",
                ])
                .arg(&output)
                .output()
                .await
                .unwrap();
            assert!(result.status.success());
            let value: Value = serde_json::from_slice(&result.stdout).unwrap();
            let streams = value["streams"].as_array().unwrap();
            assert!(streams.iter().any(|s| s["codec_type"] == "video"));
            if site == "tiktok" {
                assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
            } // The Instagram fixture is silent in the source (all source acodecs are none).
            println!(
                "Verified {}: {} bytes, streams={}, {}",
                site,
                std::fs::metadata(&output).unwrap().len(),
                value,
                output.display()
            );
        }
    }
    #[test]
    fn x_posts_keep_video_selection_and_reject_unsupported_urls() {
        for host in [
            "x.com",
            "www.x.com",
            "twitter.com",
            "www.twitter.com",
            "mobile.twitter.com",
            "m.twitter.com",
        ] {
            assert_eq!(
                video_url(&format!(
                    "https://{host}/Talal1/status/2113571139595510128/video/1?s=20"
                ))
                .as_deref(),
                Some("https://x.com/Talal1/status/2113571139595510128/video/1")
            );
        }
        assert_eq!(
            video_url("https://x.com/i/web/status/2113571139595510128").as_deref(),
            Some("https://x.com/i/web/status/2113571139595510128")
        );
        assert_eq!(
            default_filename("https://x.com/Talal1/status/2113571139595510128/video/1").as_deref(),
            Some("X-2113571139595510128-video-1.mp4")
        );
        for url in [
            "https://x.com/home",
            "https://x.com/user/status/abc",
            "https://x.com/user/status/123/video/0",
            "https://x.com/user/status/123/video/4294967296",
            "https://x.com.evil.test/user/status/123",
            "ftp://x.com/user/status/123",
            "blob:https://x.com/player",
        ] {
            assert!(video_url(url).is_none(), "{url}");
        }
    }

    #[tokio::test]
    #[ignore = "Live X download and audio/video verification using bundled tools"]
    async fn supplied_x_video_downloads_with_audio_and_video() {
        let tools = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/tools");
        let url = "https://x.com/Talal1/status/2113571139595510128/video/1";
        let info = inspect_with_tools(url, None, &tools).await.unwrap();
        assert!(!info.heights.is_empty());
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("x-video.mp4");
        let item = super::super::types::DownloadItem::new(url.into(), output.clone(), 0);
        let id = item.id.clone();
        let downloads = Arc::new(Mutex::new(vec![item]));
        download_with_tools(downloads.clone(), id, tools.clone())
            .await
            .unwrap();
        assert_eq!(downloads.lock().unwrap()[0].status, Status::Completed);
        let mut probe = Command::new(tools.join("ffprobe.exe"));
        #[cfg(windows)]
        probe.creation_flags(0x08000000);
        let result = probe
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
        assert!(result.status.success());
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        let streams = value["streams"].as_array().unwrap();
        assert!(streams.iter().any(|s| s["codec_type"] == "video"));
        assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
        println!(
            "Verified supplied X post: {} bytes, audio and video",
            std::fs::metadata(output).unwrap().len()
        );
    }
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
    fn format_choices_keep_quality_caps_and_audio_conversion() {
        for format in MediaFormat::ALL {
            let options = MediaOptions {
                format,
                height: Some(480),
                audio_bitrate: Some(128),
            };
            let args = format_args(&options);
            if format.audio() {
                assert!(args.iter().any(|arg| arg == "--extract-audio"));
                assert!(args.iter().any(|arg| arg == format.extension()));
                assert!(args.iter().any(|arg| arg == "128K"));
                assert!(!args.iter().any(|arg| arg == "--remux-video"));
            } else {
                assert!(
                    args[1]
                        .split('/')
                        .all(|alternative| alternative.contains("[height<=480]"))
                );
                assert!(args.iter().any(|arg| arg == "--remux-video"));
            }
        }
        let info = info_from_json(&serde_json::json!({"title":"Example", "formats":[
            {"height":1080,"vcodec":"avc1"}, {"height":480,"vcodec":"vp9"},
            {"height":1080,"vcodec":"vp9"}, {"height":2160,"vcodec":"avc1","has_drm":true},
            {"height":90,"vcodec":"none"}]}));
        assert_eq!(info.heights, vec![1080, 480]);
    }

    #[tokio::test]
    #[ignore = "Live YouTube format, conversion, and media probe test"]
    async fn supplied_video_supports_selected_video_quality_and_mp3() {
        let tools = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/tools");
        let url = "https://www.youtube.com/watch?v=5TBlPQrcweQ";
        let info = inspect_with_tools(url, None, &tools).await.unwrap();
        println!("Available video qualities: {:?}", info.heights);
        assert!(!info.heights.is_empty());
        let directory = tempfile::tempdir().unwrap();
        for format in [MediaFormat::Mp4, MediaFormat::Mp3, MediaFormat::Webm] {
            let output = directory
                .path()
                .join(format!("selected.{}", format.extension()));
            let mut item = super::super::types::DownloadItem::new(url.into(), output.clone(), 0);
            item.media_options = MediaOptions {
                format,
                height: Some(480),
                audio_bitrate: Some(128),
            };
            let id = item.id.clone();
            let downloads = Arc::new(Mutex::new(vec![item]));
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
                    "stream=codec_type,codec_name,height",
                    "-of",
                    "json",
                ])
                .arg(&output)
                .output()
                .await
                .unwrap();
            assert!(probe.status.success());
            let value: Value = serde_json::from_slice(&probe.stdout).unwrap();
            let streams = value["streams"].as_array().unwrap();
            assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
            if format.audio() {
                assert!(streams.iter().all(|s| s["codec_type"] != "video"));
                assert!(streams.iter().any(|s| s["codec_name"] == "mp3"));
            } else {
                assert!(streams.iter().any(|s|s["codec_type"] == "video" && s["height"].as_u64().unwrap() <= 480));
            }
            println!("Verified {}: {}", format.extension(), value);
        }
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
            &MediaOptions::default(),
            0,
        );
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|arg| arg == "--no-playlist"));
        assert!(args.iter().any(|arg| arg.contains("100%% video")));
        assert!(!args.iter().any(|arg| arg == "--limit-rate"));
        let limited = command(
            Path::new("tools"),
            "https://youtu.be/5TBlPQrcweQ",
            Path::new("video.mp4"),
            32,
            None,
            &MediaOptions::default(),
            1024,
        );
        let limited: Vec<_> = limited
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let rate = limited
            .iter()
            .position(|arg| arg == "--limit-rate")
            .unwrap();
        assert_eq!(limited[rate + 1], "1024");
        assert!(rate < limited.iter().position(|arg| arg == "--").unwrap());
        let fragments = limited
            .iter()
            .position(|arg| arg == "--concurrent-fragments")
            .unwrap();
        assert_eq!(limited[fragments + 1], "1");
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
