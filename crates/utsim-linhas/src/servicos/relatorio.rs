//! Report of the line Serviços e atendimento: the indicators in the terminal and a
//! self-contained HTML page (cards, results by ticket type and by queue, waiting-time
//! histogram with the target, attendants' occupancy, staffing by hour and call panel).

use std::fmt::Write as _;

use super::{Atendimento, letra};
use crate::lean_office::relatorio::{CSS, esc};
use crate::lean_office::{mhw, num, num1, pct_hw};

/// The indicators as text.
pub fn texto(k: &Atendimento) -> String {
    let mut t = String::new();
    let _ = writeln!(
        t,
        "{}: {} ({} replicação(ões), média ± IC 95%)",
        k.linha,
        k.modelo,
        k.reps.len()
    );
    let _ = writeln!(t);
    let linhas = [
        (
            "Clientes atendidos",
            mhw(&k.resumo(|r| r.atendidos as f64), 1.0),
        ),
        ("Abandonos", mhw(&k.resumo(|r| r.abandonos as f64), 1.0)),
        ("% de abandono", pct_hw(&k.resumo(|r| r.abandono))),
        (
            "Tempo médio de espera (TME)",
            format!("{} min", mhw(&k.resumo(|r| r.espera), 1.0)),
        ),
        (
            "Espera do 90º percentil",
            format!("{} min", mhw(&k.resumo(|r| r.espera_p90), 1.0)),
        ),
        (
            "Tempo médio de atendimento (TMA)",
            format!("{} min", mhw(&k.resumo(|r| r.atendimento), 1.0)),
        ),
        (
            "Nível de serviço",
            format!(
                "{} em até {} min (meta: {}%)",
                pct_hw(&k.resumo(|r| r.nivel)),
                num(k.meta.wait),
                num1(100.0 * k.meta.share)
            ),
        ),
        (
            "Clientes atendidos por dia",
            mhw(&k.resumo(|r| r.por_dia), 1.0),
        ),
    ];
    let w = linhas
        .iter()
        .map(|l| l.0.chars().count())
        .max()
        .unwrap_or(0);
    for (nome, valor) in linhas {
        let pad = w - nome.chars().count();
        let _ = writeln!(t, "  {nome}{}  {valor}", " ".repeat(pad));
    }
    if let Some(s) = k.resumo_opt(|r| r.espera_abandono) {
        let _ = writeln!(t, "  Espera até desistir: {} min", mhw(&s, 1.0));
    }
    let tipos: Vec<i64> = tipos(k);
    if !tipos.is_empty() {
        let _ = writeln!(t, "\nPor tipo de senha:");
        for tp in tipos {
            let g = |f: fn(&super::Grupo) -> f64| k.resumo_opt(|r| r.por_tipo.get(&tp).map(f));
            let _ = writeln!(
                t,
                "  {}: {} atendidos, {} abandonos, espera {} min, nível {}",
                letra(tp),
                g(|x| x.atendidos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.abandonos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.espera).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.nivel).map_or("-".into(), |s| pct_hw(&s)),
            );
        }
    }
    let mut filas: Vec<&String> = k.reps.iter().flat_map(|r| r.por_fila.keys()).collect();
    filas.sort();
    filas.dedup();
    if filas.len() > 1 {
        let _ = writeln!(t, "\nPor fila (canal):");
        for q in filas {
            let g = |f: fn(&super::Grupo) -> f64| k.resumo_opt(|r| r.por_fila.get(q).map(f));
            let _ = writeln!(
                t,
                "  {q}: {} atendidos, {} abandonos, espera {} min, nível {}",
                g(|x| x.atendidos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.abandonos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.espera).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.nivel).map_or("-".into(), |s| pct_hw(&s)),
            );
        }
    }
    if !k.horas.is_empty() {
        let _ = writeln!(
            t,
            "\nDimensionamento por hora (Erlang C, {}% em até {} min):",
            num1(100.0 * k.meta.share),
            num(k.meta.wait)
        );
        for h in &k.horas {
            let _ = writeln!(
                t,
                "  {:02}h: {} chegadas/h, {} atendente(s), nível simulado {}",
                h.hora,
                num(h.chegadas),
                h.necessarios.map_or("-".into(), |n| n.to_string()),
                h.nivel
                    .map_or("-".into(), |v| format!("{}%", num1(100.0 * v))),
            );
        }
    }
    t
}

/// Every ticket type seen in any replication.
fn tipos(k: &Atendimento) -> Vec<i64> {
    let mut v: Vec<i64> = k
        .reps
        .iter()
        .flat_map(|r| r.por_tipo.keys().copied())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Histogram of the waits of replication 0 with the target as a dashed line.
fn histograma(k: &Atendimento) -> String {
    let Some(r) = k.reps.first() else {
        return String::new();
    };
    if r.esperas.is_empty() {
        return String::new();
    }
    let max = r
        .esperas
        .iter()
        .copied()
        .fold(k.meta.wait, f64::max)
        .max(1e-9);
    let bins = 30usize;
    let mut counts = vec![0usize; bins];
    for w in &r.esperas {
        let b = ((w / max) * bins as f64).floor() as usize;
        counts[b.min(bins - 1)] += 1;
    }
    let top = *counts.iter().max().unwrap_or(&1) as f64;
    let (w, h, x0, y0) = (720.0, 220.0, 40.0, 190.0);
    let bw = (w - x0 - 10.0) / bins as f64;
    let mut s =
        format!("<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Distribuição da espera\">");
    let _ = write!(
        s,
        "<line class=\"eixo\" x1=\"{x0}\" y1=\"{y0}\" x2=\"{}\" y2=\"{y0}\"/>",
        w - 10.0
    );
    for (i, c) in counts.iter().enumerate() {
        let bh = (*c as f64 / top) * (y0 - 20.0);
        let _ = write!(
            s,
            "<rect class=\"barra\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\"/>",
            x0 + i as f64 * bw + 1.0,
            y0 - bh,
            (bw - 2.0).max(1.0),
            bh
        );
    }
    let xm = x0 + (k.meta.wait / max) * (w - x0 - 10.0);
    let _ = write!(
        s,
        "<line class=\"sla\" x1=\"{xm:.1}\" y1=\"15\" x2=\"{xm:.1}\" y2=\"{y0}\"/><text x=\"{:.1}\" y=\"14\">meta {} min</text>",
        xm + 4.0,
        num(k.meta.wait)
    );
    let _ = write!(
        s,
        "<text x=\"{x0}\" y=\"{}\">0</text><text x=\"{}\" y=\"{}\" text-anchor=\"end\">{} min</text></svg>",
        y0 + 16.0,
        w - 10.0,
        y0 + 16.0,
        num(max)
    );
    s
}

/// The self-contained HTML page.
pub fn html(k: &Atendimento) -> String {
    let mut h = String::new();
    let t = esc(&format!("{}: {}", k.linha, k.modelo));
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>{CSS}</style></head><body><h1>{t}</h1><p class=\"muted\">{} replicação(ões); valores com o intervalo de 95 % entre replicações. Meta de nível de serviço: {}% dos clientes atendidos em até {} min de espera.</p>",
        k.reps.len(),
        num1(100.0 * k.meta.share),
        num(k.meta.wait)
    );
    let cards = [
        ("Atendidos", mhw(&k.resumo(|r| r.atendidos as f64), 1.0)),
        ("Nível de serviço", pct_hw(&k.resumo(|r| r.nivel))),
        (
            "Espera média (TME)",
            format!("{} min", mhw(&k.resumo(|r| r.espera), 1.0)),
        ),
        (
            "Atendimento (TMA)",
            format!("{} min", mhw(&k.resumo(|r| r.atendimento), 1.0)),
        ),
        ("Abandono", pct_hw(&k.resumo(|r| r.abandono))),
        (
            "Espera do 90º percentil",
            format!("{} min", mhw(&k.resumo(|r| r.espera_p90), 1.0)),
        ),
    ];
    h.push_str("<div class=\"cards\">");
    for (nome, valor) in cards {
        let _ = write!(
            h,
            "<div class=\"card\">{}<b>{}</b></div>",
            esc(nome),
            esc(&valor)
        );
    }
    h.push_str("</div>");

    h.push_str("<h2>Por tipo de senha</h2><table><thead><tr><th>Senha</th><th class=\"n\">Atendidos</th><th class=\"n\">Abandonos</th><th class=\"n\">Espera média</th><th class=\"n\">Nível de serviço</th></tr></thead><tbody>");
    for tp in tipos(k) {
        let g = |f: fn(&super::Grupo) -> f64| k.resumo_opt(|r| r.por_tipo.get(&tp).map(f));
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{} min</td><td class=\"n\">{}</td></tr>",
            letra(tp),
            g(|x| x.atendidos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
            g(|x| x.abandonos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
            g(|x| x.espera).map_or("-".into(), |s| mhw(&s, 1.0)),
            g(|x| x.nivel).map_or("-".into(), |s| pct_hw(&s)),
        );
    }
    h.push_str("</tbody></table><p class=\"muted\">N: normal; P: preferencial; A: agendado (rótulo <code>tipo</code> 1, 2 e 3).</p>");

    let mut filas: Vec<&String> = k.reps.iter().flat_map(|r| r.por_fila.keys()).collect();
    filas.sort();
    filas.dedup();
    if filas.len() > 1 {
        h.push_str("<h2>Por fila</h2><table><thead><tr><th>Fila</th><th class=\"n\">Atendidos</th><th class=\"n\">Abandonos</th><th class=\"n\">Espera média</th><th class=\"n\">Nível de serviço</th></tr></thead><tbody>");
        for q in filas {
            let g = |f: fn(&super::Grupo) -> f64| k.resumo_opt(|r| r.por_fila.get(q).map(f));
            let _ = write!(
                h,
                "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{} min</td><td class=\"n\">{}</td></tr>",
                esc(q),
                g(|x| x.atendidos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.abandonos as f64).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.espera).map_or("-".into(), |s| mhw(&s, 1.0)),
                g(|x| x.nivel).map_or("-".into(), |s| pct_hw(&s)),
            );
        }
        h.push_str("</tbody></table>");
    }

    h.push_str("<h2>Distribuição da espera</h2><p class=\"muted\">Clientes atendidos na replicação 0; a linha tracejada é a meta.</p>");
    h.push_str(&histograma(k));

    let mut pessoas: Vec<&String> = k.reps.iter().flat_map(|r| r.ocupacao.keys()).collect();
    pessoas.sort();
    pessoas.dedup();
    if !pessoas.is_empty() {
        h.push_str("<h2>Ocupação dos atendentes</h2><table><thead><tr><th>Atendente</th><th class=\"n\">Atendendo (fração do expediente)</th></tr></thead><tbody>");
        for p in pessoas {
            let v = k
                .resumo_opt(|r| r.ocupacao.get(p).copied())
                .map_or("-".into(), |s| pct_hw(&s));
            let _ = write!(h, "<tr><td>{}</td><td class=\"n\">{v}</td></tr>", esc(p));
        }
        h.push_str("</tbody></table>");
    }

    if !k.horas.is_empty() {
        let _ = write!(
            h,
            "<h2>Dimensionamento por hora</h2><p class=\"muted\">Chegadas por hora (média dos dias com chegadas nessa hora), atendentes que a fila M/M/c (Erlang C, sem abandono) pede para {}% dos clientes esperarem até {} min com o TMA do modelo, e o nível de serviço simulado de quem chegou nessa hora.</p><table><thead><tr><th>Hora</th><th class=\"n\">Chegadas/h</th><th class=\"n\">Atendentes necessários</th><th class=\"n\">Nível simulado</th></tr></thead><tbody>",
            num1(100.0 * k.meta.share),
            num(k.meta.wait)
        );
        for x in &k.horas {
            let _ = write!(
                h,
                "<tr><td>{:02}h</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                x.hora,
                num(x.chegadas),
                x.necessarios.map_or("-".into(), |n| n.to_string()),
                x.nivel
                    .map_or("-".into(), |v| format!("{}%", num1(100.0 * v))),
            );
        }
        h.push_str("</tbody></table>");
    }

    if !k.painel.is_empty() {
        h.push_str("<h2>Painel de chamada</h2><p class=\"muted\">As primeiras chamadas da replicação 0 depois do aquecimento: hora, senha (letra do tipo e número na ordem de chegada do tipo), guichê e espera.</p><table><thead><tr><th>Hora</th><th>Senha</th><th>Guichê</th><th class=\"n\">Espera</th></tr></thead><tbody>");
        for c in &k.painel {
            let m = c.instante.rem_euclid(1440.0);
            let _ = write!(
                h,
                "<tr><td>dia {} {:02}:{:02}</td><td><b>{}</b></td><td>{}</td><td class=\"n\">{} min</td></tr>",
                (c.instante / 1440.0).floor() as i64 + 1,
                (m / 60.0).floor() as i64,
                (m % 60.0).floor() as i64,
                esc(&c.senha),
                esc(&c.guiche),
                num(c.espera)
            );
        }
        h.push_str("</tbody></table>");
    }
    h.push_str("</body></html>");
    h
}
