//! From-to matrix and chord (flow intensity) diagram: trips and distance x trips between
//! pairs of objects.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use utsim_model::csv::{num as csvnum, row};

use crate::desenho::{PALETTE, Parte, curto, svg_doc, texto};
use crate::rastro::Rastro;
use crate::text::{esc, num};

/// One cell of the matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct Par {
    pub de: usize,
    pub para: usize,
    pub viagens: usize,
    /// Sum of the distances of the trips.
    pub dist_total: f64,
}

pub fn pares(rastro: &Rastro) -> Vec<Par> {
    let mut m: BTreeMap<(usize, usize), (usize, f64)> = BTreeMap::new();
    for v in &rastro.viagens {
        let e = m.entry((v.de, v.para)).or_default();
        e.0 += 1;
        e.1 += v.dist;
    }
    m.into_iter()
        .map(|((de, para), (viagens, dist_total))| Par {
            de,
            para,
            viagens,
            dist_total,
        })
        .collect()
}

fn matriz(
    rastro: &Rastro,
    ps: &[Par],
    ids: &[usize],
    titulo: &str,
    valor: impl Fn(&Par) -> f64,
) -> String {
    let max = ps.iter().map(&valor).fold(0.0, f64::max).max(1e-12);
    let mut h = format!(
        "<h3>{}</h3><div class=\"rolagem\"><table><thead><tr><th>de \\ para</th>",
        esc(titulo)
    );
    for &c in ids {
        let _ = write!(
            h,
            "<th class=\"n\">{}</th>",
            esc(&curto(&rastro.nomes[c], 14))
        );
    }
    h.push_str("</tr></thead><tbody>");
    for &a in ids {
        let _ = write!(h, "<tr><td>{}</td>", esc(&rastro.nomes[a]));
        for &c in ids {
            match ps.iter().find(|p| p.de == a && p.para == c) {
                Some(p) => {
                    let v = valor(p);
                    let _ = write!(
                        h,
                        "<td class=\"n\" style=\"background:rgba(37,99,235,{:.2})\">{}</td>",
                        0.12 + 0.55 * v / max,
                        num(v)
                    );
                }
                None => h.push_str("<td class=\"n muted\"></td>"),
            }
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table></div>");
    h
}

fn cordas(rastro: &Rastro, ps: &[Par], ids: &[usize]) -> String {
    let (cx, cy, r) = (360.0, 330.0, 230.0);
    let n = ids.len().max(1);
    let ang = |k: usize| std::f64::consts::TAU * k as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
    let at = |k: usize, rad: f64| (cx + rad * ang(k).cos(), cy + rad * ang(k).sin());
    let max = ps.iter().map(|p| p.viagens).max().unwrap_or(1) as f64;
    let mut b = String::new();
    let _ = write!(
        b,
        "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" class=\"fino\" style=\"stroke-dasharray:3 5\"/>"
    );
    for p in ps {
        let (Some(i), Some(j)) = (
            ids.iter().position(|&x| x == p.de),
            ids.iter().position(|&x| x == p.para),
        ) else {
            continue;
        };
        let (a, c) = (at(i, r), at(j, r));
        let w = 1.0 + 9.0 * p.viagens as f64 / max;
        let color = PALETTE[i % 8];
        if i == j {
            let _ = write!(
                b,
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"14\" style=\"fill:none;stroke:{color};stroke-opacity:.6;stroke-width:{w:.1}\"/>",
                a.0, a.1
            );
            continue;
        }
        let _ = write!(
            b,
            "<path style=\"fill:none;stroke:{color};stroke-opacity:.6;stroke-width:{w:.1}\" d=\"M{:.1} {:.1} Q{cx} {cy} {:.1} {:.1}\"/>\
             <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" style=\"fill:{color}\"/>",
            a.0, a.1, c.0, c.1, c.0, c.1
        );
    }
    for (k, &o) in ids.iter().enumerate() {
        let p = at(k, r);
        let t = at(k, r + 18.0);
        let _ = write!(
            b,
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"6\" style=\"fill:{}\"/>",
            p.0,
            p.1,
            PALETTE[k % 8]
        );
        let right = ang(k).cos() >= -0.1;
        let _ = write!(
            b,
            "<text class=\"etq\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"{}\">{}</text>",
            t.0,
            t.1 + 3.0,
            if right { "start" } else { "end" },
            esc(&curto(&rastro.nomes[o], 20))
        );
    }
    b.push_str(&texto(
        14.0,
        20.0,
        "etq mut",
        "A bolinha marca o destino; a espessura é o número de viagens.",
    ));
    svg_doc(720.0, 660.0, "Diagrama de cordas", &b, true)
}

pub fn gerar(rastro: &Rastro) -> Parte {
    let ps = pares(rastro);
    let mut ids: Vec<usize> = ps.iter().flat_map(|p| [p.de, p.para]).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut tab = matriz(rastro, &ps, &ids, "Viagens", |p| p.viagens as f64);
    tab.push_str(&matriz(
        rastro,
        &ps,
        &ids,
        "Distância x viagens (distância total)",
        |p| p.dist_total,
    ));
    let mut csv = String::from("de,para,viagens,distancia_media,distancia_total\n");
    for p in &ps {
        row(
            &mut csv,
            &[
                rastro.nomes[p.de].clone(),
                rastro.nomes[p.para].clone(),
                p.viagens.to_string(),
                csvnum(p.dist_total / p.viagens.max(1) as f64),
                csvnum(p.dist_total),
            ],
        );
    }
    Parte {
        svg: Some(cordas(rastro, &ps, &ids)),
        tabelas: tab,
        csv: Some(csv),
    }
}
