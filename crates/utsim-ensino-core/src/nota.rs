//! The grade of an attempt (`docs/plano_ensino_e_jogo.md`, section 3.4): constraint gate,
//! performance against the initial state and the reference, prediction, justification,
//! hint and run discounts, and stars.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_core::stats::Summary;

use crate::missao::{Decisoes, Metrica, Missao, Objetivo, Previsao, Resposta};
use crate::motor::{Cenario, ErroMotor, Simulacao, avaliar, valor_metrica};
use crate::pacote::Pesos;

/// What the student sends: decisions and answers, never the grade.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Tentativa {
    pub decisoes: Decisoes,
    /// Prediction id to answer.
    #[serde(default)]
    pub previsoes: BTreeMap<String, RespostaPrevisao>,
    /// The option picked in each justification question.
    #[serde(default)]
    pub justificativas: Vec<Option<usize>>,
    /// Highest hint level opened (0 to 3).
    #[serde(default)]
    pub dicas: u8,
    /// Number of this run in the mission (1 for the first).
    #[serde(default = "um")]
    pub rodada: u32,
    /// The delivered model (`.ron` text) of a modelling mission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modelo: Option<String>,
    /// The student the data of the questions are generated for (the teacher's grading
    /// sets it from the signed record).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub aluno: String,
}

fn um() -> u32 {
    1
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum RespostaPrevisao {
    Escolha(usize),
    Valor(f64),
}

/// Mean and 95 % half-width over the replications.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Intervalo {
    pub media: f64,
    pub meia_largura: f64,
    pub n: usize,
}

impl Intervalo {
    pub fn de(xs: &[f64]) -> Intervalo {
        let s = Summary::from_samples(xs);
        Intervalo {
            media: s.mean,
            meia_largura: if s.half_width.is_nan() {
                0.0
            } else {
                s.half_width
            },
            n: s.n,
        }
    }
    pub fn baixo(&self) -> f64 {
        self.media - self.meia_largura
    }
    pub fn alto(&self) -> f64 {
        self.media + self.meia_largura
    }
}

/// Where a measure stands against its goal.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Situacao {
    /// The whole interval is on the right side of the goal.
    Atingida,
    /// The interval contains the goal and the mean is within the tolerance band.
    DentroDaTolerancia,
    NaoAtingida,
    /// The measure has no goal.
    SemMeta,
}

impl Situacao {
    pub fn ok(self) -> bool {
        !matches!(self, Situacao::NaoAtingida)
    }
}

/// The situation of `v` (interval) against `meta`.
pub fn situacao(metrica: &Metrica, v: &Intervalo) -> Situacao {
    let Some(meta) = metrica.meta else {
        return Situacao::SemMeta;
    };
    let tol = metrica.tolerancia.faixa(meta);
    match metrica.objetivo {
        Objetivo::Menor => {
            if v.alto() < meta {
                Situacao::Atingida
            } else if v.baixo() <= meta && v.media <= meta + tol {
                Situacao::DentroDaTolerancia
            } else {
                Situacao::NaoAtingida
            }
        }
        Objetivo::Maior => {
            if v.baixo() > meta {
                Situacao::Atingida
            } else if v.alto() >= meta && v.media >= meta - tol {
                Situacao::DentroDaTolerancia
            } else {
                Situacao::NaoAtingida
            }
        }
    }
}

/// `d_k`: how much of the way from the initial state to the reference the student went,
/// limited to 0 and 1 (and the unlimited value, for the seal). When the reference is not
/// better than the initial state in this measure (a cost the reference pays to win in
/// another), it is 1 when the student is at least as good as the reference, else 0.
pub fn fracao(objetivo: Objetivo, aluno: f64, base: f64, referencia: f64, tol: f64) -> (f64, f64) {
    let ganho = match objetivo {
        Objetivo::Menor => base - referencia,
        Objetivo::Maior => referencia - base,
    };
    if ganho <= 1e-12_f64.max(tol * 1e-6) {
        let ok = objetivo.melhor_ou_igual(aluno, referencia) || (aluno - referencia).abs() <= tol;
        let d = if ok { 1.0 } else { 0.0 };
        return (d, d);
    }
    let d = match objetivo {
        Objetivo::Menor => (base - aluno) / ganho,
        Objetivo::Maior => (aluno - base) / ganho,
    };
    (d.clamp(0.0, 1.0), d)
}

/// The result of one measure.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResultadoMetrica {
    pub nome: String,
    pub rotulo: String,
    pub unidade: String,
    pub casas: u32,
    pub objetivo: Objetivo,
    pub meta: Option<f64>,
    pub base: Intervalo,
    pub aluno: Intervalo,
    pub referencia: Intervalo,
    pub situacao: Situacao,
    /// `d_k` limited to 0 and 1.
    pub fracao: f64,
    pub peso: f64,
}

/// The result of one prediction.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResultadoPrevisao {
    pub id: String,
    pub certa: bool,
    /// The right option or value, as text.
    pub esperado: String,
    /// The right option (choices) or value (numbers).
    pub esperado_numero: f64,
}

/// The whole grade of an attempt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Resultado {
    /// Why the attempt broke a constraint (then it is not solved).
    pub portao: Option<String>,
    pub custo: f64,
    pub metricas: Vec<ResultadoMetrica>,
    pub previsoes: Vec<ResultadoPrevisao>,
    pub justificativas_certas: usize,
    /// Weighted mean of the `d_k` (0 to 1).
    pub desempenho: f64,
    pub pontos_desempenho: f64,
    pub pontos_previsao: f64,
    pub pontos_justificativa: f64,
    /// Points available (the weights of the parts the mission has).
    pub pontos_possiveis: f64,
    pub desconto_dicas: f64,
    pub desconto_rodadas: f64,
    /// 0 to 100, one decimal.
    pub nota: f64,
    pub resolvida: bool,
    pub estrelas: u8,
    /// Better than the reference.
    pub selo: bool,
    /// Automatic hints about what happened (no discount).
    pub dicas_automaticas: Vec<String>,
    /// The criteria of a modelling mission (the delivered model), what matched and what
    /// did not.
    #[serde(default)]
    pub criterios: Vec<crate::modelagem::ResultadoCriterio>,
}

/// Why `decisoes` break a constraint of `missao` (`None` when they pass).
pub fn portao(missao: &Missao, decisoes: &Decisoes) -> Option<String> {
    let iniciais = missao.decisoes_iniciais();
    for (k, v) in decisoes {
        if (k.starts_with("tarefa.") || k.starts_with("pos.")) && iniciais.contains_key(k) {
            continue;
        }
        match missao.alavanca(k) {
            None => return Some(format!("a alavanca '{k}' não existe")),
            Some(a) if !a.aceita(*v) => {
                return Some(format!("'{}' fora da faixa permitida", a.rotulo()));
            }
            _ => {}
        }
    }
    let d = missao.completar(decisoes);
    if let Some(b) = &missao.balanceamento
        && let Some(v) = b.violacao(&b.atribuicao(&d))
    {
        return Some(v);
    }
    if let Some(a) = &missao.arranjo
        && let Some(v) = a.violacao(&a.posicoes(&d))
    {
        return Some(v);
    }
    let custo = missao.custo(&d);
    if let Some(orc) = missao.orcamento
        && custo > orc + 1e-9
    {
        return Some(format!(
            "o custo das decisões ({}) passa do orçamento ({})",
            dinheiro(custo),
            dinheiro(orc)
        ));
    }
    for r in &missao.restricoes {
        let mut v = 0.0;
        for t in &r.valor {
            let x = match t.chave.as_str() {
                "const" => 1.0,
                "custo" => custo,
                k if crate::pecas::e_termo(k) => crate::pecas::termo(missao, k, &d).unwrap_or(0.0),
                k => k
                    .strip_prefix("decisao.")
                    .and_then(|id| missao.alavanca(id).map(|a| a.numero(d[id])))
                    .unwrap_or(0.0),
            };
            v += t.fator * x;
        }
        if v > r.maximo + 1e-9 {
            return Some(format!(
                "{}: {} passa do limite de {}",
                r.rotulo,
                fmt_num(v, 2),
                fmt_num(r.maximo, 2)
            ));
        }
    }
    None
}

/// Money in reais, with the thousands point (`R$ 80.000`).
pub fn dinheiro(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    format!("{}R$ {out}", if n < 0 { "-" } else { "" })
}

/// A number with `casas` decimals and the decimal comma.
pub fn fmt_num(v: f64, casas: u32) -> String {
    format!("{:.*}", casas as usize, v).replace('.', ",")
}

/// Grades `tentativa` from the three scenarios of `sim`.
pub fn corrigir(
    missao: &Missao,
    pesos: &Pesos,
    sim: &Simulacao,
    tentativa: &Tentativa,
) -> Result<Resultado, ErroMotor> {
    let portao = portao(missao, &tentativa.decisoes);
    let mut metricas = Vec::new();
    let (mut soma, mut soma_bruta, mut soma_pesos) = (0.0, 0.0, 0.0);
    for m in &missao.metricas {
        let base = Intervalo::de(&valor_metrica(missao, m, &sim.base)?);
        let aluno = Intervalo::de(&valor_metrica(missao, m, &sim.aluno)?);
        let referencia = Intervalo::de(&valor_metrica(missao, m, &sim.referencia)?);
        let tol = m.tolerancia.faixa(m.meta.unwrap_or(referencia.media));
        let (d, bruta) = fracao(m.objetivo, aluno.media, base.media, referencia.media, tol);
        soma += m.peso * d;
        soma_bruta += m.peso * bruta;
        soma_pesos += m.peso;
        metricas.push(ResultadoMetrica {
            nome: m.nome.clone(),
            rotulo: m.rotulo.clone(),
            unidade: m.unidade.clone(),
            casas: m.casas,
            objetivo: m.objetivo,
            meta: m.meta,
            situacao: situacao(m, &aluno),
            base,
            aluno,
            referencia,
            fracao: d,
            peso: m.peso,
        });
    }
    let previsoes = corrigir_previsoes(missao, &sim.base, tentativa)?;
    let certas_prev = previsoes.iter().filter(|p| p.certa).count();
    // A mission of questions only (no measure): the performance is the share of right
    // answers, and it is solved when every answer is right.
    let so_perguntas = missao.metricas.is_empty();
    let desempenho = if so_perguntas {
        certas_prev as f64 / missao.previsao.len().max(1) as f64
    } else if soma_pesos > 0.0 {
        soma / soma_pesos
    } else {
        0.0
    };
    let bruto = if soma_pesos > 0.0 {
        soma_bruta / soma_pesos
    } else {
        0.0
    };

    let justificativas_certas = missao
        .justificativa
        .iter()
        .enumerate()
        .filter(|(i, q)| tentativa.justificativas.get(*i).copied().flatten() == Some(q.certa))
        .count();

    let pontos_desempenho = if portao.is_none() && !so_perguntas {
        pesos.desempenho * desempenho
    } else {
        0.0
    };
    let pontos_previsao = if missao.previsao.is_empty() {
        0.0
    } else {
        pesos.previsao * certas_prev as f64 / missao.previsao.len() as f64
    };
    let pontos_justificativa = if missao.justificativa.is_empty() {
        0.0
    } else {
        pesos.justificativa * justificativas_certas as f64 / missao.justificativa.len() as f64
    };
    let mut pontos_possiveis = if so_perguntas { 0.0 } else { pesos.desempenho };
    if !missao.previsao.is_empty() {
        pontos_possiveis += pesos.previsao;
    }
    if !missao.justificativa.is_empty() {
        pontos_possiveis += pesos.justificativa;
    }
    let desconto_dicas = match tentativa.dicas.min(3) {
        0 => 0.0,
        n => pesos.dicas[n as usize - 1],
    };
    let desconto_rodadas = match missao.rodadas.orcamento {
        Some(n) if tentativa.rodada > n => {
            (tentativa.rodada - n) as f64 * missao.rodadas.desconto_por_extra
        }
        _ => 0.0,
    };
    let bruta = if pontos_possiveis > 0.0 {
        100.0 * (pontos_desempenho + pontos_previsao + pontos_justificativa) / pontos_possiveis
    } else {
        0.0
    };
    let nota =
        ((bruta - desconto_dicas - desconto_rodadas).clamp(0.0, 100.0) * 10.0).round() / 10.0;
    let resolvida = portao.is_none()
        && metricas.iter().all(|m| m.situacao.ok())
        && (!so_perguntas || certas_prev == missao.previsao.len());
    let estrelas = if !resolvida {
        0
    } else if desempenho >= 0.95 {
        3
    } else if desempenho >= 0.8 {
        2
    } else {
        1
    };
    let dicas_automaticas = crate::dicas::automaticas(missao, sim, &metricas)?;
    Ok(Resultado {
        portao,
        custo: sim.aluno.custo,
        metricas,
        previsoes,
        justificativas_certas,
        desempenho,
        pontos_desempenho,
        pontos_previsao,
        pontos_justificativa,
        pontos_possiveis,
        desconto_dicas,
        desconto_rodadas,
        nota,
        resolvida,
        estrelas,
        selo: resolvida && bruto > 1.0 + 1e-9,
        dicas_automaticas,
        criterios: Vec::new(),
    })
}

/// The right answer of each prediction, from the initial state, and whether the student
/// got it.
pub fn corrigir_previsoes(
    missao: &Missao,
    base: &Cenario,
    tentativa: &Tentativa,
) -> Result<Vec<ResultadoPrevisao>, ErroMotor> {
    let mut out = Vec::new();
    for p in &missao.previsao {
        let resposta = tentativa.previsoes.get(p.id());
        match p {
            Previsao::Escolha {
                id,
                opcoes,
                resposta: certa,
                ..
            } => {
                let i = match certa {
                    Resposta::Fixa(i) => *i,
                    Resposta::Maior | Resposta::Menor => {
                        let mut vals = Vec::new();
                        for o in opcoes {
                            vals.push(Intervalo::de(&avaliar(missao, &o.valor, base)?).media);
                        }
                        let mut best = 0;
                        for (j, v) in vals.iter().enumerate() {
                            let melhor = if *certa == Resposta::Maior {
                                *v > vals[best]
                            } else {
                                *v < vals[best]
                            };
                            if melhor {
                                best = j;
                            }
                        }
                        best
                    }
                };
                out.push(ResultadoPrevisao {
                    id: id.clone(),
                    certa: resposta == Some(&RespostaPrevisao::Escolha(i)),
                    esperado: opcoes.get(i).map_or_else(String::new, |o| o.rotulo.clone()),
                    esperado_numero: i as f64,
                });
            }
            Previsao::Dados {
                id,
                conjunto,
                questao,
                unidade,
                tolerancia,
                ..
            } => {
                let cj = missao.conjunto(conjunto).ok_or_else(|| {
                    ErroMotor::Modelo(format!("conjunto '{conjunto}' não existe"))
                })?;
                let dados = cj.gerar(&missao.id, &tentativa.aluno);
                let v = questao.resposta(&dados).map_err(ErroMotor::Modelo)?;
                let certa = if questao.e_escolha() {
                    resposta == Some(&RespostaPrevisao::Escolha(v as usize))
                } else {
                    let faixa = tolerancia.faixa(v);
                    matches!(resposta, Some(RespostaPrevisao::Valor(x)) if (x - v).abs() <= faixa + 1e-12)
                };
                let txt = questao.texto(cj, v);
                out.push(ResultadoPrevisao {
                    id: id.clone(),
                    certa,
                    esperado: if unidade.is_empty() || questao.e_escolha() {
                        txt
                    } else {
                        format!("{txt} {unidade}")
                    },
                    esperado_numero: v,
                });
            }
            Previsao::Valor { .. } | Previsao::Calculo { .. } => {
                let (id, unidade, tolerancia) = match p {
                    Previsao::Valor {
                        id,
                        unidade,
                        tolerancia,
                        ..
                    }
                    | Previsao::Calculo {
                        id,
                        unidade,
                        tolerancia,
                        ..
                    } => (id, unidade, tolerancia),
                    Previsao::Escolha { .. } | Previsao::Dados { .. } => {
                        unreachable!("matched above")
                    }
                };
                let v = match p {
                    Previsao::Valor { valor, .. } => {
                        Intervalo::de(&avaliar(missao, valor, base)?).media
                    }
                    Previsao::Calculo { calculo, .. } => calculo.valor(),
                    Previsao::Escolha { .. } | Previsao::Dados { .. } => {
                        unreachable!("matched above")
                    }
                };
                let faixa = tolerancia.faixa(v);
                let certa = matches!(resposta, Some(RespostaPrevisao::Valor(x)) if (x - v).abs() <= faixa + 1e-12);
                let txt = fmt_num(v, 2);
                out.push(ResultadoPrevisao {
                    id: id.clone(),
                    certa,
                    esperado: if unidade.is_empty() {
                        txt
                    } else {
                        format!("{txt} {unidade}")
                    },
                    esperado_numero: v,
                });
            }
        }
    }
    Ok(out)
}
