//! Shared types for the download subsystem.
//!
//! [`DownloadItem`] is the central record that the GUI and download engine
//! both read from and write to via the shared [`Downloads`] mutex.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Instant,
};

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared collection type
// ---------------------------------------------------------------------------

/// Thread-safe, cloneable list of all downloads.
pub type Downloads = Arc<Mutex<Vec<DownloadItem>>>;

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Current lifecycle state of a single download.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Status {
    /// Waiting in queue; no network activity yet.
    Queued,
    /// HEAD probe in flight; determining file size and range support.
    Probing,
    /// One or more segment workers are actively downloading.
    Downloading,
    /// All workers paused by the user; can be resumed.
    Paused,
    /// All bytes written; file is complete.
    Completed,
    /// A non-recoverable error occurred.
    Failed,
    /// Cancelled by the user; partial file may remain.
    Cancelled,
}

impl Status {
    /// Returns `true` while network activity is in progress.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Downloading | Self::Probing)
    }
}

// ---------------------------------------------------------------------------
// DownloadItem
// ---------------------------------------------------------------------------

/// All state for a single download, shared between the GUI and async workers.
///
/// Fields are cheaply `Clone`d so the GUI can snapshot the list each frame
/// without holding the mutex during rendering.
#[derive(Clone)]
pub struct DownloadItem {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Original URL.
    pub url: String,
    pub headers: reqwest::header::HeaderMap,
    /// Token-bearing URL-encoded form, held in memory only.
    pub post_data: Option<String>,
    pub bandwidth: Arc<super::bandwidth::BandwidthLimiter>,
    pub awaiting_confirmation: bool,
    pub media_cookies: Option<String>,
    pub media_options: super::media::MediaOptions,
    /// Resolved filename (from `Content-Disposition` or URL path).
    pub filename: String,
    /// Keep a filename explicitly selected by the user instead of server metadata.
    pub preserve_filename: bool,
    /// Absolute path of the output file.
    pub save_path: PathBuf,
    /// Total file size reported by the server (`Content-Length`), if known.
    pub total_bytes: Option<u64>,
    /// Bytes written so far across all segments.
    pub downloaded_bytes: u64,
    /// Rolling 4-second average speed in bytes per second.
    pub speed_bps: f64,
    /// Current lifecycle state.
    pub status: Status,
    /// Requested parallel connections (0 = automatic); resolved to the actual count after probing.
    pub num_segments: usize,
    /// Human-readable error message, set when `status == Failed`.
    pub error: Option<String>,
    /// When this item was added to the queue.
    pub added_at: DateTime<Utc>,
    /// When the download finished (successfully or not).
    pub completed_at: Option<DateTime<Utc>>,

    /// Setting this to `true` signals all workers to stop immediately.
    pub cancel_flag: Arc<AtomicBool>,
    /// Setting this to `true` pauses all workers; clear to resume.
    pub pause_flag: Arc<AtomicBool>,

    /// Internal rolling window of `(sample_instant, bytes)` for speed calc.
    speed_window: std::collections::VecDeque<(Instant, u64)>,
    speed_window_bytes: u64,
    speed_started: Option<Instant>,
}

impl DownloadItem {
    /// Create a new download item in the [`Status::Queued`] state.
    pub fn new(url: String, mut save_path: PathBuf, num_segments: usize) -> Self {
        if let Some(name) = super::music::source(&url)
            .map(|s| s.filename)
            .or_else(|| super::media::default_filename(&url))
        {
            let current = save_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if matches!(current, "download" | "watch" | "") || !current.contains('.') {
                save_path.set_file_name(name);
            }
        }
        let filename = save_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".to_string());

        Self {
            id: Uuid::new_v4().to_string(),
            url,
            headers: reqwest::header::HeaderMap::new(),
            post_data: None,
            bandwidth: Arc::default(),
            awaiting_confirmation: false,
            media_cookies: None,
            media_options: super::media::MediaOptions::default(),
            filename,
            preserve_filename: true,
            save_path,
            total_bytes: None,
            downloaded_bytes: 0,
            speed_bps: 0.0,
            status: Status::Queued,
            num_segments,
            error: None,
            added_at: Utc::now(),
            completed_at: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            pause_flag: Arc::new(AtomicBool::new(false)),
            speed_window: std::collections::VecDeque::new(),
            speed_window_bytes: 0,
            speed_started: None,
        }
    }

    pub fn supports_pause(&self) -> bool {
        super::media::video_url(&self.url).is_none() && super::music::source(&self.url).is_none()
    }

    /// Download progress as a value in `[0.0, 1.0]`.
    pub fn progress(&self) -> f32 {
        match self.total_bytes {
            Some(total) if total > 0 => (self.downloaded_bytes as f32 / total as f32).min(1.0),
            _ => 0.0,
        }
    }

    /// Estimated seconds remaining, or `None` if not computable.
    pub fn eta_secs(&self) -> Option<f64> {
        if !matches!(self.status, Status::Downloading) {
            return None;
        }
        let total = self.total_bytes? as f64;
        let remaining = (total - self.downloaded_bytes as f64).max(0.0);
        if self.speed_bps > 0.0 {
            Some(remaining / self.speed_bps)
        } else {
            None
        }
    }

    /// Called by segment workers for every chunk received.
    ///
    /// Accumulates bytes and updates the 4-second rolling speed average.
    pub fn record_progress(&mut self, delta_bytes: u64) {
        self.downloaded_bytes += delta_bytes;

        let now = Instant::now();
        let window = std::time::Duration::from_secs(4);
        let cutoff = now - window;

        if self
            .speed_window
            .back()
            .is_some_and(|(time, _)| *time < cutoff)
        {
            self.speed_started = None;
        }
        while self
            .speed_window
            .front()
            .is_some_and(|(time, _)| *time < cutoff)
        {
            self.speed_window_bytes -= self.speed_window.pop_front().unwrap().1;
        }
        if let Some((_, bytes)) = self
            .speed_window
            .back_mut()
            .filter(|(time, _)| now.duration_since(*time).as_millis() < 100)
        {
            *bytes += delta_bytes;
        } else {
            self.speed_window.push_back((now, delta_bytes));
        }
        self.speed_window_bytes += delta_bytes;
        let started = *self.speed_started.get_or_insert(now);
        let elapsed = now
            .duration_since(started)
            .as_secs_f64()
            .clamp(0.25, window.as_secs_f64());
        self.speed_bps = self.speed_window_bytes as f64 / elapsed;
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Apply a mutable closure to a single [`DownloadItem`] by `id`.
///
/// Acquires the lock, finds the matching item, runs `f`, then releases the lock.
/// Does nothing if the `id` is not found.
pub fn with_item<F>(downloads: &Downloads, id: &str, f: F)
where
    F: FnOnce(&mut DownloadItem),
{
    if let Ok(mut guard) = downloads.lock()
        && let Some(item) = guard.iter_mut().find(|i| i.id == id)
    {
        f(item);
    }
}

/// Keep browser and server supplied names inside the selected directory.
pub fn safe_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("download");
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches(['.', ' ']);
    if clean.is_empty() {
        "download".into()
    } else {
        let stem = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            format!("_{clean}")
        } else {
            clean.into()
        }
    }
}

#[cfg(test)]
mod filename_tests {
    use super::*;
    #[test]
    fn strips_browser_paths_and_traversal() {
        assert_eq!(safe_filename(r"C:\Users\me\Downloads\file.zip"), "file.zip");
        assert_eq!(safe_filename("../../file.zip"), "file.zip");
        assert_eq!(safe_filename(".."), "download");
        assert_eq!(safe_filename("CON.txt"), "_CON.txt");
        assert_eq!(safe_filename("bad:name.zip"), "bad_name.zip");
    }
}
