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
#[derive(Debug, Clone, PartialEq)]
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
    pub awaiting_confirmation: bool,
    /// Resolved filename (from `Content-Disposition` or URL path).
    pub filename: String,
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
    speed_window: Vec<(Instant, u64)>,
}

impl DownloadItem {
    /// Create a new download item in the [`Status::Queued`] state.
    pub fn new(url: String, save_path: PathBuf, num_segments: usize) -> Self {
        let filename = save_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".to_string());

        Self {
            id: Uuid::new_v4().to_string(),
            url,
            headers: reqwest::header::HeaderMap::new(),
            awaiting_confirmation: false,
            filename,
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
            speed_window: Vec::new(),
        }
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

        self.speed_window.push((now, delta_bytes));
        self.speed_window.retain(|(t, _)| *t >= cutoff);

        let total_in_window: u64 = self.speed_window.iter().map(|(_, b)| b).sum();
        self.speed_bps = total_in_window as f64 / window.as_secs_f64();
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
