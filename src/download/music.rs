//! Original-service music downloads through the optional GPL worker process.
use super::types::{Downloads, Status, safe_filename, with_item};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{LazyLock, atomic::Ordering},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::Semaphore,
};

static JOBS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(1));

struct ProcessTree(Option<u32>);
impl Drop for ProcessTree {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(pid) = self.0 {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("taskkill")
                .creation_flags(0x08000000)
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output();
        }
        #[cfg(not(windows))]
        let _ = self.0;
    }
}

#[derive(Debug, Clone)]
pub struct Source {
    pub service: &'static str,
    pub url: String,
    pub collection: bool,
    pub filename: String,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self.service {
            "spotify" => "Spotify",
            "bandcamp" => "Bandcamp",
            "apple_music" => "Apple Music",
            "deezer" => "Deezer",
            "qobuz" => "Qobuz",
            "tidal" => "Tidal",
            _ => "Music",
        }
    }
}

pub fn source(value: &str) -> Option<Source> {
    let mut url = reqwest::Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?.to_owned();
    let path = url.path().to_owned();
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    let safe = |id: &str| {
        !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    };
    let (service, kind, id, track_query) = if host == "open.spotify.com" {
        let parts = if parts.first().is_some_and(|part| part.starts_with("intl-")) {
            &parts[1..]
        } else {
            &parts[..]
        };
        let [
            kind @ ("track" | "album" | "playlist" | "episode" | "show"),
            id,
        ] = parts
        else {
            return None;
        };
        if id.len() != 22 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        ("spotify", *kind, (*id).to_owned(), None)
    } else if host.ends_with(".bandcamp.com") && host.split('.').count() == 3 {
        let [kind @ ("track" | "album"), id] = parts.as_slice() else {
            return None;
        };
        if !safe(id) {
            return None;
        }
        ("bandcamp", *kind, (*id).to_owned(), None)
    } else if matches!(host.as_str(), "deezer.com" | "www.deezer.com") {
        let parts = if parts.first().is_some_and(|part| part.len() == 2) {
            &parts[1..]
        } else {
            &parts[..]
        };
        let [kind @ ("track" | "album" | "playlist"), id] = parts else {
            return None;
        };
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        url.set_host(Some("www.deezer.com")).ok()?;
        ("deezer", *kind, (*id).to_owned(), None)
    } else if matches!(
        host.as_str(),
        "tidal.com" | "www.tidal.com" | "listen.tidal.com"
    ) {
        let parts = if parts.first() == Some(&"browse") {
            &parts[1..]
        } else {
            &parts[..]
        };
        let [kind @ ("track" | "album" | "playlist"), id] = parts else {
            return None;
        };
        if !safe(id) {
            return None;
        }
        ("tidal", *kind, (*id).to_owned(), None)
    } else if host == "music.apple.com" {
        let kind = *parts.get(1)?;
        if !matches!(kind, "album" | "playlist") || !safe(parts.last()?) {
            return None;
        }
        let track = url
            .query_pairs()
            .find(|(key, _)| key == "i")
            .map(|(_, value)| value.into_owned());
        if track
            .as_ref()
            .is_some_and(|id| id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()))
        {
            return None;
        }
        (
            "apple_music",
            if track.is_some() { "track" } else { kind },
            parts.last()?.to_string(),
            track,
        )
    } else if matches!(
        host.as_str(),
        "qobuz.com" | "www.qobuz.com" | "play.qobuz.com" | "open.qobuz.com"
    ) {
        let kind = parts
            .iter()
            .find(|part| matches!(**part, "track" | "album" | "playlist"))?;
        if !safe(parts.last()?) {
            return None;
        }
        ("qobuz", *kind, parts.last()?.to_string(), None)
    } else {
        return None;
    };
    let collection = matches!(kind, "album" | "playlist" | "show");
    let id: String = id.chars().take(90).collect();
    let filename = safe_filename(&format!(
        "{service}-{kind}-{id}{}",
        if collection { "" } else { ".mp3" }
    ));
    url.set_scheme("https").ok()?;
    if service == "apple_music" {
        url.set_path(&format!("/{}/{}/{}", parts[0], parts[1], id));
    }
    url.set_fragment(None);
    url.set_query(None);
    if let Some(track) = track_query {
        url.query_pairs_mut().append_pair("i", &track);
    }
    Some(Source {
        service,
        url: url.to_string(),
        collection,
        filename,
    })
}

pub fn home() -> PathBuf {
    std::env::var_os("IDG_MUSIC_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::data_local_dir().unwrap_or_default().join("IDG/music"))
}

fn assets() -> PathBuf {
    let installed = std::env::current_exe()
        .unwrap_or_default()
        .with_file_name("music");
    if installed.join("bridge.py").is_file() {
        return installed;
    }
    #[cfg(debug_assertions)]
    return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("music");
    #[cfg(not(debug_assertions))]
    installed
}

pub fn python() -> Option<PathBuf> {
    let value: Value =
        serde_json::from_slice(&std::fs::read(home().join("runtime.json")).ok()?).ok()?;
    if value["protocol"] != 1 {
        return None;
    }
    let pins: Value =
        serde_json::from_slice(&std::fs::read(assets().join("upstream.json")).ok()?).ok()?;
    if value["pins"] != pins {
        return None;
    }
    let path = PathBuf::from(value["python"].as_str()?);
    path.is_file().then_some(path)
}

pub fn ready() -> bool {
    python().is_some()
        && assets().join("bridge.py").is_file()
        && ["ffmpeg.exe", "ffprobe.exe"]
            .iter()
            .all(|name| tools().join(name).is_file())
}

fn tools() -> PathBuf {
    let installed = std::env::current_exe()
        .unwrap_or_default()
        .with_file_name("tools");
    if installed.join("ffprobe.exe").is_file() {
        return installed;
    }
    #[cfg(debug_assertions)]
    return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(target_arch = "x86") {
        "target/i686-pc-windows-msvc/release/tools"
    } else {
        "target/release/tools"
    });
    #[cfg(not(debug_assertions))]
    installed
}

fn command(executable: &Path) -> Command {
    let mut cmd = Command::new(executable);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Account {
    pub service: String,
    pub configured: bool,
}

pub async fn setup() -> Result<()> {
    anyhow::ensure!(
        ["ffmpeg.exe", "ffprobe.exe"]
            .iter()
            .all(|name| tools().join(name).is_file()),
        "Install the complete IDG package with its audio tools before setting up music."
    );
    let script = assets().join("setup.py");
    anyhow::ensure!(
        script.is_file(),
        "Music setup files are missing. Install the complete IDG package."
    );
    let mut cmd = command(Path::new("py"));
    cmd.arg("-3.12")
        .arg(&script)
        .arg("--home")
        .arg(home())
        .arg("--ffmpeg")
        .arg(tools().join("ffmpeg.exe"));
    cmd.stderr(Stdio::piped());
    let output = match cmd.output().await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut cmd = command(Path::new("python"));
            cmd.arg(script)
                .arg("--home")
                .arg(home())
                .arg("--ffmpeg")
                .arg(tools().join("ffmpeg.exe"))
                .stderr(Stdio::piped());
            cmd.output()
                .await
                .context("Install Python 3.12 (64-bit) from python.org, then retry Music setup.")?
        }
        other => other?,
    };
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1200)
            .collect::<String>()
    );
    anyhow::ensure!(ready(), "Music setup did not create a working runtime.");
    Ok(())
}

pub async fn action(action: &str, progress: impl Fn(&str)) -> Result<Vec<Account>> {
    let python =
        python().context("Install the optional music backend in Settings > Music first.")?;
    let mut cmd = command(&python);
    cmd.arg(assets().join("bridge.py"));
    let mut child = cmd.spawn()?;
    let mut process_tree = ProcessTree(child.id());
    let request = json!({"action":action,"home":home(),"ffmpeg":tools().join("ffmpeg.exe")});
    let mut input = child.stdin.take().unwrap();
    input.write_all(format!("{request}\n").as_bytes()).await?;
    drop(input);
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let operation = async {
        let mut accounts = None;
        while let Some(line) = lines.next_line().await? {
            let value: Value =
                serde_json::from_str(&line).context("Invalid music worker response")?;
            anyhow::ensure!(value["idg_music"] == 1, "Unsupported music protocol");
            match value["event"].as_str() {
                Some("error") => bail!(
                    "{}",
                    value["message"].as_str().unwrap_or("Music setup failed")
                ),
                Some("progress") => progress(value["message"].as_str().unwrap_or("Connecting…")),
                Some("status") => {
                    accounts = Some(serde_json::from_value(value["accounts"].clone())?)
                }
                _ => {}
            }
        }
        let status = child.wait().await?;
        process_tree.0 = None;
        anyhow::ensure!(
            status.success(),
            "Music worker could not finish. Check the account connection."
        );
        accounts.context("Music worker did not return account status")
    };
    tokio::time::timeout(
        Duration::from_secs(if action == "pair_spotify" { 210 } else { 7200 }),
        operation,
    )
    .await
    .context("Account setup timed out. Reopen Music settings and retry.")?
}

async fn verify_audio(
    path: &Path,
    tools: &Path,
    expected_ms: Option<f64>,
    format: &str,
) -> Result<()> {
    let mut probe = command(&tools.join("ffprobe.exe"));
    probe
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type:format=duration,format_name",
            "-of",
            "json",
        ])
        .arg(path);
    let output = tokio::time::timeout(Duration::from_secs(30), probe.output()).await??;
    anyhow::ensure!(
        output.status.success(),
        "Downloaded audio could not be verified"
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    anyhow::ensure!(
        value["streams"]
            .as_array()
            .is_some_and(|s| s.iter().any(|s| s["codec_type"] == "audio")),
        "The downloaded file has no audio stream"
    );
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|n| n.parse::<f64>().ok())
        .unwrap_or(0.0);
    anyhow::ensure!(
        duration.is_finite() && duration > 0.05,
        "Downloaded audio has no usable duration"
    );
    if let Some(expected) = expected_ms
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(|n| n / 1000.0)
    {
        anyhow::ensure!(
            (duration - expected).abs() <= (expected * 0.02).max(2.0),
            "Downloaded audio duration does not match the original service track; an incomplete file or preview was rejected"
        );
    }
    let container = value["format"]["format_name"].as_str().unwrap_or("");
    let correct_container = match format {
        "m4a" => container.contains("mp4"),
        "opus" => container.contains("ogg"),
        other => container.split(',').any(|part| part == other),
    };
    anyhow::ensure!(
        correct_container,
        "Downloaded audio encoding does not match the selected output format"
    );
    Ok(())
}

fn validated_files(paths: &Value, root: &Path) -> Result<Vec<PathBuf>> {
    let root = std::fs::canonicalize(root)?;
    let paths = paths.as_array().context("Missing music output manifest")?;
    let mut unique = std::collections::HashSet::new();
    anyhow::ensure!(
        !paths.is_empty() && paths.len() <= 10000,
        "Invalid music output count"
    );
    paths
        .iter()
        .map(|path| {
            let path = std::fs::canonicalize(
                path.as_str()
                    .or_else(|| path["path"].as_str())
                    .context("Invalid music output path")?,
            )?;
            anyhow::ensure!(
                path.starts_with(&root) && path.is_file() && path.metadata()?.len() > 0,
                "Invalid music output path or empty file"
            );
            anyhow::ensure!(unique.insert(path.clone()), "Duplicate music output file");
            Ok(path)
        })
        .collect()
}

pub async fn download(downloads: Downloads, id: String) -> Result<()> {
    let python = python()
        .context("Install the optional backend in Settings > Music, then connect this service.")?;
    download_with(
        downloads,
        id,
        &python,
        &assets().join("bridge.py"),
        &tools(),
        &home(),
    )
    .await
}

async fn download_with(
    downloads: Downloads,
    id: String,
    python: &Path,
    bridge: &Path,
    tool_dir: &Path,
    profile: &Path,
) -> Result<()> {
    let item = downloads
        .lock()
        .unwrap()
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .context("Music job was removed")?;
    let source = source(&item.url).context("Unsupported music URL")?;
    anyhow::ensure!(
        item.bandwidth.rate() == 0,
        "Original-service music currently requires Unlimited speed in Settings > Downloads."
    );
    // One service worker at a time; queued cancellation remains responsive.
    let permit = loop {
        if item.cancel_flag.load(Ordering::Relaxed) {
            with_item(&downloads, &id, |i| i.status = Status::Cancelled);
            return Ok(());
        }
        if let Ok(permit) = JOBS.try_acquire() {
            break permit;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let _permit = permit;
    let parent = item.save_path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let parent = std::fs::canonicalize(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".idg-music-")
        .tempdir_in(&parent)?;
    let output = staging.path().join("audio");
    std::fs::create_dir(&output)?;
    let format = if item.media_options.format.audio() {
        item.media_options.format.extension()
    } else {
        "mp3"
    };
    let name = safe_filename(&item.filename);
    let destination = if source.collection {
        parent.join(name)
    } else {
        parent.join(Path::new(&name).with_extension(format))
    };
    anyhow::ensure!(
        !destination.exists(),
        "A file or folder with this name already exists. Choose another name."
    );
    let request = json!({"action":"download","home":profile,"url":source.url,"service":source.service,
        "output":output,"format":format,"audio_bitrate":item.media_options.audio_bitrate,"ffmpeg":tool_dir.join("ffmpeg.exe")});
    let mut cmd = command(python);
    cmd.arg(bridge);
    let mut child = cmd.spawn().context("Could not start the music backend")?;
    let mut process_tree = ProcessTree(child.id());
    let mut input = child.stdin.take().unwrap();
    input.write_all(format!("{request}\n").as_bytes()).await?;
    drop(input);
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut files = None;
    let started = Instant::now();
    with_item(&downloads, &id, |i| {
        i.status = Status::Probing;
        i.num_segments = 1;
    });
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break; };
                let event: Value = serde_json::from_str(&line).context("Invalid music worker response")?;
                anyhow::ensure!(event["idg_music"] == 1, "Unsupported music protocol");
                match event["event"].as_str() {
                    Some("error") => bail!("{}", event["message"].as_str().unwrap_or("Music download failed")),
                    Some("complete") => { anyhow::ensure!(event["service"] == source.service, "Music source changed unexpectedly"); files = Some(event["files"].clone()); },
                    Some("progress") => with_item(&downloads, &id, |i| {
                        i.status = Status::Downloading;
                        i.downloaded_bytes = event["downloaded"].as_u64().unwrap_or(i.downloaded_bytes);
                        i.speed_bps = i.downloaded_bytes as f64 / started.elapsed().as_secs_f64().max(1.0);
                    }),
                    _ => {}
                }
            }
            _ = tick.tick() => {
                if item.cancel_flag.load(Ordering::Relaxed) {
                    #[cfg(windows)]
                    if let Some(pid) = child.id() { let _ = command(Path::new("taskkill")).args(["/PID", &pid.to_string(), "/T", "/F"]).output().await; }
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    process_tree.0 = None;
                    with_item(&downloads, &id, |i| { i.status = Status::Cancelled; i.speed_bps = 0.0; });
                    return Ok(());
                }
                anyhow::ensure!(started.elapsed() < Duration::from_secs(1900), "Music download timed out");
            }
        }
    }
    let status = child.wait().await?;
    process_tree.0 = None;
    anyhow::ensure!(
        status.success(),
        "Music worker failed. Reconnect the account or retry."
    );
    let manifest = files.context("Music worker did not confirm completed audio files")?;
    let files = validated_files(&manifest, &output)?;
    anyhow::ensure!(
        source.collection || files.len() == 1,
        "The track request returned multiple files"
    );
    for (index, file) in files.iter().enumerate() {
        if item.cancel_flag.load(Ordering::Relaxed) {
            with_item(&downloads, &id, |i| {
                i.status = Status::Cancelled;
                i.speed_bps = 0.0;
            });
            return Ok(());
        }
        verify_audio(
            file,
            tool_dir,
            manifest[index]["duration_ms"].as_f64(),
            format,
        )
        .await?;
    }
    if source.collection {
        std::fs::create_dir(&destination)?;
    }
    let mut bytes = 0;
    for file in files {
        if item.cancel_flag.load(Ordering::Relaxed) {
            with_item(&downloads, &id, |i| {
                i.status = Status::Cancelled;
                i.speed_bps = 0.0;
            });
            return Ok(());
        }
        let target = if source.collection {
            destination.join(file.file_name().unwrap())
        } else {
            destination.clone()
        };
        let mut pending = tempfile::NamedTempFile::new_in(target.parent().unwrap())?;
        let mut source = std::fs::File::open(&file)?;
        bytes += std::io::copy(&mut source, &mut pending)?;
        pending.as_file().sync_all()?;
        if item.cancel_flag.load(Ordering::Relaxed) {
            with_item(&downloads, &id, |i| {
                i.status = Status::Cancelled;
                i.speed_bps = 0.0;
            });
            return Ok(());
        }
        pending
            .persist_noclobber(&target)
            .map_err(|error| error.error)?;
    }
    with_item(&downloads, &id, |i| {
        i.status = Status::Completed;
        i.downloaded_bytes = bytes;
        i.total_bytes = Some(bytes);
        i.speed_bps = 0.0;
        i.save_path = destination.clone();
        i.filename = destination
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        i.completed_at = Some(chrono::Utc::now());
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{media::MediaFormat, types::DownloadItem};
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn music_urls_preserve_source_and_track_selection() {
        for (url, service, collection) in [
            (
                "https://open.spotify.com/track/3uCdelPvIjIA0q0V6KRWFY?si=test",
                "spotify",
                false,
            ),
            (
                "https://open.spotify.com/album/0123456789012345678901",
                "spotify",
                true,
            ),
            (
                "https://benprunty.bandcamp.com/track/lanius-battle",
                "bandcamp",
                false,
            ),
            ("https://www.deezer.com/en/track/123", "deezer", false),
            ("https://listen.tidal.com/album/123", "tidal", true),
            (
                "https://music.apple.com/us/album/example/123?i=456&other=ignored",
                "apple_music",
                false,
            ),
            ("https://play.qobuz.com/album/example", "qobuz", true),
        ] {
            let result = source(url).unwrap();
            assert_eq!(result.service, service);
            assert_eq!(result.collection, collection);
            assert!(!result.url.contains("si="));
        }
        assert!(
            source("https://music.apple.com/us/album/example/123?i=456")
                .unwrap()
                .url
                .ends_with("?i=456")
        );
        for value in [
            "https://open.spotify.com.evil.test/track/3uCdelPvIjIA0q0V6KRWFY",
            "https://artist.bandcamp.com.evil.test/track/song",
            "https://open.spotify.com/track/bad",
            "ftp://listen.tidal.com/track/123",
            "https://user:password@listen.tidal.com/track/123",
            "https://open.spotify.com/",
        ] {
            assert!(source(value).is_none(), "{value}");
        }
    }

    #[test]
    fn manifest_never_accepts_outside_or_empty_files() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let outside = root.path().join("outside.mp3");
        std::fs::write(&outside, b"audio").unwrap();
        let empty = output.join("empty.mp3");
        std::fs::write(&empty, b"").unwrap();
        assert!(validated_files(&json!([outside]), &output).is_err());
        assert!(validated_files(&json!([empty]), &output).is_err());
        assert!(validated_files(&json!([]), &output).is_err());
    }

    #[tokio::test]
    #[ignore = "Requires Python and the pinned ffprobe tools; no service accounts or network"]
    async fn worker_protocol_requires_verified_audio_and_honors_cancel() {
        let directory = tempfile::tempdir().unwrap();
        let bridge = directory.path().join("fixture.py");
        std::fs::write(&bridge, r#"import json,sys,wave,time
from pathlib import Path
r=json.loads(sys.stdin.readline()); key=r['url'].rsplit('/',1)[-1][-1]
root=Path(r['output']); root.mkdir(exist_ok=True)
if key=='4': time.sleep(60)
if key=='3': sys.exit(0)
p=(Path(r['home'])/'outside.wav') if key=='5' else root/'audio.wav'
if key=='2': p.write_bytes(b'<html>not audio</html>')
else:
 with wave.open(str(p),'wb') as f:
  f.setnchannels(1); f.setsampwidth(1); f.setframerate(8000); f.writeframes(bytes([128])*8000)
manifest=[{'path':str(p),'duration_ms':10000}] if key=='6' else [str(p)]
print(json.dumps({'idg_music':1,'event':'complete','service':'spotify','files':manifest}),flush=True)
"#).unwrap();
        for key in ['1', '2', '3', '4', '5', '6', '7'] {
            let output = directory.path().join(format!("result-{key}.wav"));
            let mut item = DownloadItem::new(
                format!("https://open.spotify.com/track/000000000000000000000{key}"),
                output.clone(),
                0,
            );
            item.media_options.format = if key == '7' {
                MediaFormat::Mp3
            } else {
                MediaFormat::Wav
            };
            let cancel = item.cancel_flag.clone();
            let id = item.id.clone();
            let downloads = Arc::new(Mutex::new(vec![item]));
            if key == '4' {
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    cancel.store(true, Ordering::Relaxed);
                });
            }
            let result = download_with(
                downloads.clone(),
                id,
                Path::new("python"),
                &bridge,
                &tools(),
                directory.path(),
            )
            .await;
            if key == '1' {
                result.unwrap();
                assert_eq!(downloads.lock().unwrap()[0].status, Status::Completed);
                assert!(output.metadata().unwrap().len() > 8000);
            } else if key == '4' {
                result.unwrap();
                assert_eq!(downloads.lock().unwrap()[0].status, Status::Cancelled);
                assert!(!output.exists());
            } else {
                assert!(result.is_err(), "{key}");
                assert!(
                    !output
                        .with_extension(if key == '7' { "mp3" } else { "wav" })
                        .exists()
                );
            }
        }
    }

    #[tokio::test]
    #[ignore = "Live original-service Bandcamp download using the installed optional runtime"]
    async fn bandcamp_original_service_audio_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("bandcamp.mp3");
        let item = DownloadItem::new(
            "https://benprunty.bandcamp.com/track/lanius-battle".into(),
            output.clone(),
            0,
        );
        let id = item.id.clone();
        let downloads = Arc::new(Mutex::new(vec![item]));
        download(downloads.clone(), id).await.unwrap();
        assert_eq!(downloads.lock().unwrap()[0].status, Status::Completed);
        println!(
            "Verified original Bandcamp audio: {} bytes",
            output.metadata().unwrap().len()
        );
    }
}
