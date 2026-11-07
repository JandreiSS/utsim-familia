//! Running a mission: the variant of the student over the base model, then the decisions
//! of each scenario, all with the same seeds (common random numbers).

use std::collections::BTreeMap;

use utsim_model::experiment::{Scenario, apply_patches, run_scenarios};
use utsim_model::replay::Replay;
use utsim_model::{Model, ModelError};

use crate::missao::{Decisoes, Missao, Termo};
use crate::pacote::MissaoCarregada;
use crate::variante;

/// One scenario run: its decisions, their cost and every metric per replication.
#[derive(Clone, Debug, PartialEq)]
pub struct Cenario {
    pub decisoes: Decisoes,
    pub custo: f64,
    pub metricas: BTreeMap<String, Vec<f64>>,
}

/// The three scenarios a grade compares.
#[derive(Clone, Debug, PartialEq)]
pub struct Simulacao {
    /// The initial state.
    pub base: Cenario,
    /// The student's attempt.
    pub aluno: Cenario,
    /// The reference solution.
    pub referencia: Cenario,
}

#[derive(Debug, thiserror::Error)]
pub enum ErroMotor {
    #[error("execução cancelada")]
    Cancelada,
    #[error("{0}")]
    Modelo(String),
    #[error("a métrica '{0}' não saiu da simulação")]
    MetricaAusente(String),
}

impl From<ModelError> for ErroMotor {
    fn from(e: ModelError) -> Self {
        match e {
            ModelError::Cancelled => ErroMotor::Cancelada,
            e => ErroMotor::Modelo(e.to_string()),
        }
    }
}

/// The base model of `aluno`: the variant applied and the mission's replications.
pub fn modelo_base(m: &MissaoCarregada, aluno: &str) -> Result<Model, ErroMotor> {
    let mut modelo = apply_patches(&m.modelo, &variante::ajustes(&m.missao, aluno))?;
    modelo.run.replications = m.missao.rodadas.replicacoes;
    Ok(modelo)
}

/// The model of an attempt (variant and decisions applied), e.g. to take it to UTSim.
pub fn modelo_da_tentativa(
    m: &MissaoCarregada,
    aluno: &str,
    decisoes: &Decisoes,
) -> Result<Model, ErroMotor> {
    let base = modelo_base(m, aluno)?;
    Ok(apply_patches(&base, &ajustes(m, decisoes))?)
}

/// The model keys of `decisoes`: the levers and the compiled pieces.
pub fn ajustes(m: &MissaoCarregada, decisoes: &Decisoes) -> BTreeMap<String, String> {
    let objetos: Vec<String> = m.modelo.objects.iter().map(|o| o.name.clone()).collect();
    m.missao.ajustes_completos(decisoes, &objetos)
}

/// Runs each set of decisions. `progresso(fração)` is called as replications finish;
/// returning `false` cancels.
pub fn rodar(
    m: &MissaoCarregada,
    aluno: &str,
    lista: &[Decisoes],
    progresso: &(dyn Fn(f32) -> bool + Sync),
) -> Result<Vec<Cenario>, ErroMotor> {
    let base = modelo_base(m, aluno)?;
    let scenarios: Vec<Scenario> = lista
        .iter()
        .enumerate()
        .map(|(i, d)| Scenario {
            name: format!("c{i}"),
            set: ajustes(m, d),
        })
        .collect();
    let total = (lista.len() as f32 * base.run.replications.max(1) as f32).max(1.0);
    let reps = base.run.replications;
    let res = run_scenarios(&base, &scenarios, &[], &|sc, done| {
        progresso((sc as f32 * reps as f32 + done as f32) / total)
    })?;
    Ok(res
        .into_iter()
        .zip(lista)
        .map(|(r, d)| {
            let d = m.missao.completar(d);
            Cenario {
                custo: m.missao.custo(&d),
                decisoes: d,
                metricas: r.per_replication,
            }
        })
        .collect())
}

/// The initial state, the attempt and the reference of `aluno`.
pub fn simular(
    m: &MissaoCarregada,
    aluno: &str,
    decisoes: &Decisoes,
    progresso: &(dyn Fn(f32) -> bool + Sync),
) -> Result<Simulacao, ErroMotor> {
    let lista = [
        m.missao.decisoes_iniciais(),
        m.missao.completar(decisoes),
        m.missao.completar(&m.missao.referencia),
    ];
    let mut v = rodar(m, aluno, &lista, progresso)?;
    let referencia = v.pop().expect("3");
    let aluno = v.pop().expect("2");
    let base = v.pop().expect("1");
    Ok(Simulacao {
        base,
        aluno,
        referencia,
    })
}

/// One replication of the attempt with its trace, for the 2D view.
pub fn replay(m: &MissaoCarregada, aluno: &str, decisoes: &Decisoes) -> Result<Replay, ErroMotor> {
    let mut modelo = modelo_da_tentativa(m, aluno, decisoes)?;
    let duracao = m
        .missao
        .animacao
        .duracao
        .unwrap_or(480.0)
        .min(modelo.run.stop_time);
    modelo.run.stop_time = duracao;
    modelo.run.warmup = 0.0;
    Ok(modelo.replay(0)?)
}

/// `termos` per replication of `cenario`.
pub fn avaliar(
    missao: &Missao,
    termos: &[Termo],
    cenario: &Cenario,
) -> Result<Vec<f64>, ErroMotor> {
    let n = cenario
        .metricas
        .values()
        .next()
        .map_or(missao.rodadas.replicacoes as usize, Vec::len);
    let mut out = vec![0.0; n];
    for t in termos {
        let col: Vec<f64> = match t.chave.as_str() {
            "const" => vec![1.0; n],
            "custo" => vec![cenario.custo; n],
            k if crate::pecas::e_termo(k) => {
                let v = crate::pecas::termo(missao, k, &cenario.decisoes)
                    .ok_or_else(|| ErroMotor::MetricaAusente(k.into()))?;
                vec![v; n]
            }
            k if k.starts_with("decisao.") => {
                let id = &k["decisao.".len()..];
                let a = missao
                    .alavanca(id)
                    .ok_or_else(|| ErroMotor::MetricaAusente(k.into()))?;
                let v = cenario.decisoes.get(id).copied().unwrap_or(a.inicial());
                vec![a.numero(v); n]
            }
            k => cenario
                .metricas
                .get(k)
                .cloned()
                .ok_or_else(|| ErroMotor::MetricaAusente(k.into()))?,
        };
        for (o, x) in out.iter_mut().zip(col) {
            *o += t.fator * x;
        }
    }
    Ok(out)
}
