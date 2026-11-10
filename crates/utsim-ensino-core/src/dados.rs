//! Data with planted effects (`docs/plano_ensino_e_jogo.md`, 6.3): seeded generators of
//! teaching data sets whose answer is known (the process time distribution, the variance
//! components of a gauge study, a shifted mean in one group, a series with a pattern, the
//! effects of a factorial). Each set keeps its recipe and the planted answer, which the
//! grading uses; the recovery tests use the same functions the student will use. The
//! records of a simulation run also come from here: the mission's own model, with the
//! process time of one station changed, run with the student's seed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_core::Dist;
use utsim_core::rng::{SimRng, stream};
use utsim_core::trace::TraceEvent;
use utsim_model::{Model, ObjectKind};

use crate::nota::fmt_num;

/// A pattern planted in a series (the special causes of a control chart).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Padrao {
    /// From point `inicio` on, the mean moves by `tamanho` standard deviations.
    Deslocamento { inicio: usize, tamanho: f64 },
    /// From point `inicio` on, the mean grows `inclinacao` standard deviations per point.
    Tendencia { inicio: usize, inclinacao: f64 },
}

/// What to generate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Gerador {
    /// `n` process times from a distribution (fitting exercises, time studies).
    Tempos { dist: Dist, n: usize },
    /// A crossed gauge study: part, operator and repeatability standard deviations.
    Msa {
        pecas: usize,
        operadores: usize,
        repeticoes: usize,
        media: f64,
        sd_peca: f64,
        sd_operador: f64,
        sd_repetibilidade: f64,
    },
    /// Groups (shifts, operators, machines) with the same mean but one, shifted by
    /// `deslocamento` standard deviations.
    MediaDeslocada {
        grupos: Vec<String>,
        n_por_grupo: usize,
        media: f64,
        sd: f64,
        afetado: usize,
        deslocamento: f64,
    },
    /// An individual series in control, then a pattern.
    Serie {
        n: usize,
        media: f64,
        sd: f64,
        padrao: Padrao,
    },
    /// A full two-level factorial (coded −1 and +1) with main effects and the A×B
    /// interaction (an effect is the change from the low to the high level).
    Fatorial {
        fatores: Vec<String>,
        replicas: usize,
        media: f64,
        sd: f64,
        efeitos: Vec<f64>,
        interacao_ab: f64,
    },
    /// Two alternatives run with common random numbers: each replication gives the result
    /// of A and of B, which share most of the noise. B differs from A by `diferenca` on
    /// average, with `sd_diferenca` of its own noise.
    Pareado {
        n: usize,
        media: f64,
        sd: f64,
        diferenca: f64,
        sd_diferenca: f64,
    },
    /// `k` subgroups of `tamanho` measurements in time order (X̄-R charts, capability with
    /// subgroups), in control or with a pattern by subgroup.
    Subgrupos {
        k: usize,
        tamanho: usize,
        media: f64,
        sd: f64,
        #[serde(default)]
        padrao: Option<Padrao>,
    },
    /// Collection records of one run of the mission's model: the time each item spends in
    /// each station of `estacoes` (processors), from `duracao` simulated time units with
    /// no warm-up. The process time of station `afetado` is replaced by `processo`, the
    /// planted cause.
    Simulacao {
        estacoes: Vec<String>,
        afetado: usize,
        processo: Dist,
        duracao: f64,
    },
}

/// At most this many records leave a simulation run, so a long run does not make a
/// table no one can read.
pub const MAX_REGISTROS: usize = 5000;

/// The processors of `modelo`, the stations a simulation set can record.
pub fn estacoes_do_modelo(modelo: &Model) -> Vec<String> {
    modelo
        .objects
        .iter()
        .filter(|o| matches!(o.kind, ObjectKind::Processor { .. }))
        .map(|o| o.name.clone())
        .collect()
}

/// What is wrong with a simulation set against `modelo` (empty for the other kinds).
pub fn erros_da_simulacao(g: &Gerador, modelo: &Model) -> Vec<String> {
    let Gerador::Simulacao {
        estacoes,
        afetado,
        processo,
        duracao,
    } = g
    else {
        return Vec::new();
    };
    let mut e = Vec::new();
    let validas = estacoes_do_modelo(modelo);
    if estacoes.len() < 2 {
        e.push("escolha ao menos duas estações para comparar".into());
    }
    for (i, x) in estacoes.iter().enumerate() {
        if !validas.contains(x) {
            e.push(format!("a estação '{x}' não é uma máquina do modelo"));
        }
        if estacoes[..i].contains(x) {
            e.push(format!("a estação '{x}' aparece duas vezes"));
        }
    }
    if *afetado >= estacoes.len() {
        e.push("escolha a estação com a causa plantada".into());
    }
    if !(duracao.is_finite() && *duracao > 0.0) {
        e.push("a duração da coleta deve ser positiva".into());
    }
    if let Err(x) = processo.validate() {
        e.push(format!("tempo de processo plantado: {x}"));
    }
    e
}

impl Padrao {
    /// The shift of the mean at point `i` (0-based), in units of `sd`.
    fn extra(&self, i: usize) -> f64 {
        match self {
            Padrao::Deslocamento { inicio, tamanho } if i >= *inicio => *tamanho,
            Padrao::Tendencia { inicio, inclinacao } if i >= *inicio => {
                (i - inicio + 1) as f64 * inclinacao
            }
            _ => 0.0,
        }
    }

    fn descrever(&self, unidade: &str) -> String {
        match self {
            Padrao::Deslocamento { inicio, tamanho } => format!(
                "deslocamento de {} desvios padrão a partir do {unidade} {}",
                n4(*tamanho),
                inicio + 1
            ),
            Padrao::Tendencia { inicio, inclinacao } => format!(
                "tendência de {} desvio padrão por {unidade} a partir do {unidade} {}",
                n4(*inclinacao),
                inicio + 1
            ),
        }
    }
}

/// A generator with its seed: the same recipe gives the same data on every machine.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Receita {
    pub gerador: Gerador,
    pub semente: u64,
}

/// A generated set: the table and the planted answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Conjunto {
    pub cabecalho: Vec<String>,
    pub linhas: Vec<Vec<String>>,
    /// The numeric columns, row by row (labels are left out).
    pub valores: Vec<Vec<f64>>,
    /// The planted effect, in words.
    pub resposta: String,
}

impl Conjunto {
    /// The table as CSV with `;` and the decimal comma.
    pub fn csv(&self) -> String {
        let mut s = self.cabecalho.join(";");
        s.push('\n');
        for l in &self.linhas {
            s.push_str(&l.join(";"));
            s.push('\n');
        }
        s
    }
}

fn normal(rng: &mut SimRng, media: f64, sd: f64) -> f64 {
    if sd <= 0.0 {
        return media;
    }
    Dist::Normal { mean: media, sd }.sample(rng)
}

fn n4(v: f64) -> String {
    fmt_num(v, 4)
}

/// Every stored value has the 4 decimals of the CSV, so an analysis of the exported file
/// gives exactly the number the game computes.
fn r4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

/// `v` with the 4 decimals of the CSV (for the first values the editor proposes).
pub fn arredondar(v: f64) -> f64 {
    r4(v)
}

impl Receita {
    /// The set of a generator that needs no model (a simulation set comes out empty).
    pub fn gerar(&self) -> Conjunto {
        self.gerar_com(None)
    }

    /// The set, with the mission's model for a simulation set.
    pub fn gerar_com(&self, modelo: Option<&Model>) -> Conjunto {
        let mut rng = stream(self.semente, 0, 0x656E_7369_6E6F);
        match &self.gerador {
            Gerador::Simulacao {
                estacoes,
                afetado,
                processo,
                duracao,
            } => {
                let cabecalho = vec![
                    "estacao".into(),
                    "peca".into(),
                    "entrada".into(),
                    "saida".into(),
                    "tempo".into(),
                ];
                let resposta = format!(
                    "o tempo de processo de \"{}\" foi trocado por {}",
                    estacoes.get(*afetado).map_or("?", String::as_str),
                    descrever_dist(processo)
                );
                let r = modelo
                    .ok_or_else(|| "o conjunto precisa do modelo da missão".to_string())
                    .and_then(|m| {
                        registros(m, estacoes, *afetado, processo, *duracao, self.semente)
                    });
                match r {
                    Ok((linhas, valores)) => Conjunto {
                        cabecalho,
                        linhas,
                        valores,
                        resposta,
                    },
                    Err(e) => Conjunto {
                        cabecalho,
                        linhas: Vec::new(),
                        valores: Vec::new(),
                        resposta: e,
                    },
                }
            }
            Gerador::Tempos { dist, n } => {
                let xs: Vec<f64> = (0..*n).map(|_| r4(dist.sample(&mut rng))).collect();
                Conjunto {
                    cabecalho: vec!["observacao".into(), "tempo".into()],
                    linhas: xs
                        .iter()
                        .enumerate()
                        .map(|(i, x)| vec![(i + 1).to_string(), n4(*x)])
                        .collect(),
                    valores: xs.iter().map(|x| vec![*x]).collect(),
                    resposta: format!("{n} valores de {dist:?}"),
                }
            }
            Gerador::Msa {
                pecas,
                operadores,
                repeticoes,
                media,
                sd_peca,
                sd_operador,
                sd_repetibilidade,
            } => {
                let p: Vec<f64> = (0..*pecas)
                    .map(|_| normal(&mut rng, 0.0, *sd_peca))
                    .collect();
                let o: Vec<f64> = (0..*operadores)
                    .map(|_| normal(&mut rng, 0.0, *sd_operador))
                    .collect();
                let mut linhas = Vec::new();
                let mut valores = Vec::new();
                for r in 0..*repeticoes {
                    for (j, oj) in o.iter().enumerate() {
                        for (i, pi) in p.iter().enumerate() {
                            let y = r4(media + pi + oj + normal(&mut rng, 0.0, *sd_repetibilidade));
                            linhas.push(vec![
                                format!("P{:02}", i + 1),
                                format!("Op{}", j + 1),
                                (r + 1).to_string(),
                                n4(y),
                            ]);
                            valores.push(vec![i as f64, j as f64, r as f64, y]);
                        }
                    }
                }
                Conjunto {
                    cabecalho: vec![
                        "peca".into(),
                        "operador".into(),
                        "repeticao".into(),
                        "medida".into(),
                    ],
                    linhas,
                    valores,
                    resposta: format!(
                        "desvios plantados: peça {}, operador {}, repetibilidade {}",
                        n4(*sd_peca),
                        n4(*sd_operador),
                        n4(*sd_repetibilidade)
                    ),
                }
            }
            Gerador::MediaDeslocada {
                grupos,
                n_por_grupo,
                media,
                sd,
                afetado,
                deslocamento,
            } => {
                let mut linhas = Vec::new();
                let mut valores = Vec::new();
                for (g, nome) in grupos.iter().enumerate() {
                    let m = if g == *afetado {
                        media + deslocamento * sd
                    } else {
                        *media
                    };
                    for _ in 0..*n_por_grupo {
                        let y = r4(normal(&mut rng, m, *sd));
                        linhas.push(vec![nome.clone(), n4(y)]);
                        valores.push(vec![g as f64, y]);
                    }
                }
                Conjunto {
                    cabecalho: vec!["grupo".into(), "medida".into()],
                    linhas,
                    valores,
                    resposta: format!(
                        "a média de \"{}\" está deslocada em {} desvios padrão",
                        grupos.get(*afetado).map_or("?", String::as_str),
                        n4(*deslocamento)
                    ),
                }
            }
            Gerador::Serie {
                n,
                media,
                sd,
                padrao,
            } => {
                let xs: Vec<f64> = (0..*n)
                    .map(|i| r4(normal(&mut rng, media + padrao.extra(i) * sd, *sd)))
                    .collect();
                let resposta = padrao.descrever("ponto");
                Conjunto {
                    cabecalho: vec!["ponto".into(), "medida".into()],
                    linhas: xs
                        .iter()
                        .enumerate()
                        .map(|(i, x)| vec![(i + 1).to_string(), n4(*x)])
                        .collect(),
                    valores: xs.iter().map(|x| vec![*x]).collect(),
                    resposta,
                }
            }
            Gerador::Fatorial {
                fatores,
                replicas,
                media,
                sd,
                efeitos,
                interacao_ab,
            } => {
                let k = fatores.len();
                let mut linhas = Vec::new();
                let mut valores = Vec::new();
                for _ in 0..*replicas {
                    for corrida in 0..(1usize << k) {
                        let x: Vec<f64> = (0..k)
                            .map(|f| if corrida >> f & 1 == 1 { 1.0 } else { -1.0 })
                            .collect();
                        let mut y = *media;
                        for (xi, e) in x.iter().zip(efeitos) {
                            y += e / 2.0 * xi;
                        }
                        if k >= 2 {
                            y += interacao_ab / 2.0 * x[0] * x[1];
                        }
                        y = r4(y + normal(&mut rng, 0.0, *sd));
                        let mut l: Vec<String> =
                            x.iter().map(|v| format!("{}", *v as i32)).collect();
                        l.push(n4(y));
                        linhas.push(l);
                        let mut v = x.clone();
                        v.push(y);
                        valores.push(v);
                    }
                }
                let mut cab = fatores.clone();
                cab.push("resposta".into());
                Conjunto {
                    cabecalho: cab,
                    linhas,
                    valores,
                    resposta: format!(
                        "efeitos principais {}, interação {}×{} de {}",
                        efeitos
                            .iter()
                            .map(|e| n4(*e))
                            .collect::<Vec<_>>()
                            .join(", "),
                        fatores.first().map_or("A", String::as_str),
                        fatores.get(1).map_or("B", String::as_str),
                        n4(*interacao_ab)
                    ),
                }
            }
            Gerador::Pareado {
                n,
                media,
                sd,
                diferenca,
                sd_diferenca,
            } => {
                let mut linhas = Vec::new();
                let mut valores = Vec::new();
                for i in 0..*n {
                    let a = r4(normal(&mut rng, *media, *sd));
                    let b = r4(a + normal(&mut rng, *diferenca, *sd_diferenca));
                    linhas.push(vec![(i + 1).to_string(), n4(a), n4(b)]);
                    valores.push(vec![a, b]);
                }
                Conjunto {
                    cabecalho: vec!["replicacao".into(), "a".into(), "b".into()],
                    linhas,
                    valores,
                    resposta: format!(
                        "B difere de A em {} em média (desvio da diferença {})",
                        n4(*diferenca),
                        n4(*sd_diferenca)
                    ),
                }
            }
            Gerador::Subgrupos {
                k,
                tamanho,
                media,
                sd,
                padrao,
            } => {
                let mut linhas = Vec::new();
                let mut valores = Vec::new();
                for g in 0..*k {
                    let m = media + padrao.as_ref().map_or(0.0, |p| p.extra(g)) * sd;
                    let xs: Vec<f64> = (0..*tamanho)
                        .map(|_| r4(normal(&mut rng, m, *sd)))
                        .collect();
                    let mut l = vec![(g + 1).to_string()];
                    l.extend(xs.iter().map(|x| n4(*x)));
                    linhas.push(l);
                    valores.push(xs);
                }
                let mut cabecalho = vec!["subgrupo".to_string()];
                cabecalho.extend((1..=*tamanho).map(|j| format!("x{j}")));
                Conjunto {
                    cabecalho,
                    linhas,
                    valores,
                    resposta: padrao.as_ref().map_or_else(
                        || "processo sob controle".to_string(),
                        |p| p.descrever("subgrupo"),
                    ),
                }
            }
        }
    }
}

/// A distribution in words, with the decimal comma.
pub fn descrever_dist(d: &Dist) -> String {
    match d {
        Dist::Const(v) => format!("constante {}", n4(*v)),
        Dist::Uniform { min, max } => format!("uniforme de {} a {}", n4(*min), n4(*max)),
        Dist::Exp { mean } => format!("exponencial de média {}", n4(*mean)),
        Dist::Triangular { min, mode, max } => {
            format!("triangular ({}, {}, {})", n4(*min), n4(*mode), n4(*max))
        }
        Dist::Normal { mean, sd } => {
            format!("normal de média {} e desvio {}", n4(*mean), n4(*sd))
        }
        Dist::LogNormal { mean, sd } => {
            format!("lognormal de média {} e desvio {}", n4(*mean), n4(*sd))
        }
        d => format!("{d:?}"),
    }
}

/// The rows of a table and their numeric values.
type Registros = (Vec<Vec<String>>, Vec<Vec<f64>>);

/// The records of one run: per item leaving a station, the station, the item (numbered
/// in order of arrival), the entry and exit times and the time spent. `valores` keeps the
/// station index and the time, like the groups of a shifted mean.
fn registros(
    modelo: &Model,
    estacoes: &[String],
    afetado: usize,
    processo: &Dist,
    duracao: f64,
    semente: u64,
) -> Result<Registros, String> {
    let g = Gerador::Simulacao {
        estacoes: estacoes.to_vec(),
        afetado,
        processo: processo.clone(),
        duracao,
    };
    if let Some(e) = erros_da_simulacao(&g, modelo).into_iter().next() {
        return Err(e);
    }
    let mut m = modelo.clone();
    if let Some(o) = m.objects.iter_mut().find(|o| o.name == estacoes[afetado])
        && let ObjectKind::Processor { process, .. } = &mut o.kind
    {
        *process = processo.clone();
    }
    m.run.seed = semente;
    m.run.warmup = 0.0;
    m.run.stop_time = duracao;
    m.run.replications = 1;
    let replay = m.replay(0).map_err(|e| e.to_string())?;
    // Block index to station index.
    let grupo: BTreeMap<usize, usize> = replay
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(b, bl)| estacoes.iter().position(|x| *x == bl.name).map(|g| (b, g)))
        .collect();
    let mut numero: BTreeMap<u64, usize> = BTreeMap::new();
    let mut entrada: BTreeMap<(u64, usize), f64> = BTreeMap::new();
    let mut linhas = Vec::new();
    let mut valores = Vec::new();
    for r in &replay.records {
        let (item, de, para) = match &r.event {
            TraceEvent::Created { item, block } => (*item, None, Some(*block)),
            TraceEvent::Moved { item, from, to } => (*item, Some(*from), Some(*to)),
            TraceEvent::Unpacked { item, block } => (*item, None, Some(*block)),
            _ => continue,
        };
        let proximo = numero.len() + 1;
        let n = *numero.entry(item).or_insert(proximo);
        if let Some(b) = de
            && let Some(g) = grupo.get(&b)
            && let Some(t0) = entrada.remove(&(item, b))
        {
            let t0 = r4(t0);
            let t1 = r4(r.t);
            let tempo = r4(t1 - t0);
            linhas.push(vec![
                estacoes[*g].clone(),
                n.to_string(),
                n4(t0),
                n4(t1),
                n4(tempo),
            ]);
            valores.push(vec![*g as f64, tempo]);
            if linhas.len() >= MAX_REGISTROS {
                break;
            }
        }
        if let Some(b) = para
            && grupo.contains_key(&b)
        {
            entrada.insert((item, b), r.t);
        }
    }
    Ok((linhas, valores))
}
