//! Report of the line Manufatura discreta: the indicators in the terminal and a
//! self-contained HTML page (cards, the OEE of every machine with its three factors and the
//! changeovers, the table by product and the operators).

use std::fmt::Write as _;

use super::{Manufatura, Replicacao};
use crate::lean_office::relatorio::{CSS, esc};
use crate::lean_office::{mhw, num, pct_hw};

/// The headline indicators; on time only when the orders have a due date, scrap only when
/// some order was scrapped.
fn linhas(k: &Manufatura) -> Vec<(&'static str, String)> {
    let mut v = vec![
        ("Ordens por dia", mhw(&k.resumo(|r| r.por_dia), 1.0)),
        (
            "Tempo de atravessamento",
            format!("{} min", mhw(&k.resumo(|r| r.atravessamento), 1.0)),
        ),
        ("Material em processo", mhw(&k.resumo(|r| r.wip), 1.0)),
    ];
    if let Some(s) = k.resumo_opt(|r| r.no_prazo) {
        v.push(("Ordens no prazo", pct_hw(&s)));
    }
    if let Some(s) = k.resumo_opt(|r| r.atraso) {
        v.push((
            "Atraso médio das atrasadas",
            format!("{} min", mhw(&s, 1.0)),
        ));
    }
    if k.reps.iter().any(|r| r.refugo > 0) {
        v.push((
            "Refugo",
            pct_hw(&k.resumo(|r| {
                let t = r.ordens + r.refugo;
                if t == 0 {
                    0.0
                } else {
                    r.refugo as f64 / t as f64
                }
            })),
        ));
    }
    v
}

fn nomes(k: &Manufatura, f: impl Fn(&Replicacao) -> Vec<&String>) -> Vec<String> {
    let mut v: Vec<String> = k.reps.iter().flat_map(f).cloned().collect();
    v.sort();
    v.dedup();
    v
}

fn produtos(k: &Manufatura) -> Vec<i64> {
    let mut v: Vec<i64> = k
        .reps
        .iter()
        .flat_map(|r| r.por_produto.keys().copied())
        .collect();
    v.sort();
    v.dedup();
    v
}

fn pct(x: f64) -> String {
    format!("{} %", num(x * 100.0))
}

/// One row of the OEE table: machine, OEE, availability, performance, quality, in
/// changeover, changeovers per day.
fn linha_maquina(k: &Manufatura, m: &str) -> [String; 7] {
    let p = |f: &dyn Fn(&super::Oee) -> f64| {
        k.resumo_opt(|r| r.maquinas.get(m).map(f))
            .map_or("-".into(), |s| pct_hw(&s))
    };
    let trocas = k
        .resumo_opt(|r| r.maquinas.get(m).and_then(|o| o.trocas_dia))
        .map_or("-".into(), |s| mhw(&s, 1.0));
    [
        m.to_string(),
        p(&|o| o.oee),
        p(&|o| o.disponibilidade),
        p(&|o| o.desempenho),
        p(&|o| o.qualidade),
        p(&|o| o.em_troca),
        trocas,
    ]
}

const COLUNAS_OEE: [&str; 7] = [
    "Máquina",
    "OEE",
    "Disponibilidade",
    "Desempenho",
    "Qualidade",
    "Em troca",
    "Trocas por dia",
];

/// One row of the table by product: product, orders, lead time, on time.
fn linha_produto(k: &Manufatura, p: i64) -> [String; 4] {
    let ordens = k
        .resumo_opt(|r| r.por_produto.get(&p).map(|x| x.ordens as f64))
        .map_or("-".into(), |s| mhw(&s, 1.0));
    let lead = k
        .resumo_opt(|r| r.por_produto.get(&p).map(|x| x.atravessamento))
        .map_or("-".into(), |s| format!("{} min", mhw(&s, 1.0)));
    let prazo = k
        .resumo_opt(|r| r.por_produto.get(&p).and_then(|x| x.no_prazo))
        .map_or("-".into(), |s| pct_hw(&s));
    [format!("produto {p}"), ordens, lead, prazo]
}

const COLUNAS_PRODUTO: [&str; 4] = ["Produto", "Ordens no período", "Atravessamento", "No prazo"];

/// The indicators as text.
pub fn texto(k: &Manufatura) -> String {
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
    let ms = nomes(k, |r| r.maquinas.keys().collect());
    if !ms.is_empty() {
        let _ = writeln!(t, "\nOEE por máquina (alvo: {}):", pct(k.meta.oee));
        for m in ms {
            let l = linha_maquina(k, &m);
            let _ = writeln!(
                t,
                "  {}: OEE {} (disponibilidade {}, desempenho {}, qualidade {}), em troca {}, trocas por dia {}",
                l[0], l[1], l[2], l[3], l[4], l[5], l[6]
            );
        }
    }
    let ps = produtos(k);
    if !ps.is_empty() {
        let _ = writeln!(t, "\nPor produto:");
        for p in ps {
            let l = linha_produto(k, p);
            let _ = writeln!(
                t,
                "  {}: {} ordens no período, atravessamento {}, no prazo {}",
                l[0], l[1], l[2], l[3]
            );
        }
    }
    let gente = nomes(k, |r| r.ocupacao.keys().collect());
    if !gente.is_empty() {
        let _ = writeln!(t, "\nOcupação dos operadores (fração do turno):");
        for p in gente {
            let s = k.resumo_opt(|r| r.ocupacao.get(&p).copied());
            let _ = writeln!(t, "  {p}: {}", s.map_or("-".into(), |s| pct_hw(&s)));
        }
    }
    t
}

fn tabela<const N: usize>(
    h: &mut String,
    titulo: &str,
    nota: &str,
    colunas: &[&str; N],
    linhas: Vec<[String; N]>,
) {
    if linhas.is_empty() {
        return;
    }
    let _ = write!(h, "<h2>{}</h2>", esc(titulo));
    if !nota.is_empty() {
        let _ = write!(h, "<p class=\"muted\">{}</p>", esc(nota));
    }
    h.push_str("<table><thead><tr>");
    for (i, c) in colunas.iter().enumerate() {
        let class = if i == 0 { "" } else { " class=\"n\"" };
        let _ = write!(h, "<th{class}>{}</th>", esc(c));
    }
    h.push_str("</tr></thead><tbody>");
    for l in linhas {
        h.push_str("<tr>");
        for (i, v) in l.iter().enumerate() {
            let class = if i == 0 { "" } else { " class=\"n\"" };
            let _ = write!(h, "<td{class}>{}</td>", esc(v));
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table>");
}

/// The self-contained HTML page.
pub fn html(k: &Manufatura) -> String {
    let mut h = String::new();
    let t = esc(&format!("{}: {}", k.linha, k.modelo));
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>{CSS}</style></head><body><h1>{t}</h1><p class=\"muted\">{} replicação(ões); valores com o intervalo de 95 % entre replicações.</p>",
        k.reps.len()
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
    let ms = nomes(k, |r| r.maquinas.keys().collect());
    tabela(
        &mut h,
        &format!("OEE por máquina (alvo: {})", pct(k.meta.oee)),
        "Disponibilidade: tempo sem quebra e sem troca ÷ tempo de turno. Desempenho: tempo de ciclo médio × peças ÷ tempo rodando (perde com falta de peça e bloqueio). Qualidade: ordens boas ÷ todas, do modelo inteiro.",
        &COLUNAS_OEE,
        ms.iter().map(|m| linha_maquina(k, m)).collect(),
    );
    tabela(
        &mut h,
        "Por produto",
        "",
        &COLUNAS_PRODUTO,
        produtos(k)
            .into_iter()
            .map(|p| linha_produto(k, p))
            .collect(),
    );
    let gente = nomes(k, |r| r.ocupacao.keys().collect());
    tabela(
        &mut h,
        "Ocupação dos operadores",
        "Fração do turno andando ou operando.",
        &["Operador", "Ocupado"],
        gente
            .iter()
            .map(|p| {
                [
                    p.clone(),
                    k.resumo_opt(|r| r.ocupacao.get(p).copied())
                        .map_or("-".into(), |s| pct_hw(&s)),
                ]
            })
            .collect(),
    );
    h.push_str("</body></html>");
    h
}
