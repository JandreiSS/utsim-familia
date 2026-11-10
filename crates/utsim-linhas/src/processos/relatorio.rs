//! Report of the line Processos contínuos e batelada: the indicators in the terminal and a
//! self-contained HTML page (cards, the time of every reactor with its cleanings, the table
//! by product, the tanks and the operators).

use std::fmt::Write as _;

use super::{Processos, Replicacao};
use crate::lean_office::relatorio::{CSS, esc};
use crate::lean_office::{mhw, num, pct_hw};

/// The headline indicators; volume and yield only when the batches carry the labels,
/// out of specification only when some batch was.
fn linhas(k: &Processos) -> Vec<(&'static str, String)> {
    let mut v = vec![("Lotes por dia", mhw(&k.resumo(|r| r.lotes_dia), 1.0))];
    if let Some(s) = k.resumo_opt(|r| r.volume_dia) {
        v.push(("Volume produzido por dia", format!("{} L", mhw(&s, 1.0))));
    }
    if let Some(s) = k.resumo_opt(|r| r.rendimento) {
        v.push(("Rendimento médio", pct_hw(&s)));
    }
    if let Some(s) = k.resumo_opt(|r| r.envasadas_dia) {
        v.push(("Unidades envasadas por dia", mhw(&s, 1.0)));
    }
    v.push((
        "Tempo de atravessamento do lote",
        format!("{} min", mhw(&k.resumo(|r| r.atravessamento), 1.0)),
    ));
    if k.reps.iter().any(|r| r.fora > 0) {
        v.push((
            "Lotes fora de especificação",
            pct_hw(&k.resumo(|r| {
                let t = r.lotes + r.fora;
                if t == 0 {
                    0.0
                } else {
                    r.fora as f64 / t as f64
                }
            })),
        ));
    }
    v
}

fn nomes(k: &Processos, f: impl Fn(&Replicacao) -> Vec<&String>) -> Vec<String> {
    let mut v: Vec<String> = k.reps.iter().flat_map(f).cloned().collect();
    v.sort();
    v.dedup();
    v
}

fn produtos(k: &Processos) -> Vec<i64> {
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

/// One row of the reactor table: reactor, making, in cleaning, broken, blocked, without a
/// batch, cleanings per day, mean cycle.
fn linha_reator(k: &Processos, m: &str) -> [String; 8] {
    let p = |f: &dyn Fn(&super::Reator) -> f64| {
        k.resumo_opt(|r| r.reatores.get(m).map(f))
            .map_or("-".into(), |s| pct_hw(&s))
    };
    let limpezas = k
        .resumo_opt(|r| r.reatores.get(m).and_then(|o| o.limpezas_dia))
        .map_or("-".into(), |s| mhw(&s, 1.0));
    let ciclo = k
        .resumo_opt(|r| r.reatores.get(m).map(|o| o.ciclo))
        .map_or("-".into(), |s| format!("{} min", mhw(&s, 1.0)));
    [
        m.to_string(),
        p(&|o| o.produzindo),
        p(&|o| o.em_limpeza),
        p(&|o| o.quebrado),
        p(&|o| o.bloqueado),
        p(&|o| o.sem_lote),
        limpezas,
        ciclo,
    ]
}

const COLUNAS_REATOR: [&str; 8] = [
    "Equipamento",
    "Produzindo",
    "Em limpeza",
    "Quebrado",
    "Bloqueado",
    "Sem lote",
    "Limpezas por dia",
    "Ciclo do lote",
];

/// One row of the table by product: product, batches, volume, lead time.
fn linha_produto(k: &Processos, p: i64) -> [String; 4] {
    let lotes = k
        .resumo_opt(|r| r.por_produto.get(&p).map(|x| x.lotes as f64))
        .map_or("-".into(), |s| mhw(&s, 1.0));
    let volume = k
        .resumo_opt(|r| r.por_produto.get(&p).map(|x| x.volume))
        .map_or("-".into(), |s| format!("{} L", mhw(&s, 1.0)));
    let lead = k
        .resumo_opt(|r| r.por_produto.get(&p).map(|x| x.atravessamento))
        .map_or("-".into(), |s| format!("{} min", mhw(&s, 1.0)));
    [format!("produto {p}"), lotes, volume, lead]
}

const COLUNAS_PRODUTO: [&str; 4] = [
    "Produto",
    "Lotes no período",
    "Volume no período",
    "Atravessamento",
];

/// A fraction with its interval, or "-" when no replication has it.
fn fracao(k: &Processos, f: impl Fn(&Replicacao) -> Option<f64>) -> String {
    k.resumo_opt(f).map_or("-".into(), |s| pct_hw(&s))
}

/// The indicators as text.
pub fn texto(k: &Processos) -> String {
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
    if k.resumo_opt(|r| r.rendimento).is_some() {
        let _ = writeln!(t, "  (alvo de rendimento: {})", pct(k.meta.rendimento));
    }
    let rs = nomes(k, |r| r.reatores.keys().collect());
    if !rs.is_empty() {
        let _ = writeln!(t, "\nTempo dos equipamentos (fração do tempo de turno):");
        for m in rs {
            let l = linha_reator(k, &m);
            let _ = writeln!(
                t,
                "  {}: produzindo {}, em limpeza {}, quebrado {}, bloqueado {}, sem lote {}; limpezas por dia {}, ciclo do lote {}",
                l[0], l[1], l[2], l[3], l[4], l[5], l[6], l[7]
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
                "  {}: {} lotes no período, {} no período, atravessamento {}",
                l[0], l[1], l[2], l[3]
            );
        }
    }
    let ts = nomes(k, |r| r.tanques.keys().collect());
    if !ts.is_empty() {
        let _ = writeln!(t, "\nNível médio dos tanques (fração da capacidade):");
        for n in ts {
            let s = fracao(k, |r| r.tanques.get(&n).copied());
            let _ = writeln!(t, "  {n}: {s}");
        }
    }
    let gente = nomes(k, |r| r.ocupacao.keys().collect());
    if !gente.is_empty() {
        let _ = writeln!(t, "\nOcupação dos operadores (fração do turno):");
        for p in gente {
            let s = fracao(k, |r| r.ocupacao.get(&p).copied());
            let _ = writeln!(t, "  {p}: {s}");
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
pub fn html(k: &Processos) -> String {
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
    let rs = nomes(k, |r| r.reatores.keys().collect());
    tabela(
        &mut h,
        "Tempo dos equipamentos",
        "Frações do tempo de turno. Em limpeza: a limpeza entre produtos (CIP). Bloqueado: o lote está pronto, mas não há lugar adiante (tanque cheio). Ciclo do lote: tempo médio do lote dentro do equipamento.",
        &COLUNAS_REATOR,
        rs.iter().map(|m| linha_reator(k, m)).collect(),
    );
    tabela(
        &mut h,
        "Por produto",
        "Volume = volume do lote × rendimento.",
        &COLUNAS_PRODUTO,
        produtos(k)
            .into_iter()
            .map(|p| linha_produto(k, p))
            .collect(),
    );
    let ts = nomes(k, |r| r.tanques.keys().collect());
    tabela(
        &mut h,
        "Tanques",
        "Nível médio como fração da capacidade.",
        &["Tanque", "Nível médio"],
        ts.iter()
            .map(|n| [n.clone(), fracao(k, |r| r.tanques.get(n).copied())])
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
            .map(|p| [p.clone(), fracao(k, |r| r.ocupacao.get(p).copied())])
            .collect(),
    );
    h.push_str("</body></html>");
    h
}
