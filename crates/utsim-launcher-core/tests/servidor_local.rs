//! Real HTTP (the `ureq` transport) against a small server on 127.0.0.1 that honours `Range`
//! and drops the first connection halfway through the body.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;

use utsim_launcher_core::Error;
use utsim_launcher_core::download::{Job, download};
use utsim_launcher_core::transport::HttpTransport;

/// Serves `body` at any path; the first response is cut after `cut` bytes of body.
/// Returns the base address and the list of `Range` headers received.
fn serve(
    body: Vec<u8>,
    cut: usize,
    connections: usize,
) -> (String, Arc<Mutex<Vec<Option<String>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = Arc::clone(&seen);
    thread::spawn(move || {
        for (n, stream) in listener.incoming().take(connections).enumerate() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut range = None;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("range:") {
                    range = Some(v.trim().to_string());
                }
            }
            seen2.lock().unwrap().push(range.clone());
            let start: usize = range
                .as_deref()
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.trim_end_matches('-').parse().ok())
                .unwrap_or(0);
            let part = &body[start..];
            let head = if start > 0 {
                format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nConnection: close\r\n\r\n",
                    part.len(),
                    body.len() - 1,
                    body.len()
                )
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    part.len()
                )
            };
            stream.write_all(head.as_bytes()).unwrap();
            let send = if n == 0 {
                &part[..cut.min(part.len())]
            } else {
                part
            };
            let _ = stream.write_all(send);
            let _ = stream.flush();
            // Dropping the stream closes the connection (early, on the first request).
        }
    });
    (addr, seen)
}

#[test]
fn http_download_resumes_after_dropped_connection() {
    let body: Vec<u8> = (0..400_000u32).map(|i| (i * 31 % 256) as u8).collect();
    let (addr, seen) = serve(body.clone(), 150_000, 2);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pacote.utpk");
    let url = format!("{addr}/pacote.utpk");
    let t = HttpTransport::new();
    let stop = AtomicBool::new(false);
    let job = Job {
        url: &url,
        dest: &dest,
        expected_size: Some(body.len() as u64),
        cancel: &stop,
    };

    let r = download(&t, &job, &mut |_| {});
    assert!(matches!(r, Err(Error::Network(_))), "{r:?}");
    let partial = utsim_launcher_core::download::partial_path(&dest);
    let have = std::fs::metadata(&partial).unwrap().len();
    assert!(have > 0 && have <= 150_000, "parcial: {have}");

    download(&t, &job, &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0], None);
    assert_eq!(seen[1].as_deref(), Some(format!("bytes={have}-").as_str()));
}
