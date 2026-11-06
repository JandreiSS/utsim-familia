//! The mission checker and the calibration (`docs/plano_ensino_e_jogo.md`, 6.2): the
//! exhaustive search of the reference over every lever combination, and the warnings of
//! a mission that teaches nothing (reference tied with the initial state), that is too
//! easy (more than 30 % of the combinations reach the goal) or impossible.

use serde::{Deserialize, Serialize};
use utsim_model::analysis::{is_significant, paired_difference};

use crate::missao::Decisoes;
use crate::motor::{Cenario, ErroMotor, avaliar, rodar};
use crate::nota::{Intervalo, fracao, portao, situacao};
use crate::pacote::MissaoCarregada;

/// Largest search space the exhaustive search accepts.
pub const MAX_COMBINACOES: usize = 400;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Conferencia {
    pub missao: String,
    pub combinacoes: usize,
    /// Combinations inside the budget and the limits.
    pub viaveis: usize,
    /// Feasible combinations that reach every goal.
    pub batem_meta: usize,
    /// The best combination found.
    pub melhor: Decisoes,
    /// Whether the mission's `referencia` is (or ties with) the best one.
    pub referencia_confere: bool,
    pub erros: Vec<String>,
    pub avisos: Vec<String>,
}

impl Conferencia {
    pub fn passou(&self) -> bool {
        self.erros.is_empty()
    }
}

struct Avaliada {
    decisoes: Decisoes,
    custo: f64,
    medias: Vec<f64>,
    bate: bool,
}

/// Runs every combination of `m` (without variant) and checks the mission.
pub fn conferir(m: &MissaoCarregada) -> Result<Conferencia, ErroMotor> {
    let missao = &m.missao;
    let todas = missao.combinacoes();
    let mut c = Conferencia {
        missao: missao.id.clone(),
        combinacoes: todas.len(),
        viaveis: 0,
        batem_meta: 0,
        melhor: Decisoes::new(),
        referencia_confere: false,
        erros: Vec::new(),
        avisos: Vec::new(),
    };
    if todas.len() > MAX_COMBINACOES {
        c.erros.push(format!(
            "{} combinações: mais que as {MAX_COMBINACOES} da busca exaustiva",
            todas.len()
        ));
        return Ok(c);
    }
    let viaveis: Vec<Decisoes> = todas
        .into_iter()
        .filter(|d| portao(missao, d).is_none())
        .collect();
    c.viaveis = viaveis.len();
    let mut lista = vec![missao.decisoes_iniciais()];
    lista.extend(viaveis.iter().cloned());
    let cenarios = rodar(m, "", &lista, &|_| true)?;
    let base = &cenarios[0];

    let medias = |cen: &Cenario| -> Result<Vec<f64>, ErroMotor> {
        missao
            .metricas
            .iter()
            .map(|mt| Ok(Intervalo::de(&avaliar(missao, &mt.valor, cen)?).media))
            .collect()
    };
    let bate = |cen: &Cenario| -> Result<bool, ErroMotor> {
        for mt in &missao.metricas {
            let v = Intervalo::de(&avaliar(missao, &mt.valor, cen)?);
            if !situacao(mt, &v).ok() {
                return Ok(false);
            }
        }
        Ok(true)
    };
    let base_medias = medias(base)?;
    if bate(base)? && missao.metricas.iter().any(|mt| mt.meta.is_some()) {
        c.erros
            .push("o estado inicial já bate todas as metas".into());
    }
    let mut avaliadas = Vec::new();
    for cen in &cenarios[1..] {
        avaliadas.push(Avaliada {
            decisoes: cen.decisoes.clone(),
            custo: cen.custo,
            medias: medias(cen)?,
            bate: bate(cen)?,
        });
    }
    c.batem_meta = avaliadas.iter().filter(|a| a.bate).count();

    // Best value of each measure among the feasible combinations.
    let melhores: Vec<f64> = missao
        .metricas
        .iter()
        .enumerate()
        .map(|(k, mt)| {
            avaliadas
                .iter()
                .map(|a| a.medias[k])
                .fold(base_medias[k], |acc, v| {
                    if mt.objetivo.melhor_ou_igual(v, acc) {
                        v
                    } else {
                        acc
                    }
                })
        })
        .collect();
    let nota = |a: &Avaliada| -> f64 {
        let (mut s, mut p) = (0.0, 0.0);
        for (k, mt) in missao.metricas.iter().enumerate() {
            let tol = mt.tolerancia.faixa(mt.meta.unwrap_or(melhores[k]));
            s += mt.peso * fracao(mt.objetivo, a.medias[k], base_medias[k], melhores[k], tol).0;
            p += mt.peso;
        }
        if p > 0.0 { s / p } else { 0.0 }
    };
    let mut melhor: Option<(&Avaliada, f64)> = None;
    for a in &avaliadas {
        let n = nota(a);
        let ganha = match melhor {
            None => true,
            Some((b, nb)) => {
                (a.bate && !b.bate)
                    || (a.bate == b.bate
                        && (n > nb + 1e-9 || ((n - nb).abs() <= 1e-9 && a.custo < b.custo - 1e-9)))
            }
        };
        if ganha {
            melhor = Some((a, n));
        }
    }
    let Some((melhor, nota_melhor)) = melhor else {
        c.erros
            .push("nenhuma combinação passa nas restrições".into());
        return Ok(c);
    };
    c.melhor = melhor.decisoes.clone();
    let referencia = missao.completar(&missao.referencia);
    c.referencia_confere = avaliadas
        .iter()
        .find(|a| a.decisoes == referencia)
        .is_some_and(|a| a.bate == melhor.bate && (nota(a) - nota_melhor).abs() <= 1e-9);
    if !c.referencia_confere {
        c.erros.push(format!(
            "a referência guardada não é a melhor combinação; a melhor é {}",
            descrever(m, &c.melhor)
        ));
    }
    if missao.metricas.iter().any(|mt| mt.meta.is_some()) && c.batem_meta == 0 {
        c.erros.push("nenhuma combinação bate as metas".into());
    }
    if c.viaveis > 1 && c.batem_meta as f64 > 0.3 * c.viaveis as f64 {
        c.avisos.push(format!(
            "{} de {} combinações batem as metas (mais de 30 %): missão fácil",
            c.batem_meta, c.viaveis
        ));
    }
    // The reference must differ from the initial state in at least one measure.
    let ref_cen = cenarios[1..]
        .iter()
        .find(|x| x.decisoes == melhor.decisoes)
        .expect("evaluated");
    let mut algum = false;
    for mt in &missao.metricas {
        let a = avaliar(missao, &mt.valor, ref_cen)?;
        let b = avaliar(missao, &mt.valor, base)?;
        if is_significant(&paired_difference(&a, &b)) {
            algum = true;
        }
    }
    if !algum && !missao.alavancas.is_empty() {
        c.erros
            .push("a referência e o estado inicial empatam estatisticamente".into());
    }
    Ok(c)
}

/// `decisoes` as text (`Máquinas na furação = 2, ...`).
pub fn descrever(m: &MissaoCarregada, decisoes: &Decisoes) -> String {
    m.missao
        .alavancas
        .iter()
        .filter_map(|a| {
            decisoes
                .get(a.id())
                .map(|v| format!("{} = {}", a.rotulo(), a.descrever(*v)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The stored numbers of a mission: the means of each measure in the initial state and
/// in the reference (no variant). A change in the engine that moves them fails the test.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Golden {
    pub missao: String,
    pub referencia: Decisoes,
    pub base: Vec<(String, f64)>,
    pub ref_valores: Vec<(String, f64)>,
}

pub fn golden(m: &MissaoCarregada) -> Result<Golden, ErroMotor> {
    let missao = &m.missao;
    let sim = crate::motor::simular(m, "", &missao.referencia, &|_| true)?;
    let mut base = Vec::new();
    let mut ref_valores = Vec::new();
    for mt in &missao.metricas {
        base.push((
            mt.nome.clone(),
            Intervalo::de(&avaliar(missao, &mt.valor, &sim.base)?).media,
        ));
        ref_valores.push((
            mt.nome.clone(),
            Intervalo::de(&avaliar(missao, &mt.valor, &sim.referencia)?).media,
        ));
    }
    Ok(Golden {
        missao: missao.id.clone(),
        referencia: missao.completar(&missao.referencia),
        base,
        ref_valores,
    })
}
