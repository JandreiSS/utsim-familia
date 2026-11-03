//! Multiple-activity (man-machine) chart: a time column for each machine and operator of a
//! cell, coloured by what it is doing, from the states of the trace.

use std::fmt::Write as _;

use utsim_core::State;
use utsim_model::csv::{num as csvnum, row};
use utsim_model::{Model, ObjectKind};

use crate::desenho::{Parte, curto, svg_doc, texto};
use crate::rastro::Rastro;
use crate::text::{esc, num, pct};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cat {
    Trabalhando,
    Preparando,
    Andando,
    Esperando,
    Ocioso,
    Parado,
}

impl Cat {
    pub const ALL: [Cat; 6] = [
        Cat::Trabalhando,
        Cat::Preparando,
        Cat::Andando,
        Cat::Esperando,
        Cat::Ocioso,
        Cat::Parado,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Cat::Trabalhando => "trabalhando",
            Cat::Preparando => "preparando (setup)",
            Cat::Andando => "andando",
            Cat::Esperando => "esperando (bloqueado, sem operador ou componentes)",
            Cat::Ocioso => "ocioso",
            Cat::Parado => "parado (quebra, fora de turno)",
        }
    }

    pub fn cor(self) -> &'static str {
        match self {
            Cat::Trabalhando => "#16a34a",
            Cat::Preparando => "#ca8a04",
            Cat::Andando => "#2563eb",
            Cat::Esperando => "#c2410c",
            Cat::Ocioso => "#9ca3af",
            Cat::Parado => "#7f1d1d",
        }
    }
}

pub fn categoria(s: State) -> Cat {
    match s {
        State::Processing
        | State::Utilize
        | State::Loading
        | State::Unloading
        | State::Generating => Cat::Trabalhando,
        State::Setup => Cat::Preparando,
        State::TravelEmpty | State::TravelLoaded => Cat::Andando,
        State::Blocked
        | State::WaitingOperator
        | State::WaitingTransport
        | State::Collecting
        | State::Releasing => Cat::Esperando,
        State::Idle | State::Empty => Cat::Ocioso,
        State::Down | State::OffShift | State::Stopped => Cat::Parado,
    }
}

/// `(start, end, category)` stretches of a block inside the window.
pub fn segmentos(estados: &[(f64, State)], w0: f64, w1: f64) -> Vec<(f64, f64, Cat)> {
    let mut out = Vec::new();
    let mut cur = State::Idle;
    let mut t = w0;
    for &(te, s) in estados {
        if te <= w0 {
            cur = s;
            continue;
        }
        if te >= w1 {
            break;
        }
        out.push((t, te, categoria(cur)));
        t = te;
        cur = s;
    }
    if t < w1 {
        out.push((t, w1, categoria(cur)));
    }
    out
}

/// Default window: a tenth of the run after the warm-up.
pub fn janela_padrao(rastro: &Rastro) -> (f64, f64) {
    let a = rastro.aquecimento;
    (a, a + (rastro.parada - a) / 10.0)
}

pub fn gerar(model: &Model, rastro: &Rastro, janela: Option<(f64, f64)>) -> Result<Parte, String> {
    let (w0, w1) = janela.unwrap_or_else(|| janela_padrao(rastro));
    if w1.partial_cmp(&w0) != Some(std::cmp::Ordering::Greater) {
        return Err(
            "o intervalo do gráfico homem-máquina precisa ter fim maior que o início".into(),
        );
    }
    let cols: Vec<usize> = model
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            matches!(
                o.kind,
                ObjectKind::Processor { .. }
                    | ObjectKind::Combiner { .. }
                    | ObjectKind::Separator { .. }
                    | ObjectKind::Transporter { .. }
            )
        })
        .map(|(i, _)| i)
        .collect();
    let (colw, gap, left, top, hgt) = (74.0, 18.0, 70.0, 50.0, 560.0);
    let width = left + cols.len() as f64 * (colw + gap) + 20.0;
    let height = top + hgt + 30.0;
    let y_of = |t: f64| top + hgt * (t - w0) / (w1 - w0);
    let mut b = String::new();
    let mut tab = String::from("<table><thead><tr><th>Recurso</th>");
    for c in Cat::ALL {
        let _ = write!(
            tab,
            "<th class=\"n\">{}</th>",
            esc(c.nome().split(' ').next().unwrap_or(""))
        );
    }
    tab.push_str("<th class=\"n\">Ocupação</th></tr></thead><tbody>");
    let mut csv = String::from("recurso,categoria,fracao\n");
    for k in 0..=5 {
        let t = w0 + (w1 - w0) * k as f64 / 5.0;
        let y = y_of(t);
        let _ = write!(
            b,
            "<line class=\"eixo\" x1=\"{:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" style=\"stroke-opacity:.35\"/>{}",
            left - 6.0,
            width - 10.0,
            texto(4.0, y + 4.0, "etq mut", &num(t))
        );
    }
    for (k, &c) in cols.iter().enumerate() {
        let x = left + k as f64 * (colw + gap);
        let segs = segmentos(&rastro.estados[c], w0, w1);
        let mut by = [0.0f64; 6];
        for &(a, e, cat) in &segs {
            let _ = write!(
                b,
                "<rect x=\"{x:.1}\" y=\"{:.1}\" width=\"{colw}\" height=\"{:.2}\" style=\"fill:{}\"/>",
                y_of(a),
                (y_of(e) - y_of(a)).max(0.3),
                cat.cor()
            );
            by[cat as usize] += e - a;
        }
        let _ = write!(
            b,
            "<rect x=\"{x:.1}\" y=\"{top}\" width=\"{colw}\" height=\"{hgt}\" class=\"fino\"/>{}",
            texto(x, top - 10.0, "etq b", &curto(&rastro.nomes[c], 11))
        );
        let _ = write!(tab, "<tr><td>{}</td>", esc(&rastro.nomes[c]));
        for cat in Cat::ALL {
            let f = by[cat as usize] / (w1 - w0);
            let _ = write!(tab, "<td class=\"n\">{}</td>", pct(f));
            row(
                &mut csv,
                &[rastro.nomes[c].clone(), cat.nome().to_string(), csvnum(f)],
            );
        }
        let occ = (by[Cat::Trabalhando as usize] + by[Cat::Preparando as usize]) / (w1 - w0);
        let _ = write!(tab, "<td class=\"n\"><b>{}</b></td></tr>", pct(occ));
    }
    tab.push_str("</tbody></table>");
    let mut leg = String::from("<div class=\"legenda\">");
    for c in Cat::ALL {
        let _ = write!(
            leg,
            "<span><i style=\"display:inline-block;width:.9rem;height:.9rem;background:{}\"></i>{}</span>",
            c.cor(),
            esc(c.nome())
        );
    }
    leg.push_str("</div>");
    Ok(Parte {
        svg: Some(svg_doc(width, height, "Gráfico homem-máquina", &b, true)),
        tabelas: format!(
            "{leg}<p class=\"muted\">Intervalo de t = {} a t = {}. Ocupação = trabalhando + preparando.</p>{tab}",
            num(w0),
            num(w1)
        ),
        csv: Some(csv),
    })
}
