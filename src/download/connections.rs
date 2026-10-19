//! Automatic connection planning and durable settings shared with browser requests.
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub const MAX_CONNECTIONS: usize = 32;
const MIN_SEGMENT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ConnectionSettings {
    /// None means automatic; manual counts are restricted to 1..=32.
    pub parallel_segments: Option<usize>,
}

pub type SharedConnectionSettings = Arc<Mutex<ConnectionSettings>>;

impl ConnectionSettings {
    pub fn requested_count(&self) -> usize {
        self.parallel_segments
            .map_or(0, |count| count.clamp(1, MAX_CONNECTIONS))
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = settings_path();
        match std::fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&settings_path())
    }

    fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}

fn settings_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("IDG")
        .join("connections.json")
}

/// Zero requests automatic planning. Unknown sizes and non-range servers use one stream.
pub fn choose_count(requested: usize, total: Option<u64>, supports_ranges: bool) -> usize {
    let Some(size) = total.filter(|size| *size > 0 && supports_ranges) else {
        return 1;
    };
    let maximum = if requested == 0 {
        (size / MIN_SEGMENT_BYTES).clamp(1, MAX_CONNECTIONS as u64) as usize
    } else {
        requested.clamp(1, MAX_CONNECTIONS)
    };
    maximum.min(size.min(MAX_CONNECTIONS as u64) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_scales_without_tiny_ranges_and_manual_counts_remain_explicit() {
        assert_eq!(choose_count(0, Some(1_000_000), true), 1);
        assert_eq!(choose_count(0, Some(16 * 1024 * 1024), true), 8);
        assert_eq!(choose_count(0, Some(64 * 1024 * 1024), true), 32);
        assert_eq!(choose_count(0, Some(u64::MAX), true), 32);
        assert_eq!(choose_count(16, Some(3), true), 3);
        assert_eq!(choose_count(12, Some(64 * 1024 * 1024), true), 12);
        assert_eq!(choose_count(32, None, true), 1);
        assert_eq!(choose_count(32, Some(64 * 1024 * 1024), false), 1);
        assert_eq!(choose_count(0, Some(0), true), 1);
    }

    #[test]
    fn settings_survive_multiple_saves_and_restart() {
        let directory = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let path = directory.join("connections.json");
        ConnectionSettings {
            parallel_segments: Some(12),
        }
        .save_to(&path)
        .unwrap();
        let restored: ConnectionSettings =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored.requested_count(), 12);
        ConnectionSettings::default().save_to(&path).unwrap();
        let restored: ConnectionSettings =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored.requested_count(), 0);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
