//! The third autonomy step (`docs/plano_ensino_e_jogo.md`, 3.2 and 4.2): the student builds
//! the model in UTSim and delivers the `.ron`. The app checks that it loads, that it has
//! the minimum structure the package asks for (an inspection with rework, breakdowns, a
//! team) and that its measures match the reference model within the tolerance. A report
//! says what matched and what did not.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use utsim_model::{Model, ObjectKind};

use crate::missao::{Termo, Tolerancia};
use crate::nota::{Intervalo, fmt_num};

fn um_f() -> f64 {
    1.0
}

/// What a modelling mission checks.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Modelagem {
    /// The reference model inside the package (the answer key).
    pub gabarito: String,
    /// Replications of each model in the check (default 5).
    #[serde(default)]
    pub replicacoes: Option<u32>,
    pub criterios: Vec<Criterio>,
}

/// One criterion of the check.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Criterio {
    /// A measure of the run (object names as the statement asks) close to the one of the
    /// reference model: the student's interval meets the band `referência ± tolerância`.
    Medida {
        rotulo: String,
        valor: Vec<Termo>,
        tolerancia: Tolerancia,
        #[serde(default = "um_f")]
        peso: f64,
    },
    /// At least `minimo` objects of a type (`Processor`, `Queue`, `Combiner`...).
    Objetos {
        rotulo: String,
        tipo: String,
        minimo: u32,
        #[serde(default = "um_f")]
        peso: f64,
    },
    /// Some object with random breakdowns (`downtime`).
    Quebras {
        rotulo: String,
        #[serde(default = "um_f")]
        peso: f64,
    },
    /// Items can go back to an earlier object (a cycle in the connections).
    Retrabalho {
        rotulo: String,
        #[serde(default = "um_f")]
        peso: f64,
    },
    /// Some processing object of capacity at least `minimo` (parallel servers).
    Capacidade {
        rotulo: String,
        minimo: u32,
        #[serde(default = "um_f")]
        peso: f64,
    },
    /// Some processor uses a team of operators.
    Operadores {
        rotulo: String,
        #[serde(default = "um_f")]
        peso: f64,
    },
}

impl Criterio {
    pub fn rotulo(&self) -> &str {
        match self {
            Criterio::Medida { rotulo, .. }
            | Criterio::Objetos { rotulo, .. }
            | Criterio::Quebras { rotulo, .. }
            | Criterio::Retrabalho { rotulo, .. }
            | Criterio::Capacidade { rotulo, .. }
            | Criterio::Operadores { rotulo, .. } => rotulo,
        }
    }
    pub fn peso(&self) -> f64 {
        match self {
            Criterio::Medida { peso, .. }
            | Criterio::Objetos { peso, .. }
            | Criterio::Quebras { peso, .. }
            | Criterio::Retrabalho { peso, .. }
            | Criterio::Capacidade { peso, .. }
            | Criterio::Operadores { peso, .. } => *peso,
        }
    }
}

/// The verdict of one criterion.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResultadoCriterio {
    pub rotulo: String,
    pub ok: bool,
    /// What was found (the student's value and the band, or the missing structure).
    pub detalhe: String,
    pub peso: f64,
}

/// The type name of an object, as written in the model.
pub fn tipo(k: &ObjectKind) -> &'static str {
    utsim_model::replay::kind_name(k)
}

/// Whether the connections have a cycle (an item can come back).
pub fn tem_ciclo(m: &Model) -> bool {
    let nomes: Vec<&str> = m.objects.iter().map(|o| o.name.as_str()).collect();
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for c in &m.connections {
        adj.entry(c.0.as_str()).or_default().push(c.1.as_str());
    }
    // Depth first search with colours.
    fn visita<'a>(
        n: &'a str,
        adj: &BTreeMap<&'a str, Vec<&'a str>>,
        cor: &mut BTreeMap<&'a str, u8>,
    ) -> bool {
        cor.insert(n, 1);
        for &v in adj.get(n).map(Vec::as_slice).unwrap_or(&[]) {
            match cor.get(v).copied().unwrap_or(0) {
                1 => return true,
                0 if visita(v, adj, cor) => return true,
                _ => {}
            }
        }
        cor.insert(n, 2);
        false
    }
    let mut cor: BTreeMap<&str, u8> = BTreeMap::new();
    for n in nomes {
        if cor.get(n).copied().unwrap_or(0) == 0 && visita(n, &adj, &mut cor) {
            return true;
        }
    }
    false
}

/// The structural criteria of `m` (the measures are checked with the runs).
pub fn estrutura(c: &Criterio, m: &Model) -> Option<(bool, String)> {
    Some(match c {
        Criterio::Medida { .. } => return None,
        Criterio::Objetos {
            tipo: t, minimo, ..
        } => {
            let n = m.objects.iter().filter(|o| tipo(&o.kind) == t).count() as u32;
            (
                n >= *minimo,
                format!("{n} objeto(s) do tipo {t}, o mínimo é {minimo}"),
            )
        }
        Criterio::Quebras { .. } => {
            let com: Vec<&str> = m
                .objects
                .iter()
                .filter(|o| o.downtime.is_some())
                .map(|o| o.name.as_str())
                .collect();
            if com.is_empty() {
                (false, "nenhum objeto tem quebras (downtime)".into())
            } else {
                (true, format!("quebras em {}", com.join(", ")))
            }
        }
        Criterio::Retrabalho { .. } => {
            if tem_ciclo(m) {
                (true, "há um caminho de volta (retrabalho)".into())
            } else {
                (
                    false,
                    "nenhuma ligação leva os itens de volta a um objeto anterior".into(),
                )
            }
        }
        Criterio::Capacidade { minimo, .. } => {
            let max = m
                .objects
                .iter()
                .filter_map(|o| match &o.kind {
                    ObjectKind::Processor { capacity, .. } => Some(*capacity as u32),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            (
                max >= *minimo,
                format!("a maior capacidade de processamento é {max}, o mínimo é {minimo}"),
            )
        }
        Criterio::Operadores { .. } => {
            let com: BTreeSet<&str> = m
                .objects
                .iter()
                .filter(|o| {
                    matches!(
                        &o.kind,
                        ObjectKind::Processor {
                            operators: Some(_),
                            ..
                        }
                    )
                })
                .map(|o| o.name.as_str())
                .collect();
            if com.is_empty() {
                (false, "nenhum processador usa operadores".into())
            } else {
                (
                    true,
                    format!(
                        "operadores em {}",
                        com.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                )
            }
        }
    })
}

/// A measure of a run (per replication): metric keys and `const`.
pub fn medida(termos: &[Termo], metricas: &BTreeMap<String, Vec<f64>>) -> Result<Vec<f64>, String> {
    let n = metricas.values().next().map_or(1, Vec::len);
    let mut out = vec![0.0; n];
    for t in termos {
        let col = if t.chave == "const" {
            vec![1.0; n]
        } else {
            metricas.get(&t.chave).cloned().ok_or_else(|| {
                let obj = t.chave.split('.').next().unwrap_or(&t.chave);
                format!(
                    "o seu modelo não produz '{}' (há um objeto chamado '{obj}'?)",
                    t.chave
                )
            })?
        };
        for (o, x) in out.iter_mut().zip(col) {
            *o += t.fator * x;
        }
    }
    Ok(out)
}

/// Decimal places for a measure of size `v`: a count per week (hundreds) with none, a time
/// in the tens with one, a fraction such as a utilization with two.
pub fn casas_para(v: f64) -> u32 {
    match v.abs() {
        x if x >= 100.0 => 0,
        x if x >= 10.0 => 1,
        _ => 2,
    }
}

/// Compares a measure of the student's model with the reference one.
pub fn comparar(
    aluno: &Intervalo,
    referencia: &Intervalo,
    tol: Tolerancia,
    casas: u32,
) -> (bool, String) {
    let faixa = tol.faixa(referencia.media);
    let (lo, hi) = (referencia.media - faixa, referencia.media + faixa);
    let ok = aluno.alto() >= lo && aluno.baixo() <= hi;
    (
        ok,
        format!(
            "seu modelo: {} ± {}; esperado entre {} e {}",
            fmt_num(aluno.media, casas),
            fmt_num(aluno.meia_largura, casas),
            fmt_num(lo, casas),
            fmt_num(hi, casas)
        ),
    )
}

/// Grades a delivered model: it must load, have the structure the package asks for and
/// give the measures of the reference model within the tolerance. The predictions are
/// checked against the reference model.
pub fn corrigir_modelagem(
    m: &crate::pacote::MissaoCarregada,
    pesos: &crate::pacote::Pesos,
    t: &crate::nota::Tentativa,
) -> Result<crate::nota::Resultado, crate::motor::ErroMotor> {
    use crate::motor::{Cenario, ErroMotor};
    use crate::nota::{Resultado, corrigir_previsoes};

    let missao = &m.missao;
    let md = missao
        .modelagem
        .as_ref()
        .ok_or_else(|| ErroMotor::Modelo("a missão não é de modelagem".into()))?;
    let gabarito = m
        .gabarito
        .as_ref()
        .ok_or_else(|| ErroMotor::Modelo("missão sem gabarito".into()))?;
    let reps = md.replicacoes.unwrap_or(5).max(2);
    let rodar = |modelo: &Model| -> Result<BTreeMap<String, Vec<f64>>, ErroMotor> {
        let mut x = modelo.clone();
        x.run.replications = reps;
        let r = x.run_replications()?;
        Ok(utsim_model::analysis::per_replication(&r))
    };
    let ref_metricas = rodar(gabarito)?;
    let base = Cenario {
        decisoes: Default::default(),
        custo: 0.0,
        metricas: ref_metricas.clone(),
    };

    let mut criterios = Vec::new();
    let aluno: Result<Model, String> = match &t.modelo {
        None => Err("nenhum modelo foi entregue".into()),
        Some(txt) => Model::parse(txt)
            .map_err(|e| e.to_string())
            .and_then(|mo| mo.validate().map(|_| mo).map_err(|e| e.to_string())),
    };
    criterios.push(ResultadoCriterio {
        rotulo: "O modelo carrega e é válido".into(),
        ok: aluno.is_ok(),
        detalhe: match &aluno {
            Ok(_) => "carrega sem erros".into(),
            Err(e) => e.clone(),
        },
        peso: 1.0,
    });
    let metricas_aluno = match &aluno {
        Ok(mo) => match rodar(mo) {
            Ok(x) => Some(x),
            Err(e) => {
                criterios.push(ResultadoCriterio {
                    rotulo: "O modelo roda".into(),
                    ok: false,
                    detalhe: e.to_string(),
                    peso: 1.0,
                });
                None
            }
        },
        Err(_) => None,
    };
    for c in &md.criterios {
        let (ok, detalhe) = match (c, &aluno) {
            (_, Err(_)) => (false, "o modelo não carregou".to_string()),
            (
                Criterio::Medida {
                    valor, tolerancia, ..
                },
                Ok(_),
            ) => match &metricas_aluno {
                None => (false, "o modelo não rodou".into()),
                Some(ma) => match (medida(valor, ma), medida(valor, &ref_metricas)) {
                    (Ok(a), Ok(r)) => {
                        let r = Intervalo::de(&r);
                        comparar(&Intervalo::de(&a), &r, *tolerancia, casas_para(r.media))
                    }
                    (Err(e), _) => (false, e),
                    (_, Err(e)) => (false, format!("gabarito: {e}")),
                },
            },
            (c, Ok(mo)) => estrutura(c, mo).expect("structural"),
        };
        criterios.push(ResultadoCriterio {
            rotulo: c.rotulo().to_string(),
            ok,
            detalhe,
            peso: c.peso(),
        });
    }
    let total: f64 = criterios.iter().map(|c| c.peso).sum();
    let feito: f64 = criterios.iter().filter(|c| c.ok).map(|c| c.peso).sum();
    let desempenho = if total > 0.0 { feito / total } else { 0.0 };
    let previsoes = corrigir_previsoes(missao, &base, Some(&m.modelo), t)?;
    let certas = previsoes.iter().filter(|p| p.certa).count();
    let justificativas_certas = missao
        .justificativa
        .iter()
        .enumerate()
        .filter(|(i, q)| t.justificativas.get(*i).copied().flatten() == Some(q.certa))
        .count();
    let pontos_desempenho = pesos.desempenho * desempenho;
    let pontos_previsao = if missao.previsao.is_empty() {
        0.0
    } else {
        pesos.previsao * certas as f64 / missao.previsao.len() as f64
    };
    let pontos_justificativa = if missao.justificativa.is_empty() {
        0.0
    } else {
        pesos.justificativa * justificativas_certas as f64 / missao.justificativa.len() as f64
    };
    let mut pontos_possiveis = pesos.desempenho;
    if !missao.previsao.is_empty() {
        pontos_possiveis += pesos.previsao;
    }
    if !missao.justificativa.is_empty() {
        pontos_possiveis += pesos.justificativa;
    }
    let desconto_dicas = match t.dicas.min(3) {
        0 => 0.0,
        n => pesos.dicas[n as usize - 1],
    };
    let bruta =
        100.0 * (pontos_desempenho + pontos_previsao + pontos_justificativa) / pontos_possiveis;
    let nota = ((bruta - desconto_dicas).clamp(0.0, 100.0) * 10.0).round() / 10.0;
    let resolvida = criterios.iter().all(|c| c.ok);
    let estrelas = if !resolvida {
        0
    } else if certas == missao.previsao.len() && justificativas_certas == missao.justificativa.len()
    {
        3
    } else {
        2
    };
    Ok(Resultado {
        portao: None,
        custo: 0.0,
        metricas: Vec::new(),
        previsoes,
        justificativas_certas,
        desempenho,
        pontos_desempenho,
        pontos_previsao,
        pontos_justificativa,
        pontos_possiveis,
        desconto_dicas,
        desconto_rodadas: 0.0,
        nota,
        resolvida,
        estrelas,
        selo: false,
        dicas_automaticas: Vec::new(),
        criterios,
    })
}
