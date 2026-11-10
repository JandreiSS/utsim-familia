//! Report of the line Alimentação e food service: the indicators in the terminal and a
//! self-contained HTML page (cards, the table by channel, the kitchen stations, the dining
//! rooms, the orders by hour of the day and the staff).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{Alimentacao, Replicacao};
use crate::lean_office::relatorio::{CSS, esc};
use crate::lean_office::{mhw, num, pct_hw};

/// Name of a channel.
pub fn canal(c: i64) -> String {
    match c {
        1 => "Balcão".into(),
        2 => "Drive-thru".into(),
        3 => "Delivery".into(),
        _ => format!("canal {c}"),
    }
}

/// Minutes without decimals when whole ("15"), with them otherwise ("7,50").
fn minutos(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 && x.abs() < 1e9 {
        format!("{}", x.round() as i64)
    } else {
        num(x)
    }
}

/// The busiest hour of the day, by the mean over the replications of the orders per day
/// that arrived in it.
fn hora_pico(k: &Alimentacao) -> Option<(usize, f64)> {
    let mut soma: BTreeMap<usize, f64> = BTreeMap::new();
    for r in &k.reps {
        for (h, x) in &r.por_hora {
            *soma.entry(*h).or_default() += x;
        }
    }
    let n = k.reps.len().max(1) as f64;
    soma.into_iter()
        .map(|(h, x)| (h, x / n))
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
}

/// The headline indicators; gave up and thrown away only when some order did.
fn linhas(k: &Alimentacao) -> Vec<(String, String)> {
    let mut v = vec![(
        "Pedidos por dia".to_string(),
        mhw(&k.resumo(|r| r.pedidos_dia), 1.0),
    )];
    if let Some((h, _)) = hora_pico(k)
        && let Some(s) = k.resumo_opt(|r| r.por_hora.iter().find(|x| x.0 == h).map(|x| x.1))
    {
        v.push((
            format!("Pedidos na hora de pico ({h}h às {}h)", h + 1),
            mhw(&s, 1.0),
        ));
    }
    v.push((
        "Tempo do pedido".to_string(),
        format!("{} min", mhw(&k.resumo(|r| r.tempo), 1.0)),
    ));
    v.push((
        "Tempo do pedido (90 % dos pedidos)".to_string(),
        format!("{} min", mhw(&k.resumo(|r| r.tempo_p90), 1.0)),
    ));
    v.push((
        format!("Pedidos no prazo (até {} min)", minutos(k.meta.prazo)),
        pct_hw(&k.resumo(|r| r.no_prazo)),
    ));
    if k.reps.iter().any(|r| r.desistencias > 0) {
        v.push((
            "Desistências".to_string(),
            pct_hw(&k.resumo(|r| r.desistencia)),
        ));
    }
    if k.reps.iter().any(|r| r.descartes > 0) {
        v.push((
            "Pedidos descartados por dia (comida fria)".to_string(),
            mhw(
                &k.resumo(|r| {
                    if r.pedidos == 0 {
                        0.0
                    } else {
                        r.descartes as f64 * r.pedidos_dia / r.pedidos as f64
                    }
                }),
                1.0,
            ),
        ));
    }
    v
}

fn nomes(k: &Alimentacao, f: impl Fn(&Replicacao) -> Vec<&String>) -> Vec<String> {
    let mut v: Vec<String> = k.reps.iter().flat_map(f).cloned().collect();
    v.sort();
    v.dedup();
    v
}

fn canais(k: &Alimentacao) -> Vec<i64> {
    let mut v: Vec<i64> = k
        .reps
        .iter()
        .flat_map(|r| r.por_canal.keys().copied())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// A fraction with its interval, or "-" when no replication has it.
fn fracao(k: &Alimentacao, f: impl Fn(&Replicacao) -> Option<f64>) -> String {
    k.resumo_opt(f).map_or("-".into(), |s| pct_hw(&s))
}

fn valor(k: &Alimentacao, f: impl Fn(&Replicacao) -> Option<f64>, unidade: &str) -> String {
    k.resumo_opt(f)
        .map_or("-".into(), |s| format!("{}{unidade}", mhw(&s, 1.0)))
}

/// One row of the table by channel.
fn linha_canal(k: &Alimentacao, c: i64) -> [String; 6] {
    let g = |r: &Replicacao| r.por_canal.get(&c).cloned();
    [
        canal(c),
        valor(k, |r| g(r).map(|x| x.pedidos as f64), ""),
        valor(
            k,
            |r| g(r).filter(|x| x.pedidos > 0).map(|x| x.tempo),
            " min",
        ),
        fracao(k, |r| g(r).filter(|x| x.pedidos > 0).map(|x| x.no_prazo)),
        valor(k, |r| g(r).map(|x| x.desistencias as f64), ""),
        valor(k, |r| g(r).map(|x| x.descartes as f64), ""),
    ]
}

const COLUNAS_CANAL: [&str; 6] = [
    "Canal",
    "Pedidos no período",
    "Tempo do pedido",
    "No prazo",
    "Desistências",
    "Descartes",
];

/// One row of the station table.
fn linha_estacao(k: &Alimentacao, m: &str) -> [String; 7] {
    let p = |f: &dyn Fn(&super::Estacao) -> f64| fracao(k, |r| r.estacoes.get(m).map(f));
    [
        m.to_string(),
        p(&|o| o.ocupacao),
        p(&|o| o.parada),
        p(&|o| o.bloqueada),
        p(&|o| o.quebrada),
        valor(k, |r| r.estacoes.get(m).map(|o| o.feitas as f64), ""),
        valor(k, |r| r.estacoes.get(m).map(|o| o.tempo), " min"),
    ]
}

const COLUNAS_ESTACAO: [&str; 7] = [
    "Estação",
    "Ocupação",
    "Parada",
    "Bloqueada",
    "Quebrada",
    "Feitas no período",
    "Tempo na estação",
];

/// One row of the dining room table.
fn linha_salao(k: &Alimentacao, m: &str) -> [String; 5] {
    let mesas = k
        .reps
        .iter()
        .find_map(|r| r.mesas.get(m).map(|x| x.mesas))
        .map_or("-".into(), |n| n.to_string());
    [
        m.to_string(),
        mesas,
        fracao(k, |r| r.mesas.get(m).map(|x| x.ocupacao)),
        valor(k, |r| r.mesas.get(m).map(|x| x.giro_dia), ""),
        valor(k, |r| r.mesas.get(m).map(|x| x.refeicao), " min"),
    ]
}

const COLUNAS_SALAO: [&str; 5] = [
    "Salão",
    "Mesas",
    "Ocupação",
    "Giro por mesa por dia",
    "Tempo à mesa",
];

/// The open hours (any replication) with the mean orders per day that arrived in each.
fn horas(k: &Alimentacao) -> Vec<[String; 2]> {
    let mut hs: Vec<usize> = k
        .reps
        .iter()
        .flat_map(|r| r.por_hora.iter().map(|x| x.0))
        .collect();
    hs.sort();
    hs.dedup();
    hs.into_iter()
        .map(|h| {
            [
                format!("{h}h às {}h", h + 1),
                valor(
                    k,
                    |r| Some(r.por_hora.iter().find(|x| x.0 == h).map_or(0.0, |x| x.1)),
                    "",
                ),
            ]
        })
        .collect()
}

/// The indicators as text.
pub fn texto(k: &Alimentacao) -> String {
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
    let cs = canais(k);
    if cs.len() > 1 {
        let _ = writeln!(t, "\nPor canal:");
        for c in cs {
            let l = linha_canal(k, c);
            let _ = writeln!(
                t,
                "  {}: {} pedidos no período, tempo {}, no prazo {}, desistências {}, descartes {}",
                l[0], l[1], l[2], l[3], l[4], l[5]
            );
        }
    }
    let es = nomes(k, |r| r.estacoes.keys().collect());
    if !es.is_empty() {
        let _ = writeln!(t, "\nEstações (fração do horário de funcionamento):");
        for m in es {
            let l = linha_estacao(k, &m);
            let _ = writeln!(
                t,
                "  {}: ocupação {}, parada {}, bloqueada {}, quebrada {}; {} feitas no período, {} na estação",
                l[0], l[1], l[2], l[3], l[4], l[5], l[6]
            );
        }
    }
    let ss = nomes(k, |r| r.mesas.keys().collect());
    if !ss.is_empty() {
        let _ = writeln!(t, "\nSalões:");
        for m in ss {
            let l = linha_salao(k, &m);
            let _ = writeln!(
                t,
                "  {}: {} mesas, ocupação {}, giro por mesa por dia {}, tempo à mesa {}",
                l[0], l[1], l[2], l[3], l[4]
            );
        }
    }
    let gente = nomes(k, |r| r.ocupacao.keys().collect());
    if !gente.is_empty() {
        let _ = writeln!(t, "\nOcupação da equipe (fração do turno):");
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
pub fn html(k: &Alimentacao) -> String {
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
            esc(&nome),
            esc(&valor)
        );
    }
    h.push_str("</div>");
    let cs = canais(k);
    if cs.len() > 1 {
        tabela(
            &mut h,
            "Por canal",
            "Tempo do pedido: da chegada do pedido até a saída.",
            &COLUNAS_CANAL,
            cs.into_iter().map(|c| linha_canal(k, c)).collect(),
        );
    }
    let es = nomes(k, |r| r.estacoes.keys().collect());
    tabela(
        &mut h,
        "Estações",
        "Frações do turno da estação ou, sem turno próprio, do horário de funcionamento (quando a loja recebe pedidos). Ocupação: lugares trabalhando ÷ lugares. Bloqueada: a parte está pronta, mas não há lugar adiante.",
        &COLUNAS_ESTACAO,
        es.iter().map(|m| linha_estacao(k, m)).collect(),
    );
    let ss = nomes(k, |r| r.mesas.keys().collect());
    tabela(
        &mut h,
        "Salões",
        "Ocupação: mesas ocupadas em média ÷ mesas, no turno do salão (ou no horário de funcionamento). Giro: grupos sentados por mesa por dia.",
        &COLUNAS_SALAO,
        ss.iter().map(|m| linha_salao(k, m)).collect(),
    );
    tabela(
        &mut h,
        "Pedidos por hora do dia",
        "Pedidos por dia que chegaram em cada hora.",
        &["Hora", "Pedidos por dia"],
        horas(k),
    );
    let gente = nomes(k, |r| r.ocupacao.keys().collect());
    tabela(
        &mut h,
        "Ocupação da equipe",
        "Fração do turno andando ou trabalhando.",
        &["Pessoa", "Ocupada"],
        gente
            .iter()
            .map(|p| [p.clone(), fracao(k, |r| r.ocupacao.get(p).copied())])
            .collect(),
    );
    h.push_str("</body></html>");
    h
}
