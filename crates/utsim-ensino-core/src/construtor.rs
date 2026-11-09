//! The case builder (`docs/plano_ensino_e_jogo.md`, 6.2): from a UTSim model, suggest the
//! levers (capacities, buffers, team sizes) and the measures (lead time and throughput of
//! each exit), write a draft mission and its starting text, put it in a package, compute
//! the reference by the exhaustive search and set the goals from it. The teacher then
//! rewrites the text and adjusts ranges and costs.

use std::collections::BTreeMap;

use utsim_model::{Model, ObjectKind};

use crate::conferir::{Conferencia, conferir};
use crate::missao::{
    Alavanca, Animacao, Decisoes, Metrica, Missao, Objetivo, OpcaoAlavanca, Rodadas, Termo,
    Tolerancia,
};
use crate::nota::{Intervalo, fmt_num};
use crate::pacote::{Arquivos, carregar};

/// The levers a model offers: the capacity of each processor (up to two more servers),
/// the size of each finite queue (half to double) and the size of each team.
pub fn sugerir_alavancas(m: &Model) -> Vec<Alavanca> {
    let mut out = Vec::new();
    for o in &m.objects {
        match &o.kind {
            ObjectKind::Processor { capacity, .. } => out.push(Alavanca::Quantidade {
                id: format!("{}_capacidade", o.name),
                rotulo: format!("Capacidade de {}", o.name),
                chave: format!("{}.capacity", o.name),
                de: *capacity as i64,
                ate: *capacity as i64 + 2,
                inicial: None,
                custo_unitario: 10000.0,
                modelo: None,
            }),
            ObjectKind::Queue {
                capacity: Some(c), ..
            } => {
                let c = *c as i64;
                out.push(Alavanca::Quantidade {
                    id: format!("{}_lugares", o.name),
                    rotulo: format!("Lugares em {}", o.name),
                    chave: format!("{}.capacity", o.name),
                    de: (c / 2).max(1),
                    ate: (c * 2).max(2),
                    inicial: Some(c),
                    custo_unitario: 500.0,
                    modelo: None,
                });
            }
            ObjectKind::Dispatcher { team, .. } if team.len() > 1 => {
                let opcoes = (1..=team.len())
                    .map(|n| OpcaoAlavanca {
                        rotulo: n.to_string(),
                        ajustes: BTreeMap::from([(
                            format!("{}.team", o.name),
                            format!(
                                "[{}]",
                                team[..n]
                                    .iter()
                                    .map(|t| format!("\"{t}\""))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        )]),
                        custo: (n as f64 - team.len() as f64).max(0.0) * 5000.0,
                        numero: Some(n as f64),
                    })
                    .collect();
                out.push(Alavanca::Escolha {
                    id: format!("{}_equipe", o.name),
                    rotulo: format!("Membros da equipe {}", o.name),
                    opcoes,
                    inicial: team.len() - 1,
                });
            }
            _ => {}
        }
    }
    out
}

/// The measures a model offers: the mean lead time and the items out of each exit.
pub fn sugerir_metricas(m: &Model) -> Vec<Metrica> {
    let mut out = Vec::new();
    for o in &m.objects {
        if matches!(o.kind, ObjectKind::Sink) {
            out.push(Metrica {
                nome: format!("{}_lead_time", o.name),
                rotulo: format!("Lead time até {}", o.name),
                unidade: String::new(),
                valor: vec![Termo {
                    chave: format!("{}.time_in_system.mean", o.name),
                    fator: 1.0,
                }],
                divisor: Vec::new(),
                objetivo: Objetivo::Menor,
                meta: None,
                peso: 1.0,
                tolerancia: Tolerancia::Relativa(0.02),
                casas: 1,
            });
            out.push(Metrica {
                nome: format!("{}_saidas", o.name),
                rotulo: format!("Itens que chegam a {}", o.name),
                unidade: String::new(),
                valor: vec![Termo {
                    chave: format!("{}.input", o.name),
                    fator: 1.0,
                }],
                divisor: Vec::new(),
                objetivo: Objetivo::Maior,
                meta: None,
                peso: 0.0,
                tolerancia: Tolerancia::Relativa(0.02),
                casas: 0,
            });
        }
    }
    out
}

/// A first statement, written from the model, for the teacher to rewrite.
pub fn texto_inicial(titulo: &str, m: &Model, alavancas: &[Alavanca]) -> String {
    let mut s = format!("# {titulo}\n\n");
    s.push_str(&format!(
        "Esta missão usa o modelo \"{}\". Descreva aqui a empresa, o problema e os dados que o aluno precisa.\n\n",
        m.name
    ));
    s.push_str("## O que fazer\n\n");
    let mut n = 1;
    for a in alavancas {
        s.push_str(&format!("{n}. Escolha: {}.\n", a.rotulo()));
        n += 1;
    }
    s.push_str(&format!(
        "{n}. Simule e confira se as metas foram atingidas.\n"
    ));
    s
}

/// A draft mission over `modelo` (path inside the package).
pub fn rascunho(
    id: &str,
    titulo: &str,
    m: &Model,
    alavancas: Vec<Alavanca>,
    metricas: Vec<Metrica>,
) -> Missao {
    Missao {
        id: id.to_string(),
        formato: crate::missao::FORMATO_MISSAO,
        titulo: titulo.to_string(),
        conceitos: Vec::new(),
        modelo: "modelo.ron".into(),
        texto: "texto.md".into(),
        variante: Vec::new(),
        medicoes: Vec::new(),
        previsao: Vec::new(),
        alavancas,
        orcamento: None,
        restricoes: Vec::new(),
        metricas,
        justificativa: Vec::new(),
        rodadas: Rodadas {
            replicacoes: m.run.replications.clamp(5, 10),
            ..Rodadas::default()
        },
        dicas: Vec::new(),
        referencia: Decisoes::new(),
        animacao: Animacao::default(),
        balanceamento: None,
        arranjo: None,
        modelagem: None,
        dados: Vec::new(),
        analisar_em: Vec::new(),
    }
}

fn ron_missao(m: &Missao) -> String {
    let cfg = ron::ser::PrettyConfig::new().struct_names(true);
    ron::ser::to_string_pretty(m, cfg).expect("mission serializes")
}

/// A package of one act and one mission, ready to load.
pub fn pacote(titulo: &str, missao: &Missao, modelo_ron: &str, texto: &str) -> Arquivos {
    let id_missao = missao.id.replace('/', "_");
    let pacote = format!(
        "Pacote(\n    id: \"{id}\",\n    formato: 1,\n    titulo: \"{titulo}\",\n    versao: \"0.1.0\",\n    autor: \"Professor\",\n    atos: [\n        (numero: 1, titulo: \"{titulo}\", missoes: [\"{id_missao}/missao.ron\"]),\n    ],\n)\n",
        id = id_missao,
    );
    let mut a = Arquivos::new();
    a.insert("pacote.ron".into(), pacote.into_bytes());
    a.insert(
        format!("{id_missao}/missao.ron"),
        ron_missao(missao).into_bytes(),
    );
    a.insert(
        format!("{id_missao}/modelo.ron"),
        modelo_ron.as_bytes().to_vec(),
    );
    a.insert(format!("{id_missao}/texto.md"), texto.as_bytes().to_vec());
    a
}

/// Computes the reference of the (only) mission of `arquivos` and sets each measure with
/// weight a goal 5 % around the reference value; checks the mission again. Returns the
/// updated package and the final check.
pub fn calibrar(arquivos: &Arquivos) -> Result<(Arquivos, Conferencia), Vec<String>> {
    let p = carregar(arquivos)?;
    let m = p
        .missoes
        .first()
        .ok_or_else(|| vec!["o pacote não tem missões".to_string()])?;
    let c = conferir(m).map_err(|e| vec![e.to_string()])?;
    let mut missao = m.missao.clone();
    missao.referencia = c.melhor.clone();
    // The goals: 5 % of slack around the reference, for the measures that count.
    let sim = crate::motor::rodar(m, "", &[missao.completar(&c.melhor)], &|_| true)
        .map_err(|e| vec![e.to_string()])?;
    for mt in missao.metricas.iter_mut().filter(|mt| mt.peso > 0.0) {
        let v = Intervalo::de(
            &crate::motor::valor_metrica(&m.missao, mt, &sim[0])
                .map_err(|e| vec![e.to_string()])?,
        )
        .media;
        let meta = match mt.objetivo {
            Objetivo::Menor => v * 1.05,
            Objetivo::Maior => v * 0.95,
        };
        let casas = mt.casas as i32;
        let f = 10f64.powi(casas);
        mt.meta = Some((meta * f).round() / f);
    }
    let dir = arquivos
        .keys()
        .find(|k| k.ends_with("/missao.ron"))
        .cloned()
        .ok_or_else(|| vec!["missão não encontrada".to_string()])?;
    let mut novo = arquivos.clone();
    novo.insert(dir, ron_missao(&missao).into_bytes());
    let p2 = carregar(&novo)?;
    let c2 = conferir(&p2.missoes[0]).map_err(|e| vec![e.to_string()])?;
    Ok((novo, c2))
}

/// The check in words, for the screen and the command line.
pub fn resumo(c: &Conferencia) -> String {
    let mut s = format!(
        "{} combinações, {} dentro das restrições, {} batem as metas",
        c.combinacoes, c.viaveis, c.batem_meta
    );
    if c.viaveis > 0 {
        s.push_str(&format!(
            " ({} %)",
            fmt_num(100.0 * c.batem_meta as f64 / c.viaveis as f64, 0)
        ));
    }
    for e in &c.erros {
        s.push_str(&format!("\nproblema: {e}"));
    }
    for a in &c.avisos {
        s.push_str(&format!("\naviso: {a}"));
    }
    if c.erros.is_empty() {
        s.push_str("\na missão passa no conferidor");
    }
    s
}
