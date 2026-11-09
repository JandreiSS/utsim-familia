//! Report of the line Saúde e hospital: the indicators in the terminal and a
//! self-contained HTML page (cards, the table by colour of the risk classification, the
//! census by hour of the day, the teams and the beds).

use std::fmt::Write as _;

use super::{Saude, cor};
use crate::lean_office::relatorio::{CSS, esc};
use crate::lean_office::{mhw, num, num1, pct_hw};

/// The headline indicators; beds only when the model has bed units, no-shows only when
/// someone missed an appointment.
fn linhas(k: &Saude) -> Vec<(&'static str, String)> {
    let mut v = vec![(
        "Pacientes atendidos por dia",
        mhw(&k.resumo(|r| r.por_dia), 1.0),
    )];
    if let Some(s) = k.resumo_opt(|r| r.espera) {
        v.push((
            "Espera até o médico (depois da classificação)",
            format!("{} min", mhw(&s, 1.0)),
        ));
    }
    if let Some(s) = k.resumo_opt(|r| r.porta_medico) {
        v.push(("Da chegada ao médico", format!("{} min", mhw(&s, 1.0))));
    }
    if let Some(s) = k.resumo_opt(|r| r.no_alvo) {
        v.push(("Atendidos no tempo-alvo da cor", pct_hw(&s)));
    }
    if k.reps.iter().any(|r| r.abandonos > 0 || r.consultas > 0) {
        v.push(("Saíram sem atendimento", pct_hw(&k.resumo(|r| r.abandono))));
    }
    v.push((
        "Permanência no serviço",
        format!("{} min", mhw(&k.resumo(|r| r.permanencia), 1.0)),
    ));
    if k.reps.iter().any(|r| r.faltas > 0) {
        v.push((
            "Faltas na agenda por dia",
            mhw(&k.resumo(|r| r.faltas_dia), 1.0),
        ));
    }
    if let Some(s) = k.resumo_opt(|r| r.espera_leito) {
        v.push(("Espera por leito", format!("{} min", mhw(&s, 1.0))));
    }
    if let Some(s) = k.resumo_opt(|r| r.internacao) {
        v.push(("Permanência no leito", format!("{} min", mhw(&s, 1.0))));
    }
    v
}

fn nomes(k: &Saude, f: impl Fn(&super::Replicacao) -> Vec<&String>) -> Vec<String> {
    let mut v: Vec<String> = k.reps.iter().flat_map(f).cloned().collect();
    v.sort();
    v.dedup();
    v
}

fn gravidades(k: &Saude) -> Vec<i64> {
    let mut v: Vec<i64> = k
        .reps
        .iter()
        .flat_map(|r| r.por_cor.keys().copied())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// One row of the table by colour: name, target, seen, wait, within target, left, stay.
fn linha_cor(k: &Saude, g: i64) -> [String; 7] {
    let c = |f: &dyn Fn(&super::Cor) -> f64| k.resumo_opt(|r| r.por_cor.get(&g).map(f));
    let alvo = k
        .meta
        .alvo_de(g)
        .map_or("-".into(), |a| format!("{} min", num(a)));
    let no_alvo = k
        .resumo_opt(|r| r.por_cor.get(&g).and_then(|c| c.no_alvo))
        .map_or("-".into(), |s| pct_hw(&s));
    let n = |s: Option<utsim_core::stats::Summary>, min: bool| {
        s.map_or("-".into(), |s| {
            if min {
                format!("{} min", mhw(&s, 1.0))
            } else {
                mhw(&s, 1.0)
            }
        })
    };
    [
        cor(g),
        alvo,
        n(c(&|c| c.atendidos as f64), false),
        n(
            k.resumo_opt(|r| r.por_cor.get(&g).and_then(|c| c.espera)),
            true,
        ),
        no_alvo,
        n(c(&|c| c.abandonos as f64), false),
        n(c(&|c| c.permanencia), true),
    ]
}

const COLUNAS: [&str; 7] = [
    "Cor",
    "Tempo-alvo",
    "Atendidos no período",
    "Espera até o médico",
    "No alvo",
    "Saíram sem atendimento",
    "Permanência",
];

/// The indicators as text.
pub fn texto(k: &Saude) -> String {
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
    let gs = gravidades(k);
    if !gs.is_empty() {
        let _ = writeln!(t, "\nPor cor da classificação de risco:");
        for g in gs {
            let l = linha_cor(k, g);
            let _ = writeln!(
                t,
                "  {} (alvo {}): {} atendidos no período, espera {}, no alvo {}, saíram {}, permanência {}",
                l[0], l[1], l[2], l[3], l[4], l[5], l[6]
            );
        }
    }
    let gente = nomes(k, |r| r.ocupacao.keys().collect());
    if !gente.is_empty() {
        let _ = writeln!(t, "\nOcupação (fração do plantão):");
        for p in gente {
            let s = k.resumo_opt(|r| r.ocupacao.get(&p).copied());
            let _ = writeln!(t, "  {p}: {}", s.map_or("-".into(), |s| pct_hw(&s)));
        }
    }
    let leitos = nomes(k, |r| r.leitos.keys().collect());
    if !leitos.is_empty() {
        let _ = writeln!(
            t,
            "\nOcupação dos leitos (alvo: até {}):",
            pct(k.meta.ocupacao)
        );
        for p in leitos {
            let s = k.resumo_opt(|r| r.leitos.get(&p).copied());
            let _ = writeln!(t, "  {p}: {}", s.map_or("-".into(), |s| pct_hw(&s)));
        }
    }
    if !k.censo.is_empty() {
        let _ = writeln!(t, "\nPacientes presentes por hora do dia:");
        for (h, v) in &k.censo {
            let _ = writeln!(t, "  {h:02}h: {}", num1(*v));
        }
    }
    t
}

fn pct(x: f64) -> String {
    format!("{} %", num(x * 100.0))
}

/// Bars of the census by hour of the day.
fn barras(k: &Saude) -> String {
    if k.censo.is_empty() {
        return String::new();
    }
    let top = k.censo.iter().map(|x| x.1).fold(0.0, f64::max).max(1e-9);
    let (w, h, x0, y0) = (720.0, 220.0, 40.0, 190.0);
    let bw = (w - x0 - 10.0) / 24.0;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Pacientes presentes por hora do dia\">"
    );
    let _ = write!(
        s,
        "<line class=\"eixo\" x1=\"{x0}\" y1=\"{y0}\" x2=\"{}\" y2=\"{y0}\"/>",
        w - 10.0
    );
    for (hr, v) in &k.censo {
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
        "<text x=\"{x0}\" y=\"14\">{} pacientes</text></svg>",
        num1(top)
    );
    s
}

fn tabela(
    h: &mut String,
    k: &Saude,
    titulo: &str,
    coluna: &str,
    nomes_: Vec<String>,
    f: impl Fn(&super::Replicacao, &str) -> Option<f64>,
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
        let v = k
            .resumo_opt(|r| f(r, &n))
            .map_or("-".into(), |s| pct_hw(&s));
        let _ = write!(h, "<tr><td>{}</td><td class=\"n\">{v}</td></tr>", esc(&n));
    }
    h.push_str("</tbody></table>");
}

/// The self-contained HTML page.
pub fn html(k: &Saude) -> String {
    let mut h = String::new();
    let t = esc(&format!("{}: {}", k.linha, k.modelo));
    let alvos: Vec<String> = (1..=5)
        .map(|g| format!("{} {} min", cor(g), num(k.meta.alvo[g as usize - 1])))
        .collect();
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>{CSS}</style></head><body><h1>{t}</h1><p class=\"muted\">{} replicação(ões); valores com o intervalo de 95 % entre replicações. A espera até o médico conta do fim da classificação de risco. Tempos-alvo: {}.</p>",
        k.reps.len(),
        esc(&alvos.join(", "))
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
    let gs = gravidades(k);
    if !gs.is_empty() {
        h.push_str("<h2>Por cor da classificação de risco</h2><table><thead><tr>");
        for (i, c) in COLUNAS.iter().enumerate() {
            let class = if i == 0 { "" } else { " class=\"n\"" };
            let _ = write!(h, "<th{class}>{}</th>", esc(c));
        }
        h.push_str("</tr></thead><tbody>");
        for g in gs {
            h.push_str("<tr>");
            for (i, v) in linha_cor(k, g).iter().enumerate() {
                let class = if i == 0 { "" } else { " class=\"n\"" };
                let _ = write!(h, "<td{class}>{}</td>", esc(v));
            }
            h.push_str("</tr>");
        }
        h.push_str("</tbody></table>");
    }
    if !k.censo.is_empty() {
        h.push_str("<h2>Pacientes presentes por hora do dia</h2><p class=\"muted\">Média de pacientes no serviço em cada hora do dia, todas as replicações.</p>");
        h.push_str(&barras(k));
    }
    tabela(
        &mut h,
        k,
        &format!("Leitos (alvo: até {})", pct(k.meta.ocupacao)),
        "Ocupação média",
        nomes(k, |r| r.leitos.keys().collect()),
        |r, n| r.leitos.get(n).copied(),
    );
    tabela(
        &mut h,
        k,
        "Ocupação das equipes",
        "Ocupada (fração do plantão)",
        nomes(k, |r| r.ocupacao.keys().collect()),
        |r, n| r.ocupacao.get(n).copied(),
    );
    h.push_str("</body></html>");
    h
}
