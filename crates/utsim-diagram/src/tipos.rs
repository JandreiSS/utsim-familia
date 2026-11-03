//! The kinds of diagram and the single entry point that makes any of them.

use std::fmt::Write as _;

use utsim_model::Model;

use crate::carta;
use crate::desenho::{Parte, pagina};
use crate::graph::{FlowGraph, Options};
use crate::text::{esc, fold};
use crate::{
    depara, flowchart, homem_maquina, html, mapa, mermaid, operacoes, raias, rastro, sipoc, svg,
    vsm,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoDiagrama {
    /// Process chart: vertical sequence of steps with the five symbols.
    Carta,
    /// Block flowchart by layers.
    Blocos,
    /// Operation process chart: operations and inspections only.
    Operacoes,
    /// Cross-functional chart with swimlanes.
    Raias,
    /// Suppliers, inputs, process, outputs, customers.
    Sipoc,
    /// Plan view with the symbols at the place of each step and the numbered route.
    Mapofluxograma,
    /// Every real movement drawn over the plan.
    Espaguete,
    /// From-to matrix and chord diagram.
    DePara,
    /// Multiple-activity chart of operators and machines.
    HomemMaquina,
    /// Current-state value stream map.
    Vsm,
}

impl TipoDiagrama {
    pub const ALL: [TipoDiagrama; 10] = [
        TipoDiagrama::Carta,
        TipoDiagrama::Blocos,
        TipoDiagrama::Operacoes,
        TipoDiagrama::Raias,
        TipoDiagrama::Sipoc,
        TipoDiagrama::Mapofluxograma,
        TipoDiagrama::Espaguete,
        TipoDiagrama::DePara,
        TipoDiagrama::HomemMaquina,
        TipoDiagrama::Vsm,
    ];

    /// Identifier of the command line and of the file names.
    pub fn id(self) -> &'static str {
        match self {
            TipoDiagrama::Carta => "carta",
            TipoDiagrama::Blocos => "blocos",
            TipoDiagrama::Operacoes => "operacoes",
            TipoDiagrama::Raias => "raias",
            TipoDiagrama::Sipoc => "sipoc",
            TipoDiagrama::Mapofluxograma => "mapofluxograma",
            TipoDiagrama::Espaguete => "espaguete",
            TipoDiagrama::DePara => "de-para",
            TipoDiagrama::HomemMaquina => "homem-maquina",
            TipoDiagrama::Vsm => "vsm",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            TipoDiagrama::Carta => "Carta de processo (fluxograma vertical)",
            TipoDiagrama::Blocos => "Fluxograma de blocos",
            TipoDiagrama::Operacoes => "Gráfico de operações do processo",
            TipoDiagrama::Raias => "Fluxograma multifuncional (raias)",
            TipoDiagrama::Sipoc => "Diagrama SIPOC",
            TipoDiagrama::Mapofluxograma => "Mapofluxograma",
            TipoDiagrama::Espaguete => "Diagrama de espaguete",
            TipoDiagrama::DePara => "Matriz de-para e diagrama de cordas",
            TipoDiagrama::HomemMaquina => "Gráfico homem-máquina",
            TipoDiagrama::Vsm => "Mapa do fluxo de valor (estado atual)",
        }
    }

    pub fn descricao(self) -> &'static str {
        match self {
            TipoDiagrama::Carta => {
                "Tabela da sequência de etapas do caminho principal, com o símbolo de cada uma (operação, transporte, inspeção, espera, armazenagem), tempo, distância e totais por símbolo."
            }
            TipoDiagrama::Blocos => {
                "Caixas e setas por camadas: ramificações com porcentagens, junções, retrabalho, recursos e tempos médios."
            }
            TipoDiagrama::Operacoes => {
                "Só as operações (O) e inspeções (I), numeradas, com as linhas de material entrando nas montagens e saindo das desmontagens."
            }
            TipoDiagrama::Raias => {
                "As etapas dentro da raia de quem as executa (equipe, frota, estação, esteira, fila), com as passagens entre raias em destaque."
            }
            TipoDiagrama::Sipoc => {
                "Visão de uma página: fornecedores, entradas, etapas principais, saídas e clientes."
            }
            TipoDiagrama::Mapofluxograma => {
                "A planta com os símbolos da carta de processo no lugar de cada etapa e o percurso do item, numerado."
            }
            TipoDiagrama::Espaguete => {
                "Todos os deslocamentos reais de itens, pessoas e veículos sobre a planta, com totais de distância."
            }
            TipoDiagrama::DePara => {
                "Viagens e distância x viagens entre cada par de objetos, em tabela e em diagrama de cordas."
            }
            TipoDiagrama::HomemMaquina => {
                "Colunas de tempo de cada operador e máquina (trabalhando, preparando, andando, esperando, ocioso, parado) e a ocupação."
            }
            TipoDiagrama::Vsm => {
                "Processos com tempo de ciclo, setup, disponibilidade e operadores; estoques entre eles e linha do tempo com o índice de agregação de valor."
            }
        }
    }

    /// Whether the diagram needs a simulation run (the others come from the model alone).
    pub fn dinamico(self) -> bool {
        matches!(
            self,
            TipoDiagrama::Mapofluxograma
                | TipoDiagrama::Espaguete
                | TipoDiagrama::DePara
                | TipoDiagrama::HomemMaquina
                | TipoDiagrama::Vsm
        )
    }

    pub fn from_id(text: &str) -> Option<TipoDiagrama> {
        let t = fold(text.trim()).replace('_', "-");
        TipoDiagrama::ALL
            .into_iter()
            .find(|x| x.id() == t)
            .or(match t.as_str() {
                "processo" | "vertical" => Some(TipoDiagrama::Carta),
                "horizontal" | "fluxograma" => Some(TipoDiagrama::Blocos),
                "opc" | "operacoes-do-processo" => Some(TipoDiagrama::Operacoes),
                "multifuncional" | "swimlane" => Some(TipoDiagrama::Raias),
                "depara" | "cordas" | "from-to" => Some(TipoDiagrama::DePara),
                "homemmaquina" | "atividades-multiplas" => Some(TipoDiagrama::HomemMaquina),
                "fluxo-de-valor" | "mapa-do-fluxo-de-valor" => Some(TipoDiagrama::Vsm),
                _ => None,
            })
    }
}

/// A diagram, in every format it has.
pub struct Saida {
    pub svg: Option<String>,
    /// Self-contained page.
    pub html: String,
    pub csv: Option<String>,
    pub mermaid: Option<String>,
}

/// The `<svg>` element of a standalone document, to put inside a page.
fn inline(svg: &str) -> &str {
    svg.find("<svg").map_or(svg, |i| &svg[i..])
}

fn do_modelo(model: &Model) -> String {
    format!(
        "o modelo \"{}\" (arquivo .ron); nenhuma simulação foi rodada.",
        model.name
    )
}

fn corpo(parte: &Parte) -> String {
    let mut b = String::new();
    if let Some(s) = &parte.svg {
        let _ = write!(b, "<div class=\"rolagem\">{}</div>", inline(s));
    }
    b.push_str(&parte.tabelas);
    b
}

fn pagina_de(tipo: TipoDiagrama, model: &Model, parte: &Parte, origem: &str) -> String {
    pagina(
        &format!("{}: {}", tipo.nome(), model.name),
        tipo.descricao(),
        &corpo(parte),
        origem,
    )
}

/// Makes the diagram `tipo` of `model`.
pub fn gerar(tipo: TipoDiagrama, model: &Model, o: &Options) -> Result<Saida, String> {
    let g = flowchart(model, o);
    let estatico = |parte: Parte, mer: Option<String>| Saida {
        html: pagina_de(tipo, model, &parte, &do_modelo(model)),
        svg: parte.svg,
        csv: parte.csv,
        mermaid: mer,
    };
    match tipo {
        TipoDiagrama::Carta => {
            let c = carta::build(&g);
            Ok(estatico(
                Parte {
                    svg: None,
                    tabelas: format!("{}{}", html::legend(), carta::html_table(&c)),
                    csv: Some(carta::csv(&c)),
                },
                None,
            ))
        }
        TipoDiagrama::Blocos => Ok(estatico(
            Parte {
                svg: Some(svg::svg(&g)),
                tabelas: format!("<h2>Legenda</h2>{}", html::legend()),
                csv: None,
            },
            Some(mermaid::mermaid(&g)),
        )),
        TipoDiagrama::Operacoes => {
            let r = operacoes::grafo(&g);
            Ok(estatico(
                Parte {
                    svg: Some(svg::svg(&r)),
                    tabelas: format!(
                        "<p class=\"muted\">O = operação, I = inspeção, numeradas na ordem do processo. As esperas, os transportes e as armazenagens ficam fora do gráfico (estão na carta de processo).</p>{}",
                        html::legend()
                    ),
                    csv: None,
                },
                Some(mermaid::mermaid(&r)),
            ))
        }
        TipoDiagrama::Raias => {
            let r = raias::montar(&g, &o.areas);
            let mut t = String::from("<h2>Passagens entre raias</h2>");
            let pas = r.passagens();
            if pas.is_empty() {
                t.push_str("<p>Não há passagens entre raias.</p>");
            } else {
                t.push_str("<table><thead><tr><th>De</th><th>Para</th><th class=\"n\">Passagens</th></tr></thead><tbody>");
                for (a, b, c) in &pas {
                    let _ = write!(
                        t,
                        "<tr><td>{}</td><td>{}</td><td class=\"n\">{c}</td></tr>",
                        esc(a),
                        esc(b)
                    );
                }
                t.push_str("</tbody></table>");
            }
            t.push_str("<p class=\"muted\">Setas laranja atravessam raias. A raia vem da equipe de operadores, do tipo de objeto ou da área escolhida com <code>--raia objeto=nome</code>; o transporte por frota tem raia própria.</p>");
            let m = raias::mermaid(&r);
            Ok(estatico(
                Parte {
                    svg: Some(raias::svg(&r, "Fluxograma multifuncional", true)),
                    tabelas: t,
                    csv: None,
                },
                Some(m),
            ))
        }
        TipoDiagrama::Sipoc => {
            let s = sipoc::montar(&g);
            Ok(estatico(
                Parte {
                    svg: Some(s.svg("Diagrama SIPOC", true)),
                    tabelas: s.html_table(),
                    csv: Some(s.csv()),
                },
                None,
            ))
        }
        _ => {
            let r = rastro::capturar(model, o.replication)?;
            let origem = r.origem(model);
            let parte = match tipo {
                TipoDiagrama::Mapofluxograma => {
                    mapa::mapofluxograma(model, &g, &r, o.plant_background.as_deref())
                }
                TipoDiagrama::Espaguete => match &o.compare {
                    Some(depois) => {
                        let g2 = flowchart(depois, o);
                        let r2 = rastro::capturar(depois, o.replication)?;
                        mapa::comparar((model, &g, &r), (depois, &g2, &r2))
                    }
                    None => mapa::espaguete(model, &g, &r),
                },
                TipoDiagrama::DePara => depara::gerar(&r),
                TipoDiagrama::HomemMaquina => homem_maquina::gerar(model, &r, o.window)?,
                _ => vsm::gerar(model, &g, &r),
            };
            Ok(Saida {
                html: pagina_de(tipo, model, &parte, &origem),
                svg: parte.svg,
                csv: parte.csv,
                mermaid: None,
            })
        }
    }
}

/// Index page that links the files of every diagram. `itens`: the type, the file name
/// (or the error that kept it from being made).
pub fn indice(model: &Model, itens: &[(TipoDiagrama, Result<String, String>)]) -> String {
    let mut b = String::from("<ul>");
    for (t, r) in itens {
        match r {
            Ok(f) => {
                let _ = write!(
                    b,
                    "<li><a href=\"{}\"><b>{}</b></a> ({}): {}</li>",
                    esc(f),
                    esc(t.nome()),
                    if t.dinamico() {
                        "da simulação"
                    } else {
                        "do modelo"
                    },
                    esc(t.descricao())
                );
            }
            Err(e) => {
                let _ = write!(
                    b,
                    "<li><b>{}</b>: não gerado ({})</li>",
                    esc(t.nome()),
                    esc(e)
                );
            }
        }
    }
    b.push_str("</ul>");
    pagina(
        &format!("Diagramas do processo: {}", model.name),
        "Todos os tipos de fluxograma gerados a partir do mesmo modelo.",
        &b,
        &format!(
            "o modelo \"{}\" e, nos diagramas da simulação, a rodada de semente {} (replicação escolhida na geração).",
            model.name, model.run.seed
        ),
    )
}

/// Graph used by the static types, for callers that want it too.
pub fn grafo(model: &Model, o: &Options) -> FlowGraph {
    flowchart(model, o)
}
