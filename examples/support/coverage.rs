//! Experimental acknowledged coverage shared by the full stream and helpers.
use crate::download::writer::DownloadWriter;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const CHUNK: u64 = 1024 * 1024;

struct Chunk {
    next: AtomicU64,
    serial: tokio::sync::Mutex<()>,
    stealing: AtomicBool,
}

pub struct StealClaim<'a> {
    chunk: &'a Chunk,
    pub index: usize,
    pub end: u64,
}
impl Drop for StealClaim<'_> {
    fn drop(&mut self) {
        self.chunk.stealing.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn overlapping_streams_acknowledge_each_byte_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("coverage.bin");
        let expected: Vec<u8> = (0..(3 * CHUNK as usize + 19))
            .map(|n| ((n * 17 + n / 127 + n / 65536) % 251) as u8)
            .collect();
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(expected.len() as u64).unwrap();
        let writer = Arc::new(DownloadWriter::new(file).unwrap());
        let coverage = Arc::new(Coverage::new(expected.len() as u64, writer.clone()).unwrap());
        // A helper acknowledges a short prefix before another stream reaches it.
        let start = CHUNK as usize;
        coverage
            .write(start as u64, expected[start..start + 31763].to_vec())
            .await
            .unwrap();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..4 {
            let coverage = coverage.clone();
            let expected = expected.clone();
            tasks.spawn(async move {
                for start in (0..expected.len()).step_by(CHUNK as usize) {
                    let end = (start + CHUNK as usize).min(expected.len());
                    coverage
                        .write(start as u64, expected[start..end].to_vec())
                        .await
                        .unwrap();
                    tokio::task::yield_now().await;
                }
            });
        }
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
        assert_eq!(coverage.bytes(), expected.len() as u64);
        assert!(coverage.complete());
        writer.finish().await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), expected);
    }

    #[tokio::test]
    async fn gaps_and_disk_failures_never_acknowledge_coverage() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("readonly.bin");
        std::fs::write(&path, b"keep").unwrap();
        let writer = Arc::new(DownloadWriter::new(std::fs::File::open(&path).unwrap()).unwrap());
        let coverage = Coverage::new(4, writer.clone()).unwrap();
        assert!(
            coverage
                .write(1, vec![b'x'])
                .await
                .unwrap_err()
                .to_string()
                .contains("gap")
        );
        assert_eq!(coverage.bytes(), 0);
        assert!(
            coverage
                .write(0, vec![b'x'])
                .await
                .unwrap_err()
                .is::<DiskFailure>()
        );
        assert_eq!(coverage.bytes(), 0);
        assert!(!coverage.complete());
        assert!(writer.finish().await.is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"keep");
    }
}

#[derive(Debug)]
pub struct DiskFailure(pub std::io::Error);
impl std::fmt::Display for DiskFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "destination write failed: {}", self.0)
    }
}
impl std::error::Error for DiskFailure {}

pub struct Coverage {
    pub size: u64,
    chunks: Vec<Chunk>,
    file: Arc<DownloadWriter>,
    acknowledged: AtomicU64,
}

impl Coverage {
    pub fn new(size: u64, file: Arc<DownloadWriter>) -> anyhow::Result<Self> {
        let count = usize::try_from(size.div_ceil(CHUNK))?;
        anyhow::ensure!(count <= 1_048_576, "Diagnostic supports at most 1 TiB");
        Ok(Self {
            size,
            chunks: (0..count)
                .map(|i| Chunk {
                    next: AtomicU64::new(i as u64 * CHUNK),
                    serial: tokio::sync::Mutex::new(()),
                    stealing: AtomicBool::new(false),
                })
                .collect(),
            file,
            acknowledged: AtomicU64::new(0),
        })
    }

    pub fn bytes(&self) -> u64 {
        self.acknowledged.load(Ordering::Acquire)
    }
    pub fn complete(&self) -> bool {
        self.bytes() == self.size
    }
    pub fn chunk_end(&self, offset: u64) -> u64 {
        ((offset / CHUNK + 1) * CHUNK).min(self.size)
    }

    pub fn first_missing(&self, first: usize, stop: usize) -> Option<u64> {
        (first..stop).find_map(|i| {
            let next = self.chunks[i].next.load(Ordering::Acquire);
            (next < ((i as u64 + 1) * CHUNK).min(self.size)).then_some(next)
        })
    }

    pub fn covers(&self, start: u64, end: u64) -> bool {
        if start == 0 && end == self.size {
            return self.complete();
        }
        ((start / CHUNK) as usize..end.div_ceil(CHUNK) as usize).all(|index| {
            self.chunks[index].next.load(Ordering::Acquire) >= ((index as u64 + 1) * CHUNK).min(end)
        })
    }

    /// One idle helper can hedge each unfinished chunk. The original group
    /// keeps its valid response; only acknowledged unique writes count.
    pub fn claim_missing(&self) -> Option<StealClaim<'_>> {
        for (index, chunk) in self.chunks.iter().enumerate().rev() {
            let end = ((index as u64 + 1) * CHUNK).min(self.size);
            if chunk.next.load(Ordering::Acquire) < end
                && chunk
                    .stealing
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
            {
                if chunk.next.load(Ordering::Acquire) < end {
                    return Some(StealClaim { chunk, index, end });
                }
                chunk.stealing.store(false, Ordering::Release);
            }
        }
        None
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Every writer supplies an ordered prefix of this chunk. Only the first
    /// acknowledged copy advances coverage; a slower duplicate is trimmed.
    pub async fn write(&self, offset: u64, mut data: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(data);
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| anyhow::anyhow!("Write offset overflow"))?;
        anyhow::ensure!(
            offset < self.size && end <= self.chunk_end(offset),
            "Write crosses coverage boundary"
        );
        let chunk = &self.chunks[(offset / CHUNK) as usize];
        let _serial = chunk.serial.lock().await;
        let next = chunk.next.load(Ordering::Acquire);
        anyhow::ensure!(offset <= next, "Write would leave an unacknowledged gap");
        let skip = (next - offset).min(data.len() as u64) as usize;
        if skip == data.len() {
            data.clear();
            return Ok(data);
        }
        if skip > 0 {
            data.copy_within(skip.., 0);
            data.truncate(data.len() - skip);
        }
        let count = data.len() as u64;
        let mut data = self.file.write_at(next, data).await.map_err(DiskFailure)?;
        chunk.next.store(next + count, Ordering::Release);
        self.acknowledged.fetch_add(count, Ordering::AcqRel);
        data.clear();
        Ok(data)
    }
}
