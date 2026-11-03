//! Google Drive links and the "can't scan for viruses" confirmation page.
//!
//! A file shared as "anyone with the link" is downloaded from
//! `https://drive.usercontent.google.com/download?id=<ID>&export=download`. Files larger
//! than about 100 MB answer that address with an HTML page (status 200) instead of the file:
//! Drive cannot scan them for viruses and asks for confirmation. The current page (2024–2026)
//! carries a form
//!
//! ```html
//! <form id="download-form" action="https://drive.usercontent.google.com/download" method="get">
//!   <input type="hidden" name="id" value="…"> <input type="hidden" name="export" value="download">
//!   <input type="hidden" name="confirm" value="t"> <input type="hidden" name="uuid" value="…">
//! </form>
//! ```
//!
//! and submitting it (a GET with those fields) returns the file. Older pages had a link
//! `/uc?export=download&confirm=<token>&id=<ID>`; both are understood. Other pages that may
//! come back instead of the file (quota exceeded, sign-in required, file removed) become
//! clear messages. The parser is deliberately tolerant: Google changes this page now and then.

use std::collections::HashMap;

/// What an HTML answer from Drive means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    /// Virus-scan warning: download from this address instead.
    Confirm(String),
    /// "Too many users have viewed or downloaded this file recently".
    QuotaExceeded,
    /// The file is not shared publicly (a sign-in or "request access" page).
    NeedsPermission,
    /// The file does not exist (deleted, wrong link).
    NotFound,
    /// Any other page.
    Unknown,
}

impl Page {
    /// Message for the tester, for pages that are not a confirmation.
    pub fn message(&self) -> &'static str {
        match self {
            Page::Confirm(_) => "página de confirmação",
            Page::QuotaExceeded => {
                "o Google Drive limitou os downloads deste arquivo por excesso de acessos recentes; tente de novo em algumas horas"
            }
            Page::NeedsPermission => {
                "o arquivo não está compartilhado como \"qualquer pessoa com o link\" (avise o autor)"
            }
            Page::NotFound => "o arquivo não existe mais no Drive (avise o autor)",
            Page::Unknown => {
                "o Drive devolveu uma página em vez do arquivo (formato desconhecido; avise o autor)"
            }
        }
    }
}

/// Download address for a Drive file id.
pub fn download_url(id: &str) -> String {
    format!("https://drive.usercontent.google.com/download?id={id}&export=download")
}

/// `true` for addresses served by Google Drive.
pub fn is_drive_url(url: &str) -> bool {
    let host = url
        .split("://")
        .nth(1)
        .and_then(|r| r.split(['/', '?']).next())
        .unwrap_or("");
    host == "drive.google.com"
        || host == "drive.usercontent.google.com"
        || host == "docs.google.com"
}

fn is_id(s: &str) -> bool {
    s.len() >= 20
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Extracts the file id from a sharing link (`…/file/d/<ID>/view?usp=sharing`,
/// `…/open?id=<ID>`, `…/uc?id=<ID>`, `…/download?id=<ID>`) or from a bare id.
pub fn file_id(link: &str) -> Option<String> {
    let link = link.trim();
    if is_id(link) {
        return Some(link.to_string());
    }
    if !is_drive_url(link) {
        return None;
    }
    if let Some(rest) = link.split("/d/").nth(1) {
        let id: String = rest.split(['/', '?', '#']).next().unwrap_or("").to_string();
        if is_id(&id) {
            return Some(id);
        }
    }
    let query = link.split_once('?')?.1;
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "id")
        .map(|(_, v)| v.split('#').next().unwrap_or("").to_string())
        .filter(|v| is_id(v))
}

/// Turns a sharing link into a direct download address. Other addresses pass unchanged.
pub fn direct_url(link: &str) -> String {
    if is_drive_url(link) && !link.contains("export=download") {
        if let Some(id) = file_id(link) {
            return download_url(&id);
        }
    }
    link.to_string()
}

/// Classifies an HTML page Drive sent instead of a file. `page_url` is the address that
/// returned it (to resolve relative links).
pub fn interpret(html: &str, page_url: &str) -> Page {
    if let Some(url) = confirm_form(html, page_url).or_else(|| legacy_link(html, page_url)) {
        return Page::Confirm(url);
    }
    let lower = html.to_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if any(&[
        "too many users have viewed or downloaded",
        "quota exceeded",
        "cota de download",
        "muitos usuários visualizaram ou fizeram download",
    ]) {
        Page::QuotaExceeded
    } else if any(&[
        "accounts.google.com/servicelogin",
        "accounts.google.com/v3/signin",
        "you need access",
        "você precisa de acesso",
        "request access",
        "solicitar acesso",
    ]) {
        Page::NeedsPermission
    } else if any(&[
        "sorry, the file you have requested does not exist",
        "o arquivo solicitado não existe",
        "error 404",
        "erro 404",
    ]) {
        Page::NotFound
    } else {
        Page::Unknown
    }
}

fn confirm_form(html: &str, page_url: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<form") {
        let start = pos + i;
        let open_end = start + lower[start..].find('>')?;
        let attrs = attributes(&html[start + 5..open_end]);
        let close = lower[open_end..]
            .find("</form")
            .map_or(html.len(), |j| open_end + j);
        let action = attrs.get("action").cloned().unwrap_or_default();
        let is_download = attrs.get("id").is_some_and(|v| v == "download-form")
            || action.contains("/download")
            || action.contains("/uc");
        if is_download {
            let fields = hidden_inputs(&html[open_end..close]);
            if fields.iter().any(|(k, _)| k == "confirm" || k == "uuid") {
                let base = if action.is_empty() {
                    page_url.split('?').next().unwrap_or(page_url).to_string()
                } else {
                    absolute(&action, page_url)
                };
                let query: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
                    .collect();
                let sep = if base.contains('?') { '&' } else { '?' };
                return Some(format!("{base}{sep}{}", query.join("&")));
            }
        }
        pos = close;
    }
    None
}

fn legacy_link(html: &str, page_url: &str) -> Option<String> {
    // href="/uc?export=download&amp;confirm=AbCd&amp;id=…"
    let mut pos = 0;
    while let Some(i) = html[pos..].find("href=") {
        let start = pos + i + 5;
        let quote = html[start..].chars().next()?;
        if quote != '"' && quote != '\'' {
            pos = start;
            continue;
        }
        let end = start + 1 + html[start + 1..].find(quote)?;
        let href = unescape(&html[start + 1..end]);
        if href.contains("confirm=") && (href.contains("export=download") || href.contains("id=")) {
            return Some(absolute(&href, page_url));
        }
        pos = end;
    }
    None
}

fn hidden_inputs(form: &str) -> Vec<(String, String)> {
    let lower = form.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<input") {
        let start = pos + i + 6;
        let Some(len) = lower[start..].find('>') else {
            break;
        };
        let attrs = attributes(&form[start..start + len]);
        let kind = attrs.get("type").map(|t| t.to_ascii_lowercase());
        if kind.as_deref() == Some("hidden") {
            if let Some(name) = attrs.get("name") {
                out.push((
                    name.clone(),
                    attrs.get("value").cloned().unwrap_or_default(),
                ));
            }
        }
        pos = start + len;
    }
    out
}

/// Attributes of a tag body (`name="v" name='v' name=v`), names in lowercase.
fn attributes(tag: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let b = tag.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let ns = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        let name = tag[ns..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                i += 1;
                let vs = i;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                value = unescape(&tag[vs..i]);
                i += 1;
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = unescape(&tag[vs..i]);
            }
        }
        if !name.is_empty() {
            out.entry(name).or_insert(value);
        } else if i < b.len() {
            i += 1;
        }
    }
    out
}

/// Decodes the HTML entities found in these pages.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn absolute(href: &str, page_url: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    let (scheme, rest) = page_url.split_once("://").unwrap_or(("https", page_url));
    let host = rest.split(['/', '?']).next().unwrap_or("");
    if let Some(h) = href.strip_prefix("//") {
        format!("{scheme}://{h}")
    } else if href.starts_with('/') {
        format!("{scheme}://{host}{href}")
    } else {
        format!("{scheme}://{host}/{href}")
    }
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "1AbCdEfGhIjKlMnOpQrStUvWxYz012345";

    #[test]
    fn ids_from_links() {
        for link in [
            format!("https://drive.google.com/file/d/{ID}/view?usp=sharing"),
            format!("https://drive.google.com/file/d/{ID}/view?usp=drive_link"),
            format!("https://drive.google.com/open?id={ID}"),
            format!("https://drive.google.com/uc?export=download&id={ID}"),
            format!("https://drive.usercontent.google.com/download?id={ID}&export=download"),
            ID.to_string(),
        ] {
            assert_eq!(file_id(&link).as_deref(), Some(ID), "{link}");
        }
        assert_eq!(file_id("https://example.com/file/d/abc"), None);
        assert_eq!(
            direct_url(&format!("https://drive.google.com/file/d/{ID}/view")),
            download_url(ID)
        );
        assert_eq!(
            direct_url("http://127.0.0.1:8000/x"),
            "http://127.0.0.1:8000/x"
        );
    }

    #[test]
    fn entities() {
        assert_eq!(unescape("a&amp;b&#39;c&#x3D;d&quot;"), "a&b'c=d\"");
        assert_eq!(unescape("&naoexiste; & x"), "&naoexiste; & x");
    }
}
