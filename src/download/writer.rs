//! Bounded disk writer shared by a download's range workers.
//!
//! One OS thread performs synchronous seek/write calls, avoiding two Tokio
//! blocking-pool round trips while holding a global file mutex for every batch.
//! An acknowledgement advances a worker's retry cursor only after its write.
use std::{
    fs::File,
    io::{self, Write},
};
use tokio::sync::{mpsc, oneshot};

enum Request {
    Write {
        offset: u64,
        bytes: Vec<u8>,
        reply: oneshot::Sender<(Vec<u8>, io::Result<()>)>,
    },
    Finish {
        reply: oneshot::Sender<io::Result<()>>,
    },
}

pub struct DownloadWriter {
    sender: mpsc::Sender<Request>,
}

impl DownloadWriter {
    pub fn new(mut file: File) -> io::Result<Self> {
        let (sender, mut receiver) = mpsc::channel::<Request>(8);
        std::thread::Builder::new()
            .name("idg-disk-writer".into())
            .spawn(move || {
                let mut failure: Option<(io::ErrorKind, String)> = None;
                while let Some(request) = receiver.blocking_recv() {
                    match request {
                        Request::Write {
                            offset,
                            bytes,
                            reply,
                        } => {
                            let result = if let Some((kind, message)) = &failure {
                                Err(io::Error::new(*kind, message.clone()))
                            } else {
                                write_range(&mut file, offset, &bytes)
                            };
                            if let Err(error) = &result {
                                failure = Some((error.kind(), error.to_string()));
                            }
                            let _ = reply.send((bytes, result));
                        }
                        Request::Finish { reply } => {
                            let result = if let Some((kind, message)) = failure {
                                Err(io::Error::new(kind, message))
                            } else {
                                file.flush().and_then(|_| file.sync_all())
                            };
                            let _ = reply.send(result);
                            break;
                        }
                    }
                }
            })?;
        Ok(Self { sender })
    }

    pub async fn write_at(&self, offset: u64, bytes: Vec<u8>) -> io::Result<Vec<u8>> {
        let (reply, result) = oneshot::channel();
        self.sender
            .send(Request::Write {
                offset,
                bytes,
                reply,
            })
            .await
            .map_err(|_| closed())?;
        let (bytes, outcome) = result.await.map_err(|_| closed())?;
        outcome?;
        Ok(bytes)
    }

    pub async fn finish(&self) -> io::Result<()> {
        let (reply, result) = oneshot::channel();
        self.sender
            .send(Request::Finish { reply })
            .await
            .map_err(|_| closed())?;
        result.await.map_err(|_| closed())?
    }
}

fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "Download disk writer stopped")
}

#[cfg(windows)]
fn write_range(file: &mut File, mut offset: u64, mut bytes: &[u8]) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    // Only this writer thread owns the handle, so seek_write's cursor effects
    // cannot race another write. Each call carries the destination offset.
    while !bytes.is_empty() {
        match file.seek_write(bytes, offset) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Disk write made no progress",
                ));
            }
            Ok(written) => {
                offset += written as u64;
                bytes = &bytes[written..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn write_range(file: &mut File, offset: u64, bytes: &[u8]) -> io::Result<()> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)
}

/// Best effort on NTFS/ReFS. Unsupported filesystems keep normal allocation.
/// This never uses SetFileValidData or exposes uninitialized disk contents.
pub fn prepare_sparse(file: &tokio::fs::File) {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::{IO::DeviceIoControl, Ioctl::FSCTL_SET_SPARSE};
        let mut returned = 0;
        // SAFETY: the live synchronous file handle is exclusively owned by the
        // orchestrator at setup. Null input sets sparse; returned is valid DWORD.
        unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                FSCTL_SET_SPARSE,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            );
        }
    }
    #[cfg(not(windows))]
    let _ = file;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn concurrent_ranges_are_acknowledged_at_exact_offsets_and_synced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.bin");
        let file = File::create(&path).unwrap();
        file.set_len(32 * 65537).unwrap();
        let writer = Arc::new(DownloadWriter::new(file).unwrap());
        let mut tasks = Vec::new();
        for n in 0..32 {
            let writer = writer.clone();
            tasks.push(tokio::spawn(async move {
                let data = vec![n as u8; 65537];
                assert_eq!(
                    writer.write_at(n * 65537, data.clone()).await.unwrap(),
                    data
                );
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        writer.finish().await.unwrap();
        let bytes = std::fs::read(path).unwrap();
        for n in 0..32 {
            assert!(
                bytes[n * 65537..(n + 1) * 65537]
                    .iter()
                    .all(|v| *v == n as u8)
            );
        }
    }

    #[tokio::test]
    async fn disk_errors_propagate_without_acknowledging_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("readonly.bin");
        std::fs::write(&path, b"keep").unwrap();
        let writer = DownloadWriter::new(File::open(&path).unwrap()).unwrap();
        assert!(writer.write_at(0, b"replace".to_vec()).await.is_err());
        assert!(writer.finish().await.is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"keep");
    }
}
