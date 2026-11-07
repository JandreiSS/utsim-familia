//! Data with planted effects (`docs/plano_ensino_e_jogo.md`, 6.3): seeded generators of
//! teaching data sets whose answer is known (the process time distribution, the variance
//! components of a gauge study, a shifted mean in one group, a series with a pattern, the
//! effects of a factorial). Each set keeps its recipe and the planted answer, which the
//! grading uses; the recovery tests use the same functions the student will use.

use serde::{Deserialize, Serialize};
use utsim_core::Dist;
use utsim_core::rng::{SimRng, stream};

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

impl Receita {
    pub fn gerar(&self) -> Conjunto {
        let mut rng = stream(self.semente, 0, 0x656E_7369_6E6F);
        match &self.gerador {
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
                    .map(|i| {
                        let extra = match padrao {
                            Padrao::Deslocamento { inicio, tamanho } if i >= *inicio => {
                                tamanho * sd
                            }
                            Padrao::Tendencia { inicio, inclinacao } if i >= *inicio => {
                                (i - inicio + 1) as f64 * inclinacao * sd
                            }
                            _ => 0.0,
                        };
                        r4(normal(&mut rng, media + extra, *sd))
                    })
                    .collect();
                let resposta = match padrao {
                    Padrao::Deslocamento { inicio, tamanho } => {
                        format!(
                            "deslocamento de {} desvios padrão a partir do ponto {}",
                            n4(*tamanho),
                            inicio + 1
                        )
                    }
                    Padrao::Tendencia { inicio, inclinacao } => format!(
                        "tendência de {} desvio padrão por ponto a partir do ponto {}",
                        n4(*inclinacao),
                        inicio + 1
                    ),
                };
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
        }
    }
}
