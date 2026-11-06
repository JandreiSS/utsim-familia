//! Diagrams of the Lean Office line: the office value stream map (steps of the main path
//! with P/T, L/T, %C&A and people; inboxes with the cases waiting; lead time ladder with
//! the touch time) and the swimlane chart by department (`department` of the objects), drawn
//! with `utsim_diagram::raias`.

use std::fmt::Write as _;

use utsim_core::stats::Summary;
use utsim_diagram::carta::main_path;
use utsim_diagram::graph::Options;
use utsim_model::{Model, ObjectKind};

use super::{Indicadores, mhw, num, num1, pct_hw, role};
use crate::pacote::{Pack, Role};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One box of the office map.
#[derive(Clone, Debug)]
pub enum Caixa {
    /// A step that works on the case.
    Processo {
        nome: String,
        papel: Role,
        /// Work per visit (P/T) and its touch part.
        pt: Option<Summary>,
        touch: Option<Summary>,
        /// Mean time a case stays in the step per visit (L/T of the step).
        lt: Summary,
        ca: Option<Summary>,
        pessoas: String,
        horario: String,
        department: Option<String>,
    },
    /// Cases waiting (inbox, batch, handover, outside information, meeting agenda).
    Espera {
        nome: String,
        papel: Role,
        parados: Summary,
        espera: Summary,
    },
}

/// The office value stream map of a model and its indicators.
#[derive(Clone, Debug)]
pub struct VsmAdministrativo {
    pub caixas: Vec<Caixa>,
    pub lead_time: Summary,
    pub touch: Summary,
    pub processo: Summary,
    pub razao: Summary,
    pub rolled_ca: Option<Summary>,
}

/// Team (and number) of people a step uses, and whether they have a schedule.
fn pessoas(model: &Model, kind: &ObjectKind) -> (String, String) {
    let ObjectKind::Processor {
        operators: Some(op),
        ..
    } = kind
    else {
        return ("sem pessoa".to_string(), "contínuo".to_string());
    };
    let members: Vec<&str> = match model.objects.iter().find(|o| o.name == op.team) {
        Some(utsim_model::ObjectDef {
            kind: ObjectKind::Dispatcher { team, .. },
            ..
        }) => team.iter().map(String::as_str).collect(),
        _ => vec![op.team.as_str()],
    };
    let horario = model
        .objects
        .iter()
        .find(|o| members.contains(&o.name.as_str()))
        .and_then(|o| o.shifts.as_ref())
        .map_or("sem horário".to_string(), |s| match s {
            utsim_model::ShiftSpec::Calendar { name, .. } => name.clone(),
            utsim_model::ShiftSpec::Inline(_) => "turnos próprios".to_string(),
        });
    (
        format!("{} de {} ({} pessoa(s))", op.count, op.team, members.len()),
        horario,
    )
}

/// Builds the map along the main path of the flowchart.
pub fn montar(model: &Model, pack: Option<&Pack>, k: &Indicadores) -> VsmAdministrativo {
    let g = utsim_diagram::flowchart(model, &Options::default());
    let mut caixas = Vec::new();
    for n in main_path(&g) {
        let name = &g.nodes[n].name;
        let Some(i) = model.objects.iter().position(|o| o.name == *name) else {
            continue;
        };
        let obj = &model.objects[i];
        let Some(papel) = role(model, pack, i) else {
            continue;
        };
        let stay = Summary::from_samples(
            &k.reports
                .iter()
                .map(|r| r.blocks[i].staytime.mean)
                .collect::<Vec<_>>(),
        );
        let processor = matches!(obj.kind, ObjectKind::Processor { .. });
        match papel {
            Role::Entry | Role::Done | Role::Person | Role::Team => {}
            Role::Work | Role::Approval | Role::Check | Role::Meeting if processor => {
                let (pessoas, horario) = pessoas(model, &obj.kind);
                let horario = match &obj.shifts {
                    Some(utsim_model::ShiftSpec::Calendar { name, .. }) => name.clone(),
                    Some(utsim_model::ShiftSpec::Inline(_)) => "horário próprio".to_string(),
                    None => horario,
                };
                caixas.push(Caixa::Processo {
                    nome: name.clone(),
                    papel,
                    pt: k.resumo_opt(|r| r.pt_etapa.get(name).map(|p| p.0)),
                    touch: k.resumo_opt(|r| r.pt_etapa.get(name).map(|p| p.1)),
                    lt: stay,
                    ca: k.resumo_opt(|r| r.ca.get(name).copied()),
                    pessoas,
                    horario,
                    department: obj.department.clone(),
                });
            }
            _ => {
                let parados = Summary::from_samples(
                    &k.reports
                        .iter()
                        .map(|r| r.blocks[i].content_mean)
                        .collect::<Vec<_>>(),
                );
                caixas.push(Caixa::Espera {
                    nome: name.clone(),
                    papel,
                    parados,
                    espera: stay,
                });
            }
        }
    }
    VsmAdministrativo {
        caixas,
        lead_time: k.resumo(|r| r.lead_time),
        touch: k.resumo(|r| r.touch),
        processo: k.resumo(|r| r.processo),
        razao: k.resumo(|r| r.razao_atividade),
        rolled_ca: k.resumo_opt(|r| r.rolled_ca),
    }
}

fn texto(x: f64, y: f64, class: &str, t: &str) -> String {
    format!(
        "<text x=\"{x:.1}\" y=\"{y:.1}\" class=\"{class}\">{}</text>",
        esc(t)
    )
}

fn curto(t: &str, max: usize) -> String {
    super::relatorio::curto(t, max)
}

impl VsmAdministrativo {
    /// The map as an SVG element (to put inside a page).
    pub fn svg(&self, titulo: &str) -> String {
        let (bw, gap, x0) = (176.0, 84.0, 20.0);
        let procs: Vec<&Caixa> = self
            .caixas
            .iter()
            .filter(|c| matches!(c, Caixa::Processo { .. }))
            .collect();
        let width = (x0 * 2.0 + procs.len() as f64 * (bw + gap)).max(760.0);
        let height = 420.0;
        let mut b = format!(
            "<svg viewBox=\"0 0 {width:.0} {height}\" role=\"img\" aria-label=\"{}\">\
             <style>.vsm-caixa{{fill:none;stroke:currentColor;stroke-width:1.2}}\
             .vsm-topo{{fill:#2563eb;opacity:.85}}.vsm-b{{font-weight:600}}\
             .vsm-tri{{fill:#f59e0b;opacity:.85}}.vsm-esp{{stroke:#c2410c;stroke-width:3}}\
             .vsm-trab{{stroke:#16a34a;stroke-width:3}}.vsm-seta{{stroke:currentColor;stroke-width:1.2;fill:none}}</style>",
            esc(titulo)
        );
        b.push_str(&texto(x0, 22.0, "vsm-b", titulo));
        // Boxes and inventories, in path order: waits go before the next process box.
        let mut x = x0 + gap;
        let mut pending: Vec<&Caixa> = Vec::new();
        let ladder_y = 300.0;
        let mut ladder: Vec<(f64, f64, f64)> = Vec::new(); // (x, wait, work)
        for c in &self.caixas {
            match c {
                Caixa::Espera { .. } => pending.push(c),
                Caixa::Processo {
                    nome,
                    pt,
                    touch,
                    lt,
                    ca,
                    pessoas,
                    horario,
                    department,
                    ..
                } => {
                    // Inventory triangle with the waits before this box.
                    let mut wait = 0.0;
                    if !pending.is_empty() {
                        let parados: f64 = pending
                            .iter()
                            .map(|p| match p {
                                Caixa::Espera { parados, .. } => parados.mean,
                                _ => 0.0,
                            })
                            .sum();
                        wait = pending
                            .iter()
                            .map(|p| match p {
                                Caixa::Espera { espera, .. } => espera.mean,
                                _ => 0.0,
                            })
                            .sum();
                        let tx = x - gap / 2.0 - 12.0;
                        let _ = write!(
                            b,
                            "<polygon class=\"vsm-tri\" points=\"{:.1},96 {:.1},96 {:.1},70\"/>",
                            tx,
                            tx + 24.0,
                            tx + 12.0
                        );
                        b.push_str(&texto(
                            tx - 14.0,
                            112.0,
                            "",
                            &format!("{} casos", num(parados)),
                        ));
                        b.push_str(&texto(
                            tx - 14.0,
                            126.0,
                            "",
                            &format!("{} h", num1(wait / 60.0)),
                        ));
                        let names: Vec<String> = pending
                            .iter()
                            .map(|p| match p {
                                Caixa::Espera { nome, .. } => nome.clone(),
                                _ => String::new(),
                            })
                            .collect();
                        b.push_str(&texto(tx - 14.0, 140.0, "", &curto(&names.join(", "), 18)));
                        pending.clear();
                    }
                    let _ = write!(
                        b,
                        "<rect class=\"vsm-caixa\" x=\"{x:.1}\" y=\"50\" width=\"{bw}\" height=\"150\"/>\
                         <rect class=\"vsm-topo\" x=\"{x:.1}\" y=\"50\" width=\"{bw}\" height=\"22\"/>"
                    );
                    b.push_str(&format!(
                        "<text x=\"{:.1}\" y=\"66\" class=\"vsm-b\" style=\"fill:#fff\">{}</text>",
                        x + 6.0,
                        esc(&curto(nome, 22))
                    ));
                    let ptm = pt.as_ref().map_or(0.0, |s| s.mean);
                    let lines = [
                        format!(
                            "P/T = {} min",
                            pt.as_ref().map_or("-".into(), |s| num(s.mean))
                        ),
                        format!(
                            "touch = {} min",
                            touch.as_ref().map_or("-".into(), |s| num(s.mean))
                        ),
                        format!("L/T = {} min", num(lt.mean)),
                        format!(
                            "%C&A = {}",
                            ca.as_ref()
                                .map_or("-".into(), |s| format!("{}%", num1(100.0 * s.mean)))
                        ),
                        curto(pessoas, 26),
                        format!("horário: {}", curto(horario, 16)),
                        department
                            .as_ref()
                            .map_or(String::new(), |a| format!("depto.: {}", curto(a, 20))),
                    ];
                    for (j, l) in lines.iter().enumerate() {
                        b.push_str(&texto(x + 6.0, 90.0 + 15.0 * j as f64, "", l));
                    }
                    let _ = write!(
                        b,
                        "<path class=\"vsm-seta\" d=\"M{:.1} 125 H{:.1}\"/>",
                        x - gap + 4.0,
                        x - 2.0
                    );
                    // Wait inside the step (L/T − P/T) goes with the wait before it.
                    ladder.push((x, wait + (lt.mean - ptm).max(0.0), ptm));
                    x += bw + gap;
                }
            }
        }
        // Lead time ladder: waits up, work down.
        b.push_str(&texto(x0, ladder_y - 40.0, "vsm-b", "Escada do lead time"));
        let mut lx = x0;
        for (bx, wait, work) in &ladder {
            let mid = bx - gap / 2.0;
            let _ = write!(
                b,
                "<line class=\"vsm-esp\" x1=\"{lx:.1}\" y1=\"{:.1}\" x2=\"{mid:.1}\" y2=\"{:.1}\"/>\
                 <line class=\"vsm-trab\" x1=\"{mid:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>",
                ladder_y - 16.0,
                ladder_y - 16.0,
                ladder_y + 16.0,
                bx + bw,
                ladder_y + 16.0
            );
            b.push_str(&texto(
                lx + 4.0,
                ladder_y - 22.0,
                "",
                &format!("{} h", num1(wait / 60.0)),
            ));
            b.push_str(&texto(
                mid + 4.0,
                ladder_y + 32.0,
                "",
                &format!("{} min", num(*work)),
            ));
            lx = bx + bw;
        }
        let rolled = self
            .rolled_ca
            .as_ref()
            .map_or("-".into(), |s| format!("{}%", num1(100.0 * s.mean)));
        b.push_str(&texto(
            x0,
            ladder_y + 72.0,
            "vsm-b",
            &format!(
                "Lead time: {} h   |   P/T: {} min   |   Touch time: {} min   |   Razão de atividade: {}%   |   %C&A acumulado: {rolled}",
                num1(self.lead_time.mean / 60.0),
                num(self.processo.mean),
                num(self.touch.mean),
                num1(100.0 * self.razao.mean),
            ),
        ));
        b.push_str(&texto(
            x0,
            ladder_y + 92.0,
            "",
            "Vermelho: espera antes e dentro da etapa (horas). Verde: trabalho na etapa (minutos). Os valores são do caminho principal; o lead time é o de todos os casos.",
        ));
        b.push_str("</svg>");
        b
    }

    /// The map as a table.
    pub fn tabela(&self) -> String {
        let mut t = String::from(
            "<table><thead><tr><th>Etapa</th><th>Papel</th><th class=\"n\">P/T (min)</th><th class=\"n\">Touch (min)</th><th class=\"n\">L/T ou espera (min)</th><th class=\"n\">Casos parados</th><th class=\"n\">%C&amp;A</th><th>Pessoas</th></tr></thead><tbody>",
        );
        for c in &self.caixas {
            match c {
                Caixa::Processo {
                    nome,
                    papel,
                    pt,
                    touch,
                    lt,
                    ca,
                    pessoas,
                    ..
                } => {
                    let _ = write!(
                        t,
                        "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">-</td><td class=\"n\">{}</td><td>{}</td></tr>",
                        esc(nome),
                        papel.nome(),
                        pt.as_ref().map_or("-".into(), |s| mhw(s, 1.0)),
                        touch.as_ref().map_or("-".into(), |s| mhw(s, 1.0)),
                        mhw(lt, 1.0),
                        ca.as_ref().map_or("-".into(), pct_hw),
                        esc(pessoas)
                    );
                }
                Caixa::Espera {
                    nome,
                    papel,
                    parados,
                    espera,
                } => {
                    let _ = write!(
                        t,
                        "<tr><td>{}</td><td>{}</td><td class=\"n\">-</td><td class=\"n\">-</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">-</td><td>-</td></tr>",
                        esc(nome),
                        papel.nome(),
                        mhw(espera, 1.0),
                        mhw(parados, 1.0)
                    );
                }
            }
        }
        t.push_str("</tbody></table>");
        t
    }
}

/// The swimlane chart by department: its SVG and the handovers between departments.
pub fn raias(model: &Model) -> (String, Vec<(String, String, usize)>) {
    let options = Options::default();
    let g = utsim_diagram::flowchart(model, &options);
    let r = utsim_diagram::raias::montar(&g, &utsim_diagram::areas(model, &options.areas));
    // The standalone document carries its styles; inside a page only the element goes.
    let doc = utsim_diagram::raias::svg(&r, "Fluxograma de raias por departamento", true);
    let element = doc
        .find("<svg")
        .map_or(doc.as_str(), |i| &doc[i..])
        .to_string();
    (element, r.passagens())
}

/// Sections of the line report with the office map (and, with `futuro`, the map of the
/// future-state scenario under it) and the swimlane chart.
pub fn secoes(
    model: &Model,
    pack: Option<&Pack>,
    k: &Indicadores,
    futuro: Option<(&Model, &Indicadores)>,
) -> String {
    let mut h = String::from("<h2>Mapa do fluxo de valor do processo administrativo</h2>");
    let atual = montar(model, pack, k);
    let _ = write!(
        h,
        "<div class=\"rolagem\">{}</div>{}",
        atual.svg(&format!("Estado atual: {}", model.name)),
        atual.tabela()
    );
    if let Some((m2, k2)) = futuro {
        let f = montar(m2, pack, k2);
        let _ = write!(
            h,
            "<h3>Estado futuro: {}</h3><div class=\"rolagem\">{}</div>{}",
            esc(&m2.name),
            f.svg(&format!("Estado futuro: {}", m2.name)),
            f.tabela()
        );
        let _ = write!(
            h,
            "<table><thead><tr><th>Indicador</th><th class=\"n\">Atual</th><th class=\"n\">Futuro</th></tr></thead><tbody>"
        );
        for ((nome, a, _), (_, b, _)) in k.tabela().into_iter().zip(k2.tabela()) {
            let _ = write!(
                h,
                "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                esc(&nome),
                esc(&a),
                esc(&b)
            );
        }
        h.push_str("</tbody></table>");
    }
    let (svg, passagens) = raias(model);
    let _ = write!(
        h,
        "<h2>Fluxograma de raias por departamento</h2><p class=\"muted\">As raias vêm do campo <code>department</code> dos objetos; setas laranja são passagens entre departamentos, tracejadas são voltas de retrabalho.</p><div class=\"rolagem\">{svg}</div>"
    );
    if !passagens.is_empty() {
        h.push_str("<table><thead><tr><th>De</th><th>Para</th><th class=\"n\">Passagens</th></tr></thead><tbody>");
        for (a, b, c) in &passagens {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td class=\"n\">{c}</td></tr>",
                esc(a),
                esc(b)
            );
        }
        h.push_str("</tbody></table>");
    }
    h
}
