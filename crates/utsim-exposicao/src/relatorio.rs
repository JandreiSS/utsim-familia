//! Self-contained HTML report of the exposure, always with the legal notice.

use std::collections::BTreeMap;
use std::fmt::Write;

use utsim_model::Model;

use crate::{Exposure, interval};

/// Notice every occupational safety report of the family carries (requirement QS-RF-08).
pub const AVISO_LEGAL: &str = "O UTSim apoia o estudo, o ensino e a organização das \
informações. Ele não substitui o profissional legalmente habilitado, não emite laudo e não \
certifica conformidade com nenhuma Norma Regulamentadora. O cumprimento da legislação é \
responsabilidade do usuário e da empresa.";

/// Notice of the simulated near-conflicts.
pub const AVISO_SIMULADO: &str = "Quase-conflitos simulados servem para comparar layouts e \
regras de circulação do mesmo modelo. Eles não estimam a probabilidade de um acidente real.";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn fmt(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}").replace('.', ",")
    } else {
        "–".into()
    }
}

/// Report over the replications of one model.
pub fn html(model: &Model, reps: &[Exposure]) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>Exposição: {n}</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:24px;max-width:960px;color:#222}}\
         table{{border-collapse:collapse;margin:8px 0 20px}}td,th{{border:1px solid #ccc;padding:4px 10px}}\
         .aviso{{border:2px solid #c0392b;padding:8px 12px;background:#fdf2f0}}</style></head><body>\
         <h1>Exposição a riscos na simulação</h1><p>Modelo <b>{n}</b>, {r} replicações. Origem: \
         <b>Simulado</b>. Limiar de quase-conflito {lim} m.</p><p class=\"aviso\">{aviso}</p>",
        n = esc(&model.name),
        r = reps.len(),
        lim = fmt(reps
            .first()
            .map_or(crate::NEAR_MISS_DISTANCE, |e| e.options.near_miss)),
        aviso = AVISO_LEGAL
    );
    let mut por: BTreeMap<(String, String), (Vec<f64>, Vec<f64>, Vec<f64>, Vec<String>)> =
        BTreeMap::new();
    for e in reps {
        for z in &e.zones {
            let k = por.entry((z.zone.clone(), z.person.clone())).or_default();
            k.0.push(z.time);
            k.1.push(f64::from(z.entries));
            k.2.push(f64::from(z.entries_without_ppe));
            k.3 = z.missing_ppe.clone();
        }
    }
    s.push_str(
        "<h2>Tempo em zona de risco</h2><table><tr><th>Zona</th><th>Pessoa</th>\
         <th>Tempo (média ± IC 95 %)</th><th>Entradas</th><th>Entradas sem EPI</th><th>EPI que falta</th></tr>",
    );
    for ((zona, pessoa), (t, ent, sem, falta)) in &por {
        let (t, ent, sem) = (interval(t), interval(ent), interval(sem));
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{}</td><td>{} ± {}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(zona),
            esc(pessoa),
            fmt(t.mean),
            fmt(t.half_width),
            fmt(ent.mean),
            fmt(sem.mean),
            esc(&falta.join(", "))
        );
    }
    s.push_str("</table>");
    let n: Vec<f64> = reps.iter().map(|e| e.near_miss_count() as f64).collect();
    let i = interval(&n);
    let _ = write!(
        s,
        "<h2>Quase-conflitos simulados</h2><p>{} ± {} por replicação.</p><p class=\"aviso\">{}</p>",
        fmt(i.mean),
        fmt(i.half_width),
        AVISO_SIMULADO
    );
    if let Some(e) = reps.first() {
        s.push_str("<table><tr><th>Instante</th><th>Pessoa</th><th>Veículo</th><th>Menor distância (m)</th><th>Local</th></tr>");
        for nm in e.near_misses.iter().take(50) {
            let _ = write!(
                s,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>({}, {})</td></tr>",
                fmt(nm.t),
                esc(&nm.person),
                esc(&nm.vehicle),
                fmt(nm.min_distance),
                fmt(nm.at[0]),
                fmt(nm.at[1])
            );
        }
        s.push_str("</table>");
        s.push_str("<h2>Mapa de calor da presença (replicação 1)</h2>");
        s.push_str(&heat_svg(e));
        if !e.warnings.is_empty() {
            s.push_str("<h2>Conferência do layout</h2><ul>");
            for w in &e.warnings {
                let _ = write!(s, "<li>{}</li>", esc(w));
            }
            s.push_str("</ul>");
        }
    }
    let _ = write!(s, "<p class=\"aviso\">{AVISO_LEGAL}</p></body></html>");
    s
}

fn heat_svg(e: &Exposure) -> String {
    let h = &e.heat;
    let max = h.presence.iter().copied().fold(0.0, f64::max).max(1e-12);
    let px = (600.0 / h.nx.max(h.ny) as f64).clamp(2.0, 30.0);
    let (w, hh) = (px * h.nx as f64, px * h.ny as f64);
    let mut s =
        format!("<svg viewBox=\"0 0 {w} {hh}\" width=\"{w}\" height=\"{hh}\" role=\"img\">");
    for iy in 0..h.ny {
        for ix in 0..h.nx {
            let k = iy * h.nx + ix;
            let v = h.presence[k] / max;
            if v <= 0.0 && h.near_misses[k] == 0 {
                continue;
            }
            let y = hh - (iy + 1) as f64 * px;
            let _ = write!(
                s,
                "<rect x=\"{:.1}\" y=\"{y:.1}\" width=\"{px:.1}\" height=\"{px:.1}\" fill=\"rgb(230,{},{})\"{}/>",
                ix as f64 * px,
                (220.0 * (1.0 - v)) as u8,
                (120.0 * (1.0 - v)) as u8,
                if h.near_misses[k] > 0 {
                    " stroke=\"#000\""
                } else {
                    ""
                }
            );
        }
    }
    s.push_str("</svg><p>Mais escuro: mais tempo de pessoas. Contorno preto: célula com quase-conflito.</p>");
    s
}
