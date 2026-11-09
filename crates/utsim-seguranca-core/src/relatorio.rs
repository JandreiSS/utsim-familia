//! Report of the inventory and action plan, per station or for the whole unit. The report
//! is a list of neutral blocks ([`Bloco`]) so the same content goes to HTML and to PDF.
//! The legal notice opens and closes every report.

use std::fmt::Write;

use crate::AVISO_LEGAL;
use crate::inventario::{ETAPAS, Inventario};

/// Notice that the report is not the legal document of the risk management programme.
pub const AVISO_PGR: &str = "Este relatório organiza o inventário de riscos e o plano de ação \
registrados no UTSim. Ele não é o documento legal do programa de gerenciamento de riscos \
(NR-1) e não substitui a avaliação do profissional responsável.";

#[derive(Clone, Debug, PartialEq)]
pub enum Bloco {
    Titulo(String),
    Subtitulo(String),
    Paragrafo(String),
    Aviso(String),
    Tabela {
        cabecalho: Vec<String>,
        linhas: Vec<Vec<String>>,
    },
}

/// Report content for the stations in `postos` (empty: every station of the unit).
pub fn blocos(inv: &Inventario, unidade: &str, postos: &[String], data: &str) -> Vec<Bloco> {
    let mut b = vec![
        Bloco::Titulo(format!("Inventário de riscos e plano de ação: {unidade}")),
        Bloco::Paragrafo(format!("Emitido em {data}. Matriz: {}.", inv.matriz.nome)),
        Bloco::Aviso(AVISO_LEGAL.to_string()),
        Bloco::Aviso(AVISO_PGR.to_string()),
    ];
    let m = &inv.matriz;
    let mut cab = vec!["Probabilidade \\ severidade".to_string()];
    cab.extend(m.severidade.iter().map(|d| d.nome.clone()));
    let linhas = m
        .probabilidade
        .iter()
        .enumerate()
        .rev()
        .map(|(p, d)| {
            let mut l = vec![d.nome.clone()];
            l.extend((0..m.severidade.len()).map(|s| m.nivel_de(p, s).nome.clone()));
            l
        })
        .collect();
    b.push(Bloco::Subtitulo("Matriz de risco".into()));
    b.push(Bloco::Tabela {
        cabecalho: cab,
        linhas,
    });
    b.push(Bloco::Tabela {
        cabecalho: vec!["Nível".into(), "Decisão".into()],
        linhas: m
            .niveis
            .iter()
            .map(|n| vec![n.nome.clone(), n.decisao.clone()])
            .collect(),
    });
    for posto in inv
        .postos
        .iter()
        .filter(|p| postos.is_empty() || postos.contains(&p.id))
    {
        b.push(Bloco::Subtitulo(format!("Posto: {}", posto.nome)));
        b.push(Bloco::Paragrafo(format!(
            "{}: área {}, {} pessoa(s) exposta(s) por turno. Atividades: {}. EPI exigido: {}.{}",
            ETAPAS[0],
            if posto.area.is_empty() {
                "não informada"
            } else {
                &posto.area
            },
            posto.pessoas_expostas,
            if posto.atividades.is_empty() {
                "não informadas".to_string()
            } else {
                posto.atividades.join(", ")
            },
            if posto.epi.is_empty() {
                "nenhum".to_string()
            } else {
                posto.epi.join(", ")
            },
            posto
                .objeto
                .as_ref()
                .map_or(String::new(), |o| format!(" Objeto do modelo: {o}."))
        )));
        let perigos = inv.perigos_do_posto(&posto.id);
        if perigos.is_empty() {
            b.push(Bloco::Paragrafo("Nenhum perigo levantado.".into()));
            continue;
        }
        b.push(Bloco::Tabela {
            cabecalho: [
                "Grupo",
                "Perigo",
                "Fonte e circunstância",
                "Possíveis lesões",
                "Controles existentes",
                "P",
                "S",
                "Nível",
            ]
            .map(String::from)
            .to_vec(),
            linhas: perigos
                .iter()
                .map(|p| {
                    vec![
                        p.grupo.nome().into(),
                        p.descricao.clone(),
                        format!("{} {}", p.fonte, p.circunstancia)
                            .trim()
                            .to_string(),
                        p.lesoes.clone(),
                        p.controles.clone(),
                        m.probabilidade
                            .get(p.probabilidade)
                            .map_or("?".into(), |d| d.nome.clone()),
                        m.severidade
                            .get(p.severidade)
                            .map_or("?".into(), |d| d.nome.clone()),
                        inv.matriz.niveis[inv.nivel_atual(p)].nome.clone(),
                    ]
                })
                .collect(),
        });
        let acoes: Vec<Vec<String>> = perigos
            .iter()
            .flat_map(|p| {
                inv.acoes_do_perigo(&p.id).into_iter().map(|a| {
                    vec![
                        p.descricao.clone(),
                        a.hierarquia.nome().into(),
                        a.descricao.clone(),
                        a.responsavel.clone(),
                        a.prazo.clone(),
                        a.situacao.nome().into(),
                        match a.eficaz {
                            Some(true) => "sim".into(),
                            Some(false) => "não".into(),
                            None => "".into(),
                        },
                    ]
                })
            })
            .collect();
        if !acoes.is_empty() {
            b.push(Bloco::Paragrafo(format!("{} e {}", ETAPAS[3], ETAPAS[4])));
            b.push(Bloco::Tabela {
                cabecalho: [
                    "Perigo",
                    "Hierarquia",
                    "Ação",
                    "Responsável",
                    "Prazo",
                    "Situação",
                    "Eficaz",
                ]
                .map(String::from)
                .to_vec(),
                linhas: acoes,
            });
        }
    }
    b.push(Bloco::Aviso(AVISO_LEGAL.to_string()));
    b
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn html(blocos: &[Bloco]) -> String {
    let mut s = String::from(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>Inventário de riscos</title>\
         <style>body{font-family:system-ui,sans-serif;margin:24px;max-width:1100px;color:#222}\
         table{border-collapse:collapse;margin:8px 0 20px}td,th{border:1px solid #bbb;padding:4px 8px;vertical-align:top}\
         .aviso{border:2px solid #c0392b;padding:8px 12px;background:#fdf2f0}\
         @media print{.aviso{break-inside:avoid}}</style></head><body>",
    );
    for b in blocos {
        match b {
            Bloco::Titulo(t) => {
                let _ = write!(s, "<h1>{}</h1>", esc(t));
            }
            Bloco::Subtitulo(t) => {
                let _ = write!(s, "<h2>{}</h2>", esc(t));
            }
            Bloco::Paragrafo(t) => {
                let _ = write!(s, "<p>{}</p>", esc(t));
            }
            Bloco::Aviso(t) => {
                let _ = write!(s, "<p class=\"aviso\">{}</p>", esc(t));
            }
            Bloco::Tabela { cabecalho, linhas } => {
                s.push_str("<table><tr>");
                for c in cabecalho {
                    let _ = write!(s, "<th>{}</th>", esc(c));
                }
                s.push_str("</tr>");
                for l in linhas {
                    s.push_str("<tr>");
                    for c in l {
                        let _ = write!(s, "<td>{}</td>", esc(c));
                    }
                    s.push_str("</tr>");
                }
                s.push_str("</table>");
            }
        }
    }
    s.push_str("</body></html>");
    s
}

/// The report as PDF (A3 landscape, for the wide tables), with the legal notice also in the
/// footer of every page. `emitido` goes in the footer (who and when printed it).
pub fn pdf(blocos: &[Bloco], emitido: &str) -> Vec<u8> {
    use utsim_ferramentas::relatorios::{Document, PageSize, Template, pdf};
    let titulo = blocos
        .iter()
        .find_map(|b| match b {
            Bloco::Titulo(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Inventário de riscos".into());
    let mut doc = Document::new(&titulo, "UTSim Segurança do Trabalho");
    doc.size = PageSize::A3Landscape;
    doc.app = "UTSim Segurança do Trabalho".into();
    for b in blocos {
        match b {
            Bloco::Titulo(_) => {}
            Bloco::Subtitulo(t) => {
                doc.heading(t);
            }
            Bloco::Paragrafo(t) => {
                doc.para(t);
            }
            Bloco::Aviso(t) => {
                doc.para(&format!("AVISO: {t}"));
            }
            Bloco::Tabela { cabecalho, linhas } => {
                let cab: Vec<&str> = cabecalho.iter().map(String::as_str).collect();
                doc.table("", &cab, linhas.clone(), &[]);
            }
        }
    }
    let modelo = Template {
        footer: AVISO_LEGAL.to_string(),
        ..Template::default()
    };
    pdf::render(&doc, &modelo, emitido)
}
