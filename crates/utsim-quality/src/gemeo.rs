//! Data of the digital twin as series, for the phase II charts of the apps (P5 of
//! `docs/plano_lean_seis_sigma.md`, section 3.5; the same data serve the control charts of
//! the quality app): the recordings of `utsim-twin` and the JSON of its panel route
//! `/api/gemeo`.
//!
//! Only the formats are read here; the twin crate is not a dependency, because it brings
//! the network clients of the plant. The field names are the ones the twin writes:
//!
//! - recording: a session folder with `sessao.json` and `observacoes-AAAAMMDD.jsonl`, one
//!   JSON object per line, `{"tipo":"obs","t":…,"tag":…,"valor":…,"qualidade":…}`;
//! - panel: `{"modelo":…,"tempo":…,"janelas":[{"index","start","end","values":{kpi:valor}}],
//!   "kpis":{kpi:{"simulado_media","simulado_dp","n_simulado",…}},"alertas":[…]}`.
//!
//! A series is keyed `tag:<tag>` (an observation of the recording) or `kpi:<id>` (the KPI
//! observed in each window of the panel). Only readings of good quality with a numeric
//! value enter (flags count as 1 and 0, texts with a decimal comma are read). Reading files
//! and the local panel is the only input and output of this crate: [`read`].

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::charts::Limits;

/// Prefix of a series of observations of a tag.
pub const TAG: &str = "tag:";
/// Prefix of a series of a KPI observed per window.
pub const KPI: &str = "kpi:";

/// One value of a series at a twin time (model time units since the twin started).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub t: f64,
    pub value: f64,
}

/// What the model expects of one KPI in a window (from the panel).
#[derive(Clone, Debug, PartialEq)]
pub struct ModelKpi {
    pub mean: f64,
    pub sd: f64,
    /// Simulated windows behind the mean and the standard deviation.
    pub n: usize,
}

impl ModelKpi {
    /// Phase II limits from the model: the simulated mean ± 3 standard deviations of the
    /// KPI in a window. `None` without spread or with fewer than two simulated windows.
    pub fn limits(&self) -> Option<Limits> {
        (self.n >= 2 && self.sd > 0.0 && self.mean.is_finite()).then_some(Limits::Fixed {
            center: self.mean,
            lcl: self.mean - 3.0 * self.sd,
            ucl: self.mean + 3.0 * self.sd,
        })
    }
}

/// An alert of the twin (the model and the plant drifted apart).
#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub t: f64,
    pub kpi: String,
    pub level: String,
    /// `BH`, `CUSUM` or `EWMA`.
    pub test: String,
    pub message: String,
}

/// What was read from the twin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TwinData {
    /// Where it came from: the panel address or the recording session folder.
    pub origin: String,
    /// Model of the twin (panel) or model file of the recording.
    pub model: String,
    /// Series by key (`tag:…` or `kpi:…`), in time order.
    pub series: BTreeMap<String, Vec<Point>>,
    /// What the model expects of each KPI (panel only), by key `kpi:…`.
    pub expected: BTreeMap<String, ModelKpi>,
    pub alerts: Vec<Alert>,
}

impl TwinData {
    /// The series of a key; a bare name is tried as a KPI and then as a tag.
    pub fn get(&self, key: &str) -> Option<&[Point]> {
        let key = key.trim();
        self.series
            .get(key)
            .or_else(|| self.series.get(&format!("{KPI}{key}")))
            .or_else(|| self.series.get(&format!("{TAG}{key}")))
            .map(Vec::as_slice)
    }

    /// The model limits of a KPI key (panel only).
    pub fn model_limits(&self, key: &str) -> Option<Limits> {
        let key = key.trim();
        self.expected
            .get(key)
            .or_else(|| self.expected.get(&format!("{KPI}{key}")))
            .and_then(ModelKpi::limits)
    }

    /// Alerts of one KPI key.
    pub fn alerts_of(&self, key: &str) -> Vec<&Alert> {
        let id = key.trim().strip_prefix(KPI).unwrap_or(key.trim());
        self.alerts.iter().filter(|a| a.kpi == id).collect()
    }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|x| x.is_finite()),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<f64>()
                .ok()
                .or_else(|| s.replace(',', ".").parse::<f64>().ok())
                .filter(|x| x.is_finite())
        }
        _ => None,
    }
}

/// The observations of recording lines (one JSON object per line), by `tag:<tag>`.
/// Lines that are not observations, of bad or uncertain quality or without a numeric value
/// are left out; a malformed line is an error with its number.
pub fn recording_series(text: &str) -> Result<BTreeMap<String, Vec<Point>>, String> {
    let mut out: BTreeMap<String, Vec<Point>> = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).map_err(|e| format!("linha {}: {e}", i + 1))?;
        if v.get("tipo").and_then(Value::as_str) != Some("obs") {
            continue;
        }
        let good = v
            .get("qualidade")
            .and_then(Value::as_str)
            .is_none_or(|q| q == "Good");
        let (Some(t), Some(tag), Some(x)) = (
            v.get("t").and_then(Value::as_f64),
            v.get("tag").and_then(Value::as_str),
            v.get("valor").and_then(number),
        ) else {
            continue;
        };
        if good && t.is_finite() {
            out.entry(format!("{TAG}{tag}"))
                .or_default()
                .push(Point { t, value: x });
        }
    }
    for s in out.values_mut() {
        s.sort_by(|a, b| a.t.total_cmp(&b.t));
    }
    Ok(out)
}

/// The panel JSON (`/api/gemeo`): the KPIs observed per window (at the end of the window),
/// what the model expects of them and the alerts.
pub fn panel(json: &str) -> Result<TwinData, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("painel do gêmeo: {e}"))?;
    if v.get("janelas").is_none() && v.get("kpis").is_none() {
        return Err("a resposta não é do painel do gêmeo (faltam janelas e kpis)".into());
    }
    let mut d = TwinData {
        model: v
            .get("modelo")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ..TwinData::default()
    };
    for w in v
        .get("janelas")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(t) = w.get("end").and_then(Value::as_f64) else {
            continue;
        };
        for (k, x) in w
            .get("values")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if let Some(x) = number(x) {
                d.series
                    .entry(format!("{KPI}{k}"))
                    .or_default()
                    .push(Point { t, value: x });
            }
        }
    }
    for s in d.series.values_mut() {
        s.sort_by(|a, b| a.t.total_cmp(&b.t));
    }
    for (k, s) in v
        .get("kpis")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let f = |n: &str| s.get(n).and_then(Value::as_f64);
        if let (Some(mean), Some(sd)) = (f("simulado_media"), f("simulado_dp")) {
            let n = s.get("n_simulado").and_then(Value::as_u64).unwrap_or(0) as usize;
            d.expected
                .insert(format!("{KPI}{k}"), ModelKpi { mean, sd, n });
        }
    }
    for a in v
        .get("alertas")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let s = |n: &str| {
            a.get(n)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let level = match a.get("nivel") {
            Some(Value::String(x)) => x.clone(),
            Some(x) => x.to_string(),
            None => String::new(),
        };
        d.alerts.push(Alert {
            t: a.get("t").and_then(Value::as_f64).unwrap_or(f64::NAN),
            kpi: s("kpi"),
            level,
            test: s("teste"),
            message: s("mensagem"),
        });
    }
    Ok(d)
}

/// The session folder of a recording: the folder itself when it has `sessao.json`,
/// otherwise its most recent session.
fn session_folder(path: &Path) -> Result<PathBuf, String> {
    if path.join("sessao.json").is_file() {
        return Ok(path.to_path_buf());
    }
    let mut s: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("sessao.json").is_file())
        .collect();
    s.sort();
    s.pop().ok_or_else(|| {
        format!(
            "{}: nenhuma gravação do gêmeo (sessao.json) encontrada",
            path.display()
        )
    })
}

/// Reads a recording: a session folder, a folder of sessions (the most recent one) or a
/// single `.jsonl` file.
pub fn read_recording(path: &Path) -> Result<TwinData, String> {
    let (origin, files) = if path.is_file() {
        (path.to_path_buf(), vec![path.to_path_buf()])
    } else {
        let session = session_folder(path)?;
        let mut files: Vec<PathBuf> = std::fs::read_dir(&session)
            .map_err(|e| format!("{}: {e}", session.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("observacoes-") && n.ends_with(".jsonl"))
            })
            .collect();
        files.sort();
        (session, files)
    };
    let mut text = String::new();
    for f in &files {
        text.push_str(&std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?);
        text.push('\n');
    }
    let model = std::fs::read_to_string(origin.join("sessao.json"))
        .ok()
        .and_then(|h| serde_json::from_str::<Value>(&h).ok())
        .and_then(|h| h.get("modelo").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    Ok(TwinData {
        origin: origin.display().to_string(),
        model,
        series: recording_series(&text)?,
        ..TwinData::default()
    })
}

/// Host, port and path of an `http://` address on this machine. Other hosts are refused:
/// the twin panel is served on the local machine only.
fn local_address(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .trim()
        .strip_prefix("http://")
        .ok_or_else(|| format!("{url}: só endereços http:// desta máquina"))?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| format!("{url}: porta inválida"))?,
        ),
        _ => (host_port.to_string(), 80),
    };
    if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]") {
        return Err(format!(
            "{url}: o painel do gêmeo é lido só nesta máquina (127.0.0.1)"
        ));
    }
    Ok((host, port, path))
}

/// GET of a route of the local panel; the body of a `200` answer.
pub fn fetch_local(url: &str, timeout: Duration) -> Result<String, String> {
    let (host, port, path) = local_address(url)?;
    let name = host.trim_start_matches('[').trim_end_matches(']');
    let addr = (name, port)
        .to_socket_addrs()
        .map_err(|e| format!("{url}: {e}"))?
        .next()
        .ok_or_else(|| format!("{url}: endereço não encontrado"))?;
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("{url}: {e}"))?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(timeout)).ok();
    let request =
        format!("GET {path} HTTP/1.0\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    s.write_all(request.as_bytes())
        .map_err(|e| format!("{url}: {e}"))?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(|e| format!("{url}: {e}"))?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("{url}: resposta sem cabeçalho"))?;
    let status = head.lines().next().unwrap_or_default();
    if !status.split_whitespace().nth(1).is_some_and(|c| c == "200") {
        return Err(format!("{url}: {status}"));
    }
    Ok(body.to_string())
}

/// Reads the twin at `address`: the local panel (`http://127.0.0.1:porta`, route
/// `/api/gemeo` added when the address has no path), a saved panel JSON file, a recording
/// session folder, a folder of sessions or a `.jsonl` file.
pub fn read(address: &str) -> Result<TwinData, String> {
    let a = address.trim();
    if a.is_empty() {
        return Err("informe o endereço do painel do gêmeo ou a pasta de gravações".into());
    }
    if a.starts_with("http://") {
        let url = if a.trim_end_matches('/').matches('/').count() <= 2 {
            format!("{}/api/gemeo", a.trim_end_matches('/'))
        } else {
            a.to_string()
        };
        let mut d = panel(&fetch_local(&url, Duration::from_secs(3))?)?;
        d.origin = url;
        return Ok(d);
    }
    let p = Path::new(a);
    if p.is_file()
        && p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{a}: {e}"))?;
        let mut d = panel(&text)?;
        d.origin = a.to_string();
        return Ok(d);
    }
    read_recording(p)
}

/// The series as a CSV with a header `t;<name>` and decimal points, for a data set.
pub fn csv(name: &str, points: &[Point]) -> String {
    let mut s = format!("t;{}\n", name.replace(';', ","));
    for p in points {
        s.push_str(&format!("{};{}\n", p.t, p.value));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: &str = r#"{"tipo":"obs","t":1.0,"ts":0,"fonte":"opc","seq":1,"tag":"Torno.diam","valor":10.1,"qualidade":"Good"}
{"tipo":"obs","t":0.5,"ts":0,"fonte":"opc","seq":0,"tag":"Torno.diam","valor":"9,9"}
{"tipo":"obs","t":2.0,"ts":0,"fonte":"opc","seq":2,"tag":"Torno.diam","valor":12.0,"qualidade":"Bad"}
{"tipo":"obs","t":2.5,"ts":0,"fonte":"opc","seq":3,"tag":"Porta","valor":true}
{"tipo":"obs","t":3.0,"ts":0,"fonte":"opc","seq":4,"tag":"Estado","valor":"parado"}
{"tipo":"previsao","t":3.0,"pedido":false}
{"tipo":"fim","t":4.0}
"#;

    #[test]
    fn recording_lines_become_series_of_good_numeric_readings() {
        let s = recording_series(LINES).unwrap();
        assert_eq!(
            s["tag:Torno.diam"],
            [
                Point { t: 0.5, value: 9.9 },
                Point {
                    t: 1.0,
                    value: 10.1
                }
            ]
        );
        assert_eq!(s["tag:Porta"], [Point { t: 2.5, value: 1.0 }]);
        assert!(!s.contains_key("tag:Estado"));
        assert!(
            recording_series("{nada")
                .unwrap_err()
                .starts_with("linha 1")
        );
    }

    #[test]
    fn recording_folder_takes_the_latest_session() {
        let dir = std::env::temp_dir().join(format!("utsim-gemeo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (s, v) in [
            ("sessao-20261101-080000", 1.0),
            ("sessao-20261102-080000", 2.0),
        ] {
            let f = dir.join(s);
            std::fs::create_dir_all(&f).unwrap();
            std::fs::write(f.join("sessao.json"), r#"{"modelo":"linha.ron"}"#).unwrap();
            std::fs::write(
                f.join("observacoes-20261102.jsonl"),
                format!(r#"{{"tipo":"obs","t":1,"tag":"x","valor":{v}}}"#),
            )
            .unwrap();
        }
        let d = read(dir.to_str().unwrap()).unwrap();
        assert_eq!(d.model, "linha.ron");
        assert_eq!(d.get("x").unwrap()[0].value, 2.0);
        assert!(d.origin.ends_with("sessao-20261102-080000"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn panel_windows_expectations_and_alerts() {
        let json = r#"{"modelo":"linha","tempo":30,
            "janelas":[{"index":1,"start":10,"end":20,"values":{"vazao":11.0}},
                       {"index":0,"start":0,"end":10,"values":{"vazao":12.0,"wip":3}}],
            "kpis":{"vazao":{"nivel":"Ok","simulado_media":12.0,"simulado_dp":0.5,"n_simulado":40}},
            "alertas":[{"janela":1,"t":20,"kpi":"vazao","objeto":"","nivel":"Alerta","teste":"CUSUM","mensagem":"abaixo"}]}"#;
        let d = panel(json).unwrap();
        assert_eq!(d.model, "linha");
        assert_eq!(
            d.get("vazao").unwrap(),
            [
                Point {
                    t: 10.0,
                    value: 12.0
                },
                Point {
                    t: 20.0,
                    value: 11.0
                }
            ]
        );
        let Some(Limits::Fixed { center, lcl, ucl }) = d.model_limits("kpi:vazao") else {
            panic!()
        };
        assert_eq!((center, lcl, ucl), (12.0, 10.5, 13.5));
        assert_eq!(d.alerts_of("kpi:vazao")[0].test, "CUSUM");
        assert_eq!(d.alerts_of("vazao")[0].level, "Alerta");
        assert!(panel(r#"{"x":1}"#).is_err());
    }

    #[test]
    fn only_local_addresses() {
        assert_eq!(
            local_address("http://127.0.0.1:8080").unwrap(),
            ("127.0.0.1".into(), 8080, "/".into())
        );
        assert_eq!(
            local_address("http://localhost:9/api/gemeo").unwrap().2,
            "/api/gemeo"
        );
        assert!(local_address("http://192.168.0.5:8080").is_err());
        assert!(local_address("https://127.0.0.1").is_err());
    }

    #[test]
    fn fetches_from_a_local_server() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 512];
            while !got.ends_with(b"\r\n\r\n") {
                let n = c.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            assert!(String::from_utf8_lossy(&got).starts_with("GET /api/gemeo "));
            let body = r#"{"janelas":[{"end":5,"values":{"vazao":3}}],"kpis":{}}"#;
            write!(
                c,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let d = read(&format!("http://127.0.0.1:{port}")).unwrap();
        h.join().unwrap();
        assert_eq!(d.get("kpi:vazao").unwrap()[0].value, 3.0);
    }

    #[test]
    fn csv_of_a_series() {
        let p = [Point { t: 1.5, value: 2.0 }];
        assert_eq!(csv("vazão;x", &p), "t;vazão,x\n1.5;2\n");
    }
}
