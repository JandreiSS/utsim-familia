//! Questions about the data sets of a mission (the Lean Six Sigma case, act 8). Each
//! student gets data generated from their own seed, with the planted effect sometimes in
//! another group, and the right answer is computed from those data by the same analyses
//! the UTSim Seis Sigma app uses (`utsim-quality`): the student can answer with the
//! reduced tools of the game or with the Seis Sigma app, and gets the same number.

use serde::{Deserialize, Serialize};

use crate::dados::{Conjunto, Gerador, Receita};
use crate::nota::fmt_num;

/// A data set of a mission.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConjuntoMissao {
    pub id: String,
    /// Name shown to the student and given to the exported file.
    pub nome: String,
    pub gerador: Gerador,
    /// Draw the affected group from the student's seed (shifted means).
    #[serde(default)]
    pub sortear_afetado: bool,
}

impl ConjuntoMissao {
    /// The recipe of `aluno` in mission `missao` (no student: a fixed seed).
    pub fn receita(&self, missao: &str, aluno: &str) -> Receita {
        let semente = if aluno.is_empty() {
            1
        } else {
            crate::variante::semente(&format!("{missao}/{}", self.id), aluno)
        };
        let mut gerador = self.gerador.clone();
        if self.sortear_afetado
            && let Gerador::MediaDeslocada {
                grupos, afetado, ..
            } = &mut gerador
            && !grupos.is_empty()
        {
            *afetado = (semente % grupos.len() as u64) as usize;
        }
        Receita { gerador, semente }
    }

    pub fn gerar(&self, missao: &str, aluno: &str) -> Conjunto {
        self.receita(missao, aluno).gerar()
    }

    /// The group names, for the questions that ask for a group.
    pub fn grupos(&self) -> Vec<String> {
        match &self.gerador {
            Gerador::MediaDeslocada { grupos, .. } => grupos.clone(),
            _ => Vec::new(),
        }
    }
}

/// What a question asks about a data set.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum QuestaoDados {
    /// %R&R of the gauge study (total GRR over the total variation, in % of the study
    /// variation), by the ANOVA method.
    RrPercentual,
    /// The verdict of the gauge study: 0 acceptable (below 10 %), 1 marginal (10 % to
    /// 30 %), 2 unacceptable (above 30 %).
    RrVeredito,
    /// The capability Ppk of individual values against the limits.
    Ppk { lie: Option<f64>, lse: Option<f64> },
    /// The group whose mean is shifted (the one farthest from the overall mean).
    GrupoDeslocado,
    /// The first point (1-based) after the first `base` points that breaks a Nelson rule
    /// on an individuals chart whose limits come from the first `base` points (centre at
    /// their mean, sigma from the mean moving range over d2 = 1.128).
    PrimeiroSinal { base: usize },
    /// The effect of factor `fator` (0 is the first) in a two-level factorial: the mean
    /// response at the high level minus the mean at the low level.
    Efeito { fator: usize },
    /// Half-width of the Student-t interval of the mean of the values, at `confianca`
    /// (0.95 for 95 %).
    MeiaLargura { confianca: f64 },
    /// Replications needed for a 95 % interval of half-width `meia_largura`, from these
    /// values as the pilot: `n = ⌈(t · s / h)²⌉`, with `t` of the pilot's degrees of
    /// freedom (never fewer than the pilot itself).
    Replicacoes { meia_largura: f64 },
    /// Mean of B − A of paired results.
    DiferencaMedia,
    /// Whether B differs from A by the paired t test at 5 %: 0 yes, 1 no.
    DiferencaSignificativa,
    /// The first subgroup (1-based) after the first `base` with a point beyond the limits
    /// of an X̄-R chart whose limits come from the first `base` subgroups.
    PrimeiroSubgrupoFora { base: usize },
    /// The capability Cpk of subgroups (sigma within from R̄/d2) against the limits.
    Cpk { lie: Option<f64>, lse: Option<f64> },
    /// The larger source of the measurement variation in a gauge study: 0 repeatability
    /// (the instrument), 1 reproducibility (the operators).
    FonteDominante,
}

impl QuestaoDados {
    /// Whether the answer is one of the options (a group, a verdict) instead of a number.
    pub fn e_escolha(&self) -> bool {
        matches!(
            self,
            QuestaoDados::RrVeredito
                | QuestaoDados::GrupoDeslocado
                | QuestaoDados::DiferencaSignificativa
                | QuestaoDados::FonteDominante
        )
    }

    /// The options of a choice question.
    pub fn opcoes(&self, conjunto: &ConjuntoMissao) -> Vec<String> {
        match self {
            QuestaoDados::RrVeredito => vec![
                "Aceitável (abaixo de 10 %)".into(),
                "Marginal (de 10 % a 30 %)".into(),
                "Inaceitável (acima de 30 %)".into(),
            ],
            QuestaoDados::GrupoDeslocado => conjunto.grupos(),
            QuestaoDados::DiferencaSignificativa => vec![
                "Sim, a diferença é significativa a 5 %".into(),
                "Não, a diferença pode ser só ruído".into(),
            ],
            QuestaoDados::FonteDominante => vec![
                "Repetibilidade (o instrumento)".into(),
                "Reprodutibilidade (os operadores)".into(),
            ],
            _ => Vec::new(),
        }
    }

    /// The right answer for the data `c` (the option index for a choice question).
    pub fn resposta(&self, c: &Conjunto) -> Result<f64, String> {
        match self {
            QuestaoDados::RrPercentual | QuestaoDados::RrVeredito => {
                let pct = rr_percentual(c)?;
                Ok(match self {
                    QuestaoDados::RrPercentual => pct,
                    _ if pct < 10.0 => 0.0,
                    _ if pct <= 30.0 => 1.0,
                    _ => 2.0,
                })
            }
            QuestaoDados::Ppk { lie, lse } => {
                let xs: Vec<f64> = c.valores.iter().filter_map(|v| v.last().copied()).collect();
                let grupos: Vec<Vec<f64>> = xs.iter().map(|x| vec![*x]).collect();
                let spec = utsim_quality::capability::Spec {
                    lsl: *lie,
                    usl: *lse,
                    target: None,
                };
                let cap =
                    utsim_quality::capability::capability(&grupos, &spec, &Default::default())?;
                Ok(cap.ppk.value)
            }
            QuestaoDados::GrupoDeslocado => {
                let n = c
                    .valores
                    .iter()
                    .map(|v| v[0] as usize)
                    .max()
                    .map_or(0, |m| m + 1);
                let medias: Vec<f64> = (0..n)
                    .map(|g| {
                        let xs: Vec<f64> = c
                            .valores
                            .iter()
                            .filter(|v| v[0] as usize == g)
                            .map(|v| v[1])
                            .collect();
                        xs.iter().sum::<f64>() / xs.len().max(1) as f64
                    })
                    .collect();
                let geral = medias.iter().sum::<f64>() / n.max(1) as f64;
                let i = (0..n)
                    .max_by(|a, b| {
                        (medias[*a] - geral)
                            .abs()
                            .total_cmp(&(medias[*b] - geral).abs())
                    })
                    .ok_or("sem grupos")?;
                Ok(i as f64)
            }
            QuestaoDados::PrimeiroSinal { base } => {
                let xs: Vec<f64> = c.valores.iter().map(|v| v[0]).collect();
                if xs.len() <= *base || *base < 2 {
                    return Err("série curta demais".into());
                }
                let fase1 = &xs[..*base];
                let centro = fase1.iter().sum::<f64>() / *base as f64;
                let mr =
                    fase1.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (*base - 1) as f64;
                let sigma = mr / 1.128;
                let z: Vec<Option<f64>> = xs
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        if i < *base {
                            None
                        } else {
                            Some((x - centro) / sigma)
                        }
                    })
                    .collect();
                let v = utsim_quality::RuleSet::nelson().evaluate(&z);
                v.iter()
                    .map(|x| x.index)
                    .filter(|i| *i >= *base)
                    .min()
                    .map(|i| (i + 1) as f64)
                    .ok_or_else(|| "nenhum sinal na série".into())
            }
            QuestaoDados::Efeito { fator } => {
                let y = c.valores.first().map_or(0, |v| v.len().saturating_sub(1));
                let (mut a, mut na, mut b, mut nb) = (0.0, 0.0, 0.0, 0.0);
                for v in &c.valores {
                    if v.get(*fator).copied().unwrap_or(0.0) > 0.0 {
                        a += v[y];
                        na += 1.0;
                    } else {
                        b += v[y];
                        nb += 1.0;
                    }
                }
                if na == 0.0 || nb == 0.0 {
                    return Err("fator sem os dois níveis".into());
                }
                Ok(a / na - b / nb)
            }
            QuestaoDados::MeiaLargura { confianca } => {
                let xs = coluna(c);
                let (_, s, n) = media_sd(&xs)?;
                Ok(utsim_stats::t_quantile(*confianca, n - 1) * s / (n as f64).sqrt())
            }
            QuestaoDados::Replicacoes { meia_largura } => {
                let xs = coluna(c);
                let (_, s, n) = media_sd(&xs)?;
                if *meia_largura <= 0.0 {
                    return Err("meia largura deve ser positiva".into());
                }
                let t = utsim_stats::t_quantile(0.95, n - 1);
                Ok(((t * s / meia_largura).powi(2).ceil()).max(n as f64))
            }
            QuestaoDados::DiferencaMedia | QuestaoDados::DiferencaSignificativa => {
                let a: Vec<f64> = c
                    .valores
                    .iter()
                    .filter_map(|v| v.first().copied())
                    .collect();
                let b: Vec<f64> = c.valores.iter().filter_map(|v| v.get(1).copied()).collect();
                let t = utsim_stats::hypothesis::t_paired(
                    &b,
                    &a,
                    0.0,
                    0.95,
                    utsim_stats::hypothesis::Alternative::TwoSided,
                )
                .ok_or("dados pareados insuficientes")?;
                Ok(match self {
                    QuestaoDados::DiferencaMedia => t.estimate,
                    _ if t.p_value < 0.05 => 0.0,
                    _ => 1.0,
                })
            }
            QuestaoDados::PrimeiroSubgrupoFora { base } => {
                use utsim_quality::charts::{ChartData, ChartKind, apply, estimate_limits};
                if c.valores.len() <= *base || *base < 2 {
                    return Err("poucos subgrupos".into());
                }
                let base_dados = ChartData::Variables(c.valores[..*base].to_vec());
                let limites = estimate_limits(ChartKind::XbarR, &base_dados, &[])?;
                let carta = apply(
                    &limites,
                    &ChartData::Variables(c.valores.clone()),
                    &utsim_quality::RuleSet::basic(),
                    &[],
                )?;
                carta
                    .panels
                    .iter()
                    .flat_map(|p| p.out_of_limits())
                    .filter(|i| *i >= *base)
                    .min()
                    .map(|i| (i + 1) as f64)
                    .ok_or_else(|| "nenhum subgrupo fora dos limites".into())
            }
            QuestaoDados::Cpk { lie, lse } => {
                let spec = utsim_quality::capability::Spec {
                    lsl: *lie,
                    usl: *lse,
                    target: None,
                };
                let cap =
                    utsim_quality::capability::capability(&c.valores, &spec, &Default::default())?;
                Ok(cap.cpk.value)
            }
            QuestaoDados::FonteDominante => {
                let (rep, reprod) = fontes_rr(c)?;
                Ok(if rep >= reprod { 0.0 } else { 1.0 })
            }
        }
    }

    /// The right answer as text.
    pub fn texto(&self, conjunto: &ConjuntoMissao, v: f64) -> String {
        if self.e_escolha() {
            self.opcoes(conjunto)
                .get(v as usize)
                .cloned()
                .unwrap_or_default()
        } else {
            fmt_num(v, 2)
        }
    }
}

/// The values of a one-column set (the last column of each row).
fn coluna(c: &Conjunto) -> Vec<f64> {
    c.valores.iter().filter_map(|v| v.last().copied()).collect()
}

/// Mean, sample standard deviation and size, with at least two values.
fn media_sd(xs: &[f64]) -> Result<(f64, f64, usize), String> {
    let n = xs.len();
    if n < 2 {
        return Err("são precisos ao menos 2 valores".into());
    }
    let m = xs.iter().sum::<f64>() / n as f64;
    let s = (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
    Ok((m, s, n))
}

fn gage(c: &Conjunto) -> Result<utsim_quality::msa::GageRR, String> {
    let ms: Vec<utsim_quality::msa::Measurement> = c
        .valores
        .iter()
        .map(|v| utsim_quality::msa::Measurement {
            part: format!("P{}", v[0] as usize + 1),
            operator: format!("Op{}", v[1] as usize + 1),
            value: v[3],
        })
        .collect();
    let data = utsim_quality::msa::GageData::from_measurements(&ms)?;
    utsim_quality::msa::gage_anova(&data, &Default::default())
}

/// The repeatability and reproducibility of a gauge study, in % of the study variation.
pub fn fontes_rr(c: &Conjunto) -> Result<(f64, f64), String> {
    let rr = gage(c)?;
    let pct = |nome: &str| {
        rr.components
            .iter()
            .find(|k| k.name == nome)
            .map_or(0.0, |k| k.pct_study_var)
    };
    Ok((
        pct(utsim_quality::msa::REPEATABILITY),
        pct(utsim_quality::msa::REPRODUCIBILITY),
    ))
}

/// %R&R (study variation) of a gauge data set (columns part, operator, repetition, value).
pub fn rr_percentual(c: &Conjunto) -> Result<f64, String> {
    let rr = gage(c)?;
    rr.components
        .iter()
        .find(|k| k.name == utsim_quality::msa::GRR)
        .map(|k| k.pct_study_var)
        .ok_or_else(|| "estudo sem GRR".into())
}

/// The reduced tools of the game: a short report of a data set, the same analysis the
/// Seis Sigma app shows in full.
pub fn ferramenta(conjunto: &ConjuntoMissao, c: &Conjunto) -> Vec<String> {
    match &conjunto.gerador {
        Gerador::Msa { .. } => match (rr_percentual(c), fontes_rr(c)) {
            (Ok(p), Ok((rep, reprod))) => vec![
                format!(
                    "Estudo de R&R (ANOVA): %R&R da variação do estudo = {} %",
                    fmt_num(p, 1)
                ),
                format!(
                    "repetibilidade {} %, reprodutibilidade {} %",
                    fmt_num(rep, 1),
                    fmt_num(reprod, 1)
                ),
            ],
            (Err(e), _) | (_, Err(e)) => vec![e],
        },
        Gerador::Pareado { .. } => {
            let a: Vec<f64> = c.valores.iter().map(|v| v[0]).collect();
            let b: Vec<f64> = c.valores.iter().map(|v| v[1]).collect();
            let d: Vec<f64> = b.iter().zip(&a).map(|(y, x)| y - x).collect();
            let mut out = Vec::new();
            for (nome, xs) in [("A", &a), ("B", &b), ("B − A", &d)] {
                if let Ok((m, s, n)) = media_sd(xs) {
                    let h = utsim_stats::t_quantile(0.95, n - 1) * s / (n as f64).sqrt();
                    out.push(format!(
                        "{nome}: média {}, desvio padrão {}, IC 95 % {} a {}",
                        fmt_num(m, 3),
                        fmt_num(s, 3),
                        fmt_num(m - h, 3),
                        fmt_num(m + h, 3)
                    ));
                }
            }
            out
        }
        Gerador::Subgrupos { .. } => {
            let k = c.valores.len() as f64;
            let medias: Vec<f64> = c
                .valores
                .iter()
                .map(|g| g.iter().sum::<f64>() / g.len().max(1) as f64)
                .collect();
            let amps: Vec<f64> = c
                .valores
                .iter()
                .map(|g| {
                    g.iter().copied().fold(f64::MIN, f64::max)
                        - g.iter().copied().fold(f64::MAX, f64::min)
                })
                .collect();
            let n = c.valores.first().map_or(1, Vec::len);
            let xbb = medias.iter().sum::<f64>() / k.max(1.0);
            let rb = amps.iter().sum::<f64>() / k.max(1.0);
            let d2 = utsim_quality::constants::d2(n);
            vec![
                format!(
                    "{} subgrupos de {n}: média das médias {}, amplitude média {}",
                    c.valores.len(),
                    fmt_num(xbb, 4),
                    fmt_num(rb, 4)
                ),
                format!(
                    "sigma dentro = amplitude média / d2 ({}) = {}",
                    fmt_num(d2, 3),
                    fmt_num(rb / d2, 4)
                ),
            ]
        }
        Gerador::MediaDeslocada { grupos, .. } => grupos
            .iter()
            .enumerate()
            .map(|(g, nome)| {
                let xs: Vec<f64> = c
                    .valores
                    .iter()
                    .filter(|v| v[0] as usize == g)
                    .map(|v| v[1])
                    .collect();
                let n = xs.len() as f64;
                let m = xs.iter().sum::<f64>() / n.max(1.0);
                let sd =
                    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
                format!(
                    "{nome}: média {}, desvio padrão {}, n = {}",
                    fmt_num(m, 3),
                    fmt_num(sd, 3),
                    xs.len()
                )
            })
            .collect(),
        Gerador::Tempos { .. } | Gerador::Serie { .. } => {
            let xs: Vec<f64> = c.valores.iter().map(|v| v[0]).collect();
            let n = xs.len() as f64;
            let m = xs.iter().sum::<f64>() / n.max(1.0);
            let sd = (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
            let mr = xs.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (n - 1.0).max(1.0);
            vec![
                format!(
                    "n = {}, média {}, desvio padrão {}",
                    xs.len(),
                    fmt_num(m, 3),
                    fmt_num(sd, 3)
                ),
                format!(
                    "amplitude móvel média {} (sigma = AM / 1,128 = {})",
                    fmt_num(mr, 3),
                    fmt_num(mr / 1.128, 3)
                ),
            ]
        }
        Gerador::Fatorial { fatores, .. } => fatores
            .iter()
            .enumerate()
            .map(
                |(i, f)| match (QuestaoDados::Efeito { fator: i }).resposta(c) {
                    Ok(e) => format!("efeito de {f}: {}", fmt_num(e, 3)),
                    Err(e) => e,
                },
            )
            .collect(),
    }
}
