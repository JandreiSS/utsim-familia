//! HTTP GET behind a trait, so downloads are tested without the network.
//!
//! [`HttpTransport`] is the real one (blocking `ureq` with rustls, follows redirects);
//! [`MemoryTransport`] serves bytes from a table, honours `Range` and can cut a body short
//! to simulate a dropped connection.

use std::collections::HashMap;
use std::io::{self, Read};
use std::sync::Mutex;
use std::time::Duration;

use crate::{Error, Result};

/// An HTTP response with a streaming body.
pub struct Response {
    pub status: u16,
    /// Header names in lowercase.
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn Read + Send>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    /// `true` when the server sent a web page instead of a file.
    pub fn is_html(&self) -> bool {
        self.header("content-type")
            .is_some_and(|c| c.to_ascii_lowercase().starts_with("text/html"))
    }

    /// Reads the whole body as text, up to `limit` bytes.
    pub fn text(self, limit: u64) -> Result<String> {
        let mut s = String::new();
        self.body
            .take(limit)
            .read_to_string(&mut s)
            .map_err(|e| Error::Network(format!("leitura da resposta: {e}")))?;
        Ok(s)
    }
}

/// A GET with an optional `Range: bytes=<from>-`.
pub trait Transport: Send + Sync {
    fn get(&self, url: &str, range_from: Option<u64>) -> Result<Response>;
}

/// The real transport.
pub struct HttpTransport {
    agent: ureq::Agent,
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(120))
            .redirects(10)
            .user_agent(concat!("utsim-launcher/", env!("CARGO_PKG_VERSION")))
            .build();
        Self { agent }
    }
}

/// Path of a `file://` address (`file:///C:/pasta/x` or `file:///home/x`), with `%20` and
/// other escapes decoded.
pub fn file_url_path(url: &str) -> Option<std::path::PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // `file:///C:/x` is `C:/x` on Windows; `file:///home/x` is `/home/x` elsewhere.
    let rest = match rest.strip_prefix('/') {
        Some(r) if r.len() > 1 && r.as_bytes()[1] == b':' => r,
        _ => rest,
    };
    let mut bytes = Vec::with_capacity(rest.len());
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    String::from_utf8(bytes).ok().map(std::path::PathBuf::from)
}

/// `file://` address of a local path.
pub fn file_url(path: &std::path::Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.replace(' ', "%20");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn serve_file(path: &std::path::Path, range_from: Option<u64>) -> Result<Response> {
    use std::io::{Seek, SeekFrom};
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Response {
                status: 404,
                headers: Vec::new(),
                body: Box::new(io::empty()),
            });
        }
        Err(e) => return Err(e.into()),
    };
    let total = f.metadata()?.len();
    let mut headers = vec![(
        "content-type".to_string(),
        "application/octet-stream".to_string(),
    )];
    let status = match range_from {
        Some(from) if from > 0 && from < total => {
            f.seek(SeekFrom::Start(from))?;
            headers.push((
                "content-range".into(),
                format!("bytes {from}-{}/{total}", total - 1),
            ));
            headers.push(("content-length".into(), (total - from).to_string()));
            206
        }
        _ => {
            headers.push(("content-length".into(), total.to_string()));
            200
        }
    };
    Ok(Response {
        status,
        headers,
        body: Box::new(io::BufReader::new(f)),
    })
}

impl Transport for HttpTransport {
    /// Also reads `file://` addresses, so the author can test the launcher against the local
    /// publication folder before anything is uploaded.
    fn get(&self, url: &str, range_from: Option<u64>) -> Result<Response> {
        if let Some(path) = file_url_path(url) {
            return serve_file(&path, range_from);
        }
        let mut req = self.agent.get(url);
        if let Some(from) = range_from {
            req = req.set("Range", &format!("bytes={from}-"));
        }
        let resp = match req.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => return Err(Error::Network(t.to_string())),
        };
        let status = resp.status();
        let headers = resp
            .headers_names()
            .into_iter()
            .filter_map(|n| {
                resp.header(&n)
                    .map(|v| (n.to_ascii_lowercase(), v.to_string()))
            })
            .collect();
        Ok(Response {
            status,
            headers,
            body: Box::new(resp.into_reader()),
        })
    }
}

/// A canned response of [`MemoryTransport`].
#[derive(Clone)]
pub struct Canned {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    /// Honour `Range` requests (answer 206).
    pub ranges: bool,
}

impl Canned {
    pub fn file(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type: "application/octet-stream".into(),
            body,
            ranges: true,
        }
    }

    pub fn html(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8".into(),
            body: body.as_bytes().to_vec(),
            ranges: false,
        }
    }
}

/// In-memory transport for tests and offline previews.
#[derive(Default)]
pub struct MemoryTransport {
    routes: Mutex<HashMap<String, Canned>>,
    /// Bytes still allowed before the next body is cut (`None`: never cut).
    cut_after: Mutex<Option<u64>>,
    /// Every request made: (url, range start).
    pub log: Mutex<Vec<(String, Option<u64>)>>,
}

impl MemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, url: &str, canned: Canned) {
        if let Ok(mut r) = self.routes.lock() {
            r.insert(url.to_string(), canned);
        }
    }

    /// The next file (non-HTML) body breaks after `n` bytes, like a dropped connection.
    pub fn cut_next_after(&self, n: u64) {
        if let Ok(mut c) = self.cut_after.lock() {
            *c = Some(n);
        }
    }

    pub fn requests(&self) -> Vec<(String, Option<u64>)> {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

struct Cut {
    data: io::Cursor<Vec<u8>>,
    left: u64,
}

impl Read for Cut {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "conexão interrompida (simulada)",
            ));
        }
        let max = buf
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.data.read(&mut buf[..max])?;
        if n == 0 {
            return Ok(0);
        }
        self.left -= n as u64;
        Ok(n)
    }
}

impl Transport for MemoryTransport {
    fn get(&self, url: &str, range_from: Option<u64>) -> Result<Response> {
        if let Ok(mut l) = self.log.lock() {
            l.push((url.to_string(), range_from));
        }
        let canned = self
            .routes
            .lock()
            .ok()
            .and_then(|r| r.get(url).cloned())
            .ok_or_else(|| Error::Network(format!("404 em {url}")))?;
        let total = canned.body.len() as u64;
        let (status, body, mut headers) = match range_from {
            Some(from) if canned.ranges && from > 0 && from < total => (
                206,
                canned.body[from as usize..].to_vec(),
                vec![(
                    "content-range".to_string(),
                    format!("bytes {from}-{}/{total}", total - 1),
                )],
            ),
            _ => (canned.status, canned.body, Vec::new()),
        };
        let is_page = canned.content_type.starts_with("text/html");
        headers.push(("content-type".into(), canned.content_type));
        headers.push(("content-length".into(), body.len().to_string()));
        let cut = if is_page {
            None
        } else {
            self.cut_after.lock().ok().and_then(|mut c| c.take())
        };
        let body: Box<dyn Read + Send> = match cut {
            Some(n) => Box::new(Cut {
                data: io::Cursor::new(body),
                left: n,
            }),
            None => Box::new(io::Cursor::new(body)),
        };
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls() {
        let p = file_url_path("file:///C:/Meu%20Drive/x.json").unwrap();
        assert_eq!(p, std::path::PathBuf::from("C:/Meu Drive/x.json"));
        let p = file_url_path("file:///home/a/x.json").unwrap();
        assert_eq!(p, std::path::PathBuf::from("/home/a/x.json"));
        assert_eq!(
            file_url(std::path::Path::new(r"C:\Meu Drive\m.json")),
            "file:///C:/Meu%20Drive/m.json"
        );
        assert!(file_url_path("https://x").is_none());
    }

    #[test]
    fn local_files_are_served_with_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a b.bin");
        std::fs::write(&path, b"0123456789").unwrap();
        let t = HttpTransport::new();
        let url = file_url(&path);
        let r = t.get(&url, Some(4)).unwrap();
        assert_eq!(r.status, 206);
        assert_eq!(r.text(100).unwrap(), "456789");
        assert_eq!(t.get(&url, None).unwrap().status, 200);
        assert_eq!(t.get(&format!("{url}.nao"), None).unwrap().status, 404);
    }
}
