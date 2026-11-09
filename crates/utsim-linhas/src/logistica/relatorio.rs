//! Report of the line Logística e armazém: the indicators in the terminal and a
//! self-contained HTML page (cards, receiving, picking by hour of the day, shipping,
//! storage by ABC class and the occupancy of teams and docks).

use std::fmt::Write as _;

use super::Armazem;
use crate::lean_office::relatorio::{CSS, esc};
use crate::lean_office::{mhw, num, num1, pct_hw};

/// The headline indicators: receiving only when trucks reached a dock, picking only when
/// lines were picked, shipping only when orders shipped.
fn linhas(k: &Armazem) -> Vec<(&'static str, String)> {
    let algum = |f: fn(&super::Replicacao) -> usize| k.reps.iter().any(|r| f(r) > 0);
    let mut v = Vec::new();
    if algum(|r| r.caminhoes) {
        v.extend(recebimento(k));
    }
    if algum(|r| r.linhas) {
        v.push((
            "Linhas separadas por hora",
            mhw(&k.resumo(|r| r.linhas_hora), 1.0),
        ));
    }
    if algum(|r| r.pedidos) {
        v.extend(expedicao(k));
    }
    v
}

fn recebimento(k: &Armazem) -> Vec<(&'static str, String)> {
    vec![
        (
            "Caminhões descarregados",
            mhw(&k.resumo(|r| r.caminhoes as f64), 1.0),
        ),
        (
            "Espera no pátio",
            format!("{} min", mhw(&k.resumo(|r| r.patio), 1.0)),
        ),
        (
            "Tempo de doca",
            format!("{} min", mhw(&k.resumo(|r| r.doca), 1.0)),
        ),
        (
            "Caminhões na meta de doca",
            format!(
                "{} (meta: até {} min)",
                pct_hw(&k.resumo(|r| r.doca_na_meta)),
                num(k.meta.doca)
            ),
        ),
        (
            "Paletes recebidos",
            mhw(&k.resumo(|r| r.paletes as f64), 1.0),
        ),
    ]
}

fn expedicao(k: &Armazem) -> Vec<(&'static str, String)> {
    let mut v = vec![
        (
            "Pedidos expedidos por dia",
            mhw(&k.resumo(|r| r.pedidos_dia), 1.0),
        ),
        (
            "Ciclo do pedido",
            format!("{} min", mhw(&k.resumo(|r| r.ciclo), 1.0)),
        ),
        (
            "Linhas por pedido",
            mhw(&k.resumo(|r| r.linhas_por_pedido), 1.0),
        ),
    ];
    if let Some(s) = k.resumo_opt(|r| r.no_prazo) {
        v.push(("Pedidos no prazo", pct_hw(&s)));
    }
    v
}

fn nomes(k: &Armazem, f: impl Fn(&super::Replicacao) -> Vec<&String>) -> Vec<String> {
    let mut v: Vec<String> = k.reps.iter().flat_map(f).cloned().collect();
    v.sort();
    v.dedup();
    v
}

/// The indicators as text.
pub fn texto(k: &Armazem) -> String {
    let mut t = String::new();
    let _ = writeln!(
        t,
        "{}: {} ({} replicação(ões), média ± IC 95%)",
        k.linha,
        k.modelo,
        k.reps.len()
    );
    let _ = writeln!(t);
    let ls = linhas(k);
    let w = ls.iter().map(|l| l.0.chars().count()).max().unwrap_or(0);
    for (nome, valor) in ls {
        let pad = w - nome.chars().count();
        let _ = writeln!(t, "  {nome}{}  {valor}", " ".repeat(pad));
    }
    let racks = nomes(k, |r| r.porta_paletes.keys().collect());
    if !racks.is_empty() {
        let _ = writeln!(t, "\nPorta-paletes (ocupação média das posições):");
        for p in racks {
            let s = k.resumo_opt(|r| r.porta_paletes.get(&p).copied());
            let _ = writeln!(t, "  {p}: {}", s.map_or("-".into(), |s| pct_hw(&s)));
        }
    }
    let classes = nomes(k, |r| r.acesso.keys().collect());
    if !classes.is_empty() {
        let _ = writeln!(t, "\nTempo de acesso por classe ABC:");
        for c in classes {
            let s = k.resumo_opt(|r| r.acesso.get(&c).copied());
            let _ = writeln!(t, "  {c}: {} min", s.map_or("-".into(), |s| mhw(&s, 1.0)));
        }
    }
    let gente = nomes(k, |r| r.ocupacao.keys().collect());
    if !gente.is_empty() {
        let _ = writeln!(t, "\nOcupação (fração do turno):");
        for p in gente {
            let s = k.resumo_opt(|r| r.ocupacao.get(&p).copied());
            let _ = writeln!(t, "  {p}: {}", s.map_or("-".into(), |s| pct_hw(&s)));
        }
    }
    let docas = nomes(k, |r| r.docas.keys().collect());
    if !docas.is_empty() {
        let _ = writeln!(t, "\nDocas (fração do tempo com caminhão):");
        for d in docas {
            let s = k.resumo_opt(|r| r.docas.get(&d).copied());
            let _ = writeln!(t, "  {d}: {}", s.map_or("-".into(), |s| pct_hw(&s)));
        }
    }
    if !k.linhas_por_hora.is_empty() {
        let _ = writeln!(t, "\nLinhas separadas por hora do dia:");
        for (h, v) in &k.linhas_por_hora {
            let _ = writeln!(t, "  {h:02}h: {}", num1(*v));
        }
    }
    t
}

/// Bars of the lines picked by hour of the day.
fn barras(k: &Armazem) -> String {
    if k.linhas_por_hora.is_empty() {
        return String::new();
    }
    let top = k
        .linhas_por_hora
        .iter()
        .map(|x| x.1)
        .fold(0.0, f64::max)
        .max(1e-9);
    let (w, h, x0, y0) = (720.0, 220.0, 40.0, 190.0);
    let bw = (w - x0 - 10.0) / 24.0;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Linhas separadas por hora do dia\">"
    );
    let _ = write!(
        s,
        "<line class=\"eixo\" x1=\"{x0}\" y1=\"{y0}\" x2=\"{}\" y2=\"{y0}\"/>",
        w - 10.0
    );
    for (hr, v) in &k.linhas_por_hora {
        let bh = (v / top) * (y0 - 20.0);
        let x = x0 + *hr as f64 * bw;
        let _ = write!(
            s,
            "<rect class=\"barra\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\"><title>{:02}h: {}</title></rect>",
            x + 1.0,
            y0 - bh,
            (bw - 2.0).max(1.0),
            bh,
            hr,
            num1(*v)
        );
    }
    for hr in (0..24).step_by(3) {
        let _ = write!(
            s,
            "<text x=\"{:.1}\" y=\"{}\">{hr:02}h</text>",
            x0 + hr as f64 * bw,
            y0 + 16.0
        );
    }
    let _ = write!(
        s,
        "<text x=\"{x0}\" y=\"14\">{} linhas</text></svg>",
        num1(top)
    );
    s
}

fn tabela(
    h: &mut String,
    k: &Armazem,
    titulo: &str,
    coluna: &str,
    nomes_: Vec<String>,
    f: impl Fn(&super::Replicacao, &str) -> Option<f64>,
    pct: bool,
) {
    if nomes_.is_empty() {
        return;
    }
    let _ = write!(
        h,
        "<h2>{}</h2><table><thead><tr><th>Nome</th><th class=\"n\">{}</th></tr></thead><tbody>",
        esc(titulo),
        esc(coluna)
    );
    for n in nomes_ {
        let v = k.resumo_opt(|r| f(r, &n)).map_or("-".into(), |s| {
            if pct {
                pct_hw(&s)
            } else {
                format!("{} min", mhw(&s, 1.0))
            }
        });
        let _ = write!(h, "<tr><td>{}</td><td class=\"n\">{v}</td></tr>", esc(&n));
    }
    h.push_str("</tbody></table>");
}

/// The self-contained HTML page.
pub fn html(k: &Armazem) -> String {
    let mut h = String::new();
    let t = esc(&format!("{}: {}", k.linha, k.modelo));
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>{CSS}</style></head><body><h1>{t}</h1><p class=\"muted\">{} replicação(ões); valores com o intervalo de 95 % entre replicações. Meta de doca: caminhão liberado em até {} min depois de encostar.</p>",
        k.reps.len(),
        num(k.meta.doca)
    );
    h.push_str("<div class=\"cards\">");
    for (nome, valor) in linhas(k) {
        let _ = write!(
            h,
            "<div class=\"card\">{}<b>{}</b></div>",
            esc(nome),
            esc(&valor)
        );
    }
    h.push_str("</div>");
    if !k.linhas_por_hora.is_empty() {
        h.push_str("<h2>Separação por hora do dia</h2><p class=\"muted\">Linhas separadas em cada hora, média dos dias com separação nessa hora, todas as replicações.</p>");
        h.push_str(&barras(k));
    }
    tabela(
        &mut h,
        k,
        "Docas",
        "Tempo com caminhão",
        nomes(k, |r| r.docas.keys().collect()),
        |r, n| r.docas.get(n).copied(),
        true,
    );
    tabela(
        &mut h,
        k,
        "Porta-paletes",
        "Ocupação média das posições",
        nomes(k, |r| r.porta_paletes.keys().collect()),
        |r, n| r.porta_paletes.get(n).copied(),
        true,
    );
    tabela(
        &mut h,
        k,
        "Tempo de acesso por classe ABC",
        "Tempo de acesso",
        nomes(k, |r| r.acesso.keys().collect()),
        |r, n| r.acesso.get(n).copied(),
        false,
    );
    tabela(
        &mut h,
        k,
        "Ocupação das equipes",
        "Ocupada (fração do turno)",
        nomes(k, |r| r.ocupacao.keys().collect()),
        |r, n| r.ocupacao.get(n).copied(),
        true,
    );
    h.push_str("</body></html>");
    h
}
