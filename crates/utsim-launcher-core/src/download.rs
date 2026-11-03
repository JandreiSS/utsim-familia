//! Resumable download to a file.
//!
//! Bytes go to `<destination>.parcial`; a later call continues from its length with
//! `Range: bytes=<n>-`. If the server ignores the range (status 200) the file restarts from
//! zero. When Drive answers with its virus-scan page, the confirmation address is followed
//! (also for the resumed request). The partial file is renamed to the destination only when
//! complete; checking the SHA-256 is the caller's job ([`crate::install`]).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::transport::Transport;
use crate::{Error, Result, drive};

/// How far a download went: bytes on disk and the expected total, if known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
}

/// Path of the partial file of `dest`.
pub fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".parcial");
    dest.with_file_name(name)
}

/// HTML pages are small: anything larger than this is not a confirmation page.
const PAGE_LIMIT: u64 = 2 << 20;
/// Confirmation pages and restarts followed before giving up.
const MAX_HOPS: usize = 4;

/// One file to fetch.
#[derive(Clone, Copy)]
pub struct Job<'a> {
    pub url: &'a str,
    pub dest: &'a Path,
    /// Size announced by the manifest.
    pub expected_size: Option<u64>,
    /// Set to stop between blocks, keeping the partial file.
    pub cancel: &'a AtomicBool,
}

/// Downloads `url` into `dest`, resuming a previous partial file. `expected_size` (from the
/// manifest) lets a finished partial file be accepted without a request and catches a
/// server that sends the wrong thing. `cancel` stops between blocks, keeping the partial
/// file. `progress` is called after each block.
pub fn download(
    transport: &dyn Transport,
    job: &Job<'_>,
    progress: &mut dyn FnMut(Progress),
) -> Result<()> {
    let Job {
        url,
        dest,
        expected_size,
        cancel,
    } = *job;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = partial_path(dest);
    let mut have = fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
    if let Some(size) = expected_size {
        if have > size {
            // Not the file we expect: start again.
            File::create(&partial)?;
            have = 0;
        } else if have == size && size > 0 {
            progress(Progress {
                done: have,
                total: Some(size),
            });
            fs::rename(&partial, dest)?;
            return Ok(());
        }
    }

    let mut url = drive::direct_url(url);
    let mut hops = 0;
    loop {
        hops += 1;
        if hops > MAX_HOPS {
            return Err(Error::Drive(
                "muitas páginas de confirmação seguidas; tente mais tarde".into(),
            ));
        }
        let range = (have > 0).then_some(have);
        let resp = transport.get(&url, range)?;
        if resp.is_html() {
            let page = resp.text(PAGE_LIMIT)?;
            match drive::interpret(&page, &url) {
                drive::Page::Confirm(next) => {
                    url = next;
                    continue;
                }
                other if drive::is_drive_url(&url) => {
                    return Err(Error::Drive(other.message().into()));
                }
                _ => {
                    return Err(Error::Network(
                        "o servidor devolveu uma página da web em vez do arquivo".into(),
                    ));
                }
            }
        }
        let mut file = match resp.status {
            206 => {
                let start = resp
                    .header("content-range")
                    .and_then(range_start)
                    .unwrap_or(u64::MAX);
                if start != have {
                    // A range we did not ask for: start over.
                    have = 0;
                    File::create(&partial)?;
                    continue;
                }
                OpenOptions::new().append(true).open(&partial)?
            }
            200 => {
                have = 0;
                File::create(&partial)?
            }
            416 if have > 0 => {
                // The partial file is not a prefix of this file (or already complete but of an
                // unknown size): start over.
                have = 0;
                File::create(&partial)?;
                continue;
            }
            404 => return Err(Error::Network("arquivo não encontrado (HTTP 404)".into())),
            s => return Err(Error::Network(format!("o servidor respondeu HTTP {s}"))),
        };
        let total = expected_size.or_else(|| {
            resp.header("content-length")
                .and_then(|l| l.trim().parse::<u64>().ok())
                .map(|l| l + have)
        });
        let mut body = resp.body;
        let mut buf = vec![0u8; 1 << 16];
        let mut done = have;
        progress(Progress { done, total });
        loop {
            if cancel.load(Ordering::Relaxed) {
                file.flush()?;
                return Err(Error::Cancelled);
            }
            let n = match body.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    file.flush()?;
                    return Err(Error::Network(format!(
                        "download interrompido em {done} bytes ({e}); ele continua de onde parou na próxima tentativa"
                    )));
                }
            };
            file.write_all(&buf[..n])?;
            done += n as u64;
            if let Some(size) = expected_size {
                if done > size {
                    drop(file);
                    let _ = fs::remove_file(&partial);
                    return Err(Error::Integrity(
                        "o servidor enviou mais bytes do que o manifesto anuncia".into(),
                    ));
                }
            }
            progress(Progress { done, total });
        }
        file.sync_all()?;
        drop(file);
        if let Some(size) = expected_size {
            if done < size {
                return Err(Error::Network(format!(
                    "download incompleto ({done} de {size} bytes); tente de novo para continuar"
                )));
            }
        }
        fs::rename(&partial, dest)?;
        return Ok(());
    }
}

/// [`download`] with up to `attempts` tries on network errors, waiting `pause` between them;
/// each try resumes the previous one.
pub fn download_with_retries(
    transport: &dyn Transport,
    job: &Job<'_>,
    progress: &mut dyn FnMut(Progress),
    attempts: u32,
    pause: Duration,
) -> Result<()> {
    let mut last = Error::Network("nenhuma tentativa".into());
    for i in 0..attempts.max(1) {
        if i > 0 {
            std::thread::sleep(pause);
        }
        match download(transport, job, progress) {
            Ok(()) => return Ok(()),
            Err(e @ Error::Network(_)) => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// Start of `Content-Range: bytes <start>-<end>/<total>`.
fn range_start(v: &str) -> Option<u64> {
    v.trim()
        .strip_prefix("bytes")?
        .trim()
        .split('-')
        .next()?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{Canned, MemoryTransport};

    fn data(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 253) as u8).collect()
    }

    #[test]
    fn resumes_after_a_cut() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("p.utpk");
        let t = MemoryTransport::new();
        let body = data(300_000);
        t.add("http://x/p", Canned::file(body.clone()));
        let stop = AtomicBool::new(false);

        t.cut_next_after(100_000);
        let job = Job {
            url: "http://x/p",
            dest: &dest,
            expected_size: Some(300_000),
            cancel: &stop,
        };
        let r = download(&t, &job, &mut |_| {});
        assert!(matches!(r, Err(Error::Network(_))), "{r:?}");
        assert_eq!(fs::metadata(partial_path(&dest)).unwrap().len(), 100_000);

        let mut last = None;
        download(&t, &job, &mut |p| last = Some(p)).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!partial_path(&dest).exists());
        assert_eq!(last.unwrap().done, 300_000);
        assert_eq!(t.requests()[1].1, Some(100_000));
    }

    #[test]
    fn server_without_ranges_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("p.bin");
        let t = MemoryTransport::new();
        let body = data(5000);
        let mut c = Canned::file(body.clone());
        c.ranges = false;
        t.add("http://x/p", c);
        fs::write(partial_path(&dest), &body[..1234]).unwrap();
        let stop = AtomicBool::new(false);
        let job = Job {
            url: "http://x/p",
            dest: &dest,
            expected_size: Some(5000),
            cancel: &stop,
        };
        download(&t, &job, &mut |_| {}).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
    }

    #[test]
    fn oversized_and_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("p.bin");
        let t = MemoryTransport::new();
        t.add("http://x/p", Canned::file(data(5000)));
        let stop = AtomicBool::new(false);
        let mut job = Job {
            url: "http://x/p",
            dest: &dest,
            expected_size: Some(4000),
            cancel: &stop,
        };
        let r = download(&t, &job, &mut |_| {});
        assert!(matches!(r, Err(Error::Integrity(_))));
        let stop = AtomicBool::new(true);
        job.expected_size = Some(5000);
        job.cancel = &stop;
        let r = download(&t, &job, &mut |_| {});
        assert!(matches!(r, Err(Error::Cancelled)));
    }

    #[test]
    fn retries_resume() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("p.bin");
        let t = MemoryTransport::new();
        let body = data(70_000);
        t.add("http://x/p", Canned::file(body.clone()));
        t.cut_next_after(65_536);
        let stop = AtomicBool::new(false);
        let job = Job {
            url: "http://x/p",
            dest: &dest,
            expected_size: Some(70_000),
            cancel: &stop,
        };
        download_with_retries(&t, &job, &mut |_| {}, 3, Duration::ZERO).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
    }

    #[test]
    fn content_range_parsing() {
        assert_eq!(range_start("bytes 100-199/200"), Some(100));
        assert_eq!(range_start("bytes */200"), None);
    }
}
