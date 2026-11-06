//! Self-contained HTML report of the quality indicators (opens without network: styles
//! and charts are inline SVG).

use std::fmt::Write;

use crate::simulado::{Medicao, RelatorioQualidade, coluna};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fmt(v: f64) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    let s = if v.abs() >= 1000.0 {
        format!("{v:.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.4}")
    };
    s.replace('.', ",")
}

/// Horizontal Pareto bars with the cumulative percentage.
fn pareto_svg(dados: &[(String, f64)]) -> String {
    if dados.is_empty() {
        return "<p>Nenhum defeito na simulação.</p>".into();
    }
    let total: f64 = dados.iter().map(|d| d.1).sum();
    let max = dados[0].1.max(1e-12);
    let h = 26.0;
    let altura = h * dados.len() as f64 + 10.0;
    let mut s = format!(
        "<svg viewBox=\"0 0 640 {altura}\" width=\"640\" height=\"{altura}\" role=\"img\">"
    );
    let mut acum = 0.0;
    for (i, (nome, v)) in dados.iter().enumerate() {
        acum += v;
        let y = 5.0 + i as f64 * h;
        let w = 360.0 * v / max;
        let _ = write!(
            s,
            "<text x=\"150\" y=\"{}\" text-anchor=\"end\" font-size=\"13\">{}</text>\
             <rect x=\"158\" y=\"{y}\" width=\"{w:.1}\" height=\"{}\" fill=\"#3b6ea5\"/>\
             <text x=\"{:.1}\" y=\"{}\" font-size=\"12\">{} ({}% acum.)</text>",
            y + 17.0,
            esc(nome),
            h - 6.0,
            164.0 + w,
            y + 17.0,
            fmt(*v),
            fmt(100.0 * acum / total.max(1e-12))
        );
    }
    s.push_str("</svg>");
    s
}

/// Individual values chart with limits at the mean ± 3 sigma estimated by the moving range.
fn carta_svg(valores: &[f64], lsl: Option<f64>, usl: Option<f64>) -> String {
    if valores.len() < 2 {
        return String::new();
    }
    let n = valores.len();
    let media = valores.iter().sum::<f64>() / n as f64;
    let mr = valores.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (n - 1) as f64;
    let sigma = mr / 1.128;
    let (lic, lsc) = (media - 3.0 * sigma, media + 3.0 * sigma);
    let mut lo = valores.iter().copied().fold(lic, f64::min);
    let mut hi = valores.iter().copied().fold(lsc, f64::max);
    for l in [lsl, usl].into_iter().flatten() {
        lo = lo.min(l);
        hi = hi.max(l);
    }
    let pad = (hi - lo).max(1e-9) * 0.05;
    let (lo, hi) = (lo - pad, hi + pad);
    let (w, h) = (640.0, 220.0);
    let x = |i: usize| 40.0 + (w - 50.0) * i as f64 / (n - 1) as f64;
    let y = |v: f64| h - 20.0 - (h - 30.0) * (v - lo) / (hi - lo);
    let mut s = format!("<svg viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\">");
    let linha = |s: &mut String, v: f64, cor: &str, rotulo: &str| {
        let _ = write!(
            s,
            "<line x1=\"40\" x2=\"{}\" y1=\"{:.1}\" y2=\"{:.1}\" stroke=\"{cor}\" stroke-dasharray=\"4 3\"/>\
             <text x=\"2\" y=\"{:.1}\" font-size=\"10\" fill=\"{cor}\">{rotulo}</text>",
            w - 10.0,
            y(v),
            y(v),
            y(v) + 3.0
        );
    };
    linha(&mut s, media, "#555", "média");
    linha(&mut s, lic, "#c0392b", "LIC");
    linha(&mut s, lsc, "#c0392b", "LSC");
    if let Some(l) = lsl {
        linha(&mut s, l, "#8e44ad", "LIE");
    }
    if let Some(u) = usl {
        linha(&mut s, u, "#8e44ad", "LSE");
    }
    let pontos: Vec<String> = valores
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{:.1},{:.1}", x(i), y(*v)))
        .collect();
    let _ = write!(
        s,
        "<polyline points=\"{}\" fill=\"none\" stroke=\"#3b6ea5\" stroke-width=\"1\"/>",
        pontos.join(" ")
    );
    for (i, v) in valores.iter().enumerate() {
        if *v > lsc || *v < lic {
            let _ = write!(
                s,
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" fill=\"#c0392b\"/>",
                x(i),
                y(*v)
            );
        }
    }
    s.push_str("</svg>");
    s
}

/// The whole report.
pub fn html(r: &RelatorioQualidade) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <title>Qualidade: {nome}</title><style>\
         body{{font-family:system-ui,sans-serif;margin:24px;max-width:960px;color:#222}}\
         table{{border-collapse:collapse;margin:8px 0 20px}}\
         td,th{{border:1px solid #ccc;padding:4px 10px;text-align:right}}\
         td:first-child,th:first-child{{text-align:left}}\
         .nota{{color:#555;font-size:13px}}</style></head><body>\
         <h1>Relatório de qualidade da simulação</h1><p>Modelo <b>{nome}</b>, {n} replicações. \
         Origem dos dados: <b>Simulado</b>.</p>",
        nome = esc(&r.modelo),
        n = r.replicacoes.len()
    );
    s.push_str(
        "<h2>Indicadores</h2><table><tr><th>Indicador</th><th>Média</th><th>± IC 95 %</th></tr>",
    );
    for (nome, i) in &r.resumo {
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(nome),
            fmt(i.media),
            fmt(i.meia_largura)
        );
    }
    let _ = write!(
        s,
        "</table><p class=\"nota\">DPMO com {} oportunidade(s) por unidade (tipos de defeito e \
         características do modelo). Rendimentos e percentuais como frações de 0 a 1.</p>",
        r.oportunidades
    );
    s.push_str("<h2>Pareto por tipo de defeito</h2>");
    s.push_str(&pareto_svg(&r.pareto_tipo));
    s.push_str("<h2>Pareto por posto de origem</h2>");
    s.push_str(&pareto_svg(&r.pareto_posto));
    if !r.capacidades.is_empty() {
        s.push_str(
            "<h2>Características medidas</h2><table><tr><th>Característica</th><th>Posto</th>\
             <th>n</th><th>Média</th><th>Cp</th><th>Cpk</th><th>Pp</th><th>Ppk</th></tr>",
        );
        for c in &r.capacidades {
            match &c.capacidade {
                Some(k) => {
                    let _ = write!(
                        s,
                        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                        esc(&c.caracteristica),
                        esc(&c.objeto),
                        c.n,
                        fmt(k.mean),
                        k.cp.map_or("–".into(), |i| fmt(i.value)),
                        fmt(k.cpk.value),
                        k.pp.map_or("–".into(), |i| fmt(i.value)),
                        fmt(k.ppk.value)
                    );
                }
                None => {
                    let _ = write!(
                        s,
                        "<tr><td>{}</td><td>{}</td><td>{}</td><td colspan=\"5\">{}</td></tr>",
                        esc(&c.caracteristica),
                        esc(&c.objeto),
                        c.n,
                        esc(c.erro.as_deref().unwrap_or(""))
                    );
                }
            }
        }
        s.push_str("</table>");
        for c in &r.capacidades {
            let v = coluna(&r.medicoes, &c.caracteristica, 0);
            let spec = c.capacidade.as_ref().map(|k| (k.spec.lsl, k.spec.usl));
            let (lsl, usl) = spec.unwrap_or((None, None));
            let _ = write!(
                s,
                "<h3>Carta de valores individuais: {} (replicação 1)</h3>{}",
                esc(&c.caracteristica),
                carta_svg(&v, lsl, usl)
            );
        }
    }
    s.push_str(
        "<p class=\"nota\">Os valores vêm do modelo, não de medições reais. Servem para estudar \
         onde o defeito nasce, onde é detectado e o efeito de mudar a inspeção, o método ou a \
         máquina.</p></body></html>",
    );
    s
}

/// Long output of the measured values, also usable alone.
pub fn csv(medicoes: &[Medicao]) -> String {
    crate::simulado::csv_medicoes(medicoes)
}
