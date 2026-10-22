//! Hand-written CSV export of run results.

use std::io;
use std::path::Path;

use utsim_core::{RunReport, summarize};

/// Quotes a field when it contains a comma, quote or line break.
pub fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn row(out: &mut String, fields: &[String]) {
    let line: Vec<String> = fields.iter().map(|f| field(f)).collect();
    out.push_str(&line.join(","));
    out.push('\n');
}

/// Numbers are written with Rust's shortest round-trip format; NaN becomes an empty cell.
fn num(x: f64) -> String {
    if x.is_nan() { String::new() } else { x.to_string() }
}

fn summary_csv(reports: &[RunReport]) -> String {
    let mut out = String::from("metrica,n,media,desvio_padrao,meia_largura_95\n");
    for (metric, s) in summarize(reports) {
        row(&mut out, &[metric, s.n.to_string(), num(s.mean), num(s.std_dev), num(s.half_width)]);
    }
    out
}

fn replications_csv(reports: &[RunReport]) -> String {
    let mut out = String::from("replicacao,metrica,valor\n");
    for r in reports {
        for (metric, v) in r.metrics() {
            row(&mut out, &[r.replication.to_string(), metric, num(v)]);
        }
    }
    out
}

fn series_csv(reports: &[RunReport]) -> String {
    let mut out = String::from("replicacao,objeto,t,conteudo,estado\n");
    for r in reports {
        for b in &r.blocks {
            for p in &b.series {
                row(&mut out, &[r.replication.to_string(), b.name.clone(), num(p.t), p.content.to_string(), p.state.clone()]);
            }
        }
    }
    out
}

fn histograms_csv(reports: &[RunReport]) -> String {
    let mut out = String::from("replicacao,objeto,metrica,inicio_bin,fim_bin,contagem\n");
    for r in reports {
        for b in &r.blocks {
            let tallies = std::iter::once(("staytime", &b.staytime))
                .chain(b.extra.iter().map(|(k, t)| (k.as_str(), t)));
            for (metric, t) in tallies {
                let Some(h) = &t.histogram else { continue };
                for (k, count) in h.counts.iter().enumerate() {
                    let (start, end) = h.edges(k);
                    row(
                        &mut out,
                        &[r.replication.to_string(), b.name.clone(), metric.to_string(), num(start), num(end), count.to_string()],
                    );
                }
            }
        }
    }
    out
}

/// Writes the CSV files into `dir` (created if needed) and returns the file names written.
pub fn export(dir: &Path, reports: &[RunReport]) -> io::Result<Vec<String>> {
    std::fs::create_dir_all(dir)?;
    let mut files = vec![
        ("resumo.csv", summary_csv(reports)),
        ("replicacoes.csv", replications_csv(reports)),
    ];
    if reports.iter().any(|r| r.blocks.iter().any(|b| !b.series.is_empty())) {
        files.push(("series.csv", series_csv(reports)));
    }
    let has_histograms = reports
        .iter()
        .flat_map(|r| &r.blocks)
        .any(|b| b.staytime.histogram.is_some() || b.extra.values().any(|t| t.histogram.is_some()));
    if has_histograms {
        files.push(("histogramas.csv", histograms_csv(reports)));
    }
    let mut written = Vec::new();
    for (name, content) in files {
        std::fs::write(dir.join(name), content)?;
        written.push(name.to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(field("fila.content.mean"), "fila.content.mean");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("diz \"oi\""), "\"diz \"\"oi\"\"\"");
        assert_eq!(field("linha\nnova"), "\"linha\nnova\"");
    }

    #[test]
    fn rows_are_comma_joined_and_quoted() {
        let mut out = String::new();
        row(&mut out, &["x".into(), "a,b".into(), "3".into()]);
        assert_eq!(out, "x,\"a,b\",3\n");
    }
}
