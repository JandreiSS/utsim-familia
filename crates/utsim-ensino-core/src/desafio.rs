//! Class challenges and teams (`docs/plano_ensino_e_jogo.md`, 2.4): the scoreboard of a
//! mission by nickname, ordered by grade and then cost, where two results whose intervals
//! overlap appear tied (the noise is not rewarded); and the teams of a large mission,
//! compared by paired differences with common random numbers.

use serde::{Deserialize, Serialize};

use crate::missao::{Decisoes, Objetivo};
use crate::motor::{ErroMotor, rodar, valor_metrica};
use crate::nota::{Intervalo, Resultado};
use crate::pacote::MissaoCarregada;

/// A challenge of the class: one mission, an optional deadline.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Desafio {
    pub missao: String,
    /// Milliseconds since 1970: attempts after it do not count.
    #[serde(default)]
    pub fim: Option<i64>,
}

/// A team: a name and the records of its members.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Equipe {
    pub nome: String,
    pub membros: Vec<String>,
}

/// One student in the scoreboard.
#[derive(Clone, Debug, PartialEq)]
pub struct Entrada {
    pub apelido: String,
    pub resultado: Resultado,
}

/// One line of the scoreboard: position 1, 1, 3... with ties marked.
#[derive(Clone, Debug, PartialEq)]
pub struct Linha {
    pub posicao: usize,
    pub empate: bool,
    pub apelido: String,
    pub nota: f64,
    pub custo: f64,
}

/// Points that do not come from the simulation (prediction, justification, discounts).
fn fora_da_simulacao(r: &Resultado) -> f64 {
    r.pontos_previsao + r.pontos_justificativa - r.desconto_dicas - r.desconto_rodadas
}

/// Two results are a statistical tie when they differ only in what the simulation
/// measured and every measured interval overlaps.
pub fn empatados(a: &Resultado, b: &Resultado) -> bool {
    if a.portao.is_some() != b.portao.is_some()
        || (fora_da_simulacao(a) - fora_da_simulacao(b)).abs() > 1e-9
        || a.metricas.len() != b.metricas.len()
    {
        return false;
    }
    a.metricas
        .iter()
        .zip(&b.metricas)
        .all(|(x, y)| x.aluno.baixo() <= y.aluno.alto() && y.aluno.baixo() <= x.aluno.alto())
}

/// The scoreboard: by grade, then by cost. A student tied with the first of the group
/// above takes the same position.
pub fn placar(mut entradas: Vec<Entrada>) -> Vec<Linha> {
    entradas.sort_by(|a, b| {
        b.resultado
            .nota
            .total_cmp(&a.resultado.nota)
            .then(a.resultado.custo.total_cmp(&b.resultado.custo))
            .then(a.apelido.cmp(&b.apelido))
    });
    let mut out: Vec<Linha> = Vec::with_capacity(entradas.len());
    let mut lider = 0usize;
    for (i, e) in entradas.iter().enumerate() {
        let empata = i > 0 && empatados(&entradas[lider].resultado, &e.resultado);
        if !empata {
            lider = i;
        }
        out.push(Linha {
            posicao: lider + 1,
            empate: empata,
            apelido: e.apelido.clone(),
            nota: e.resultado.nota,
            custo: e.resultado.custo,
        });
        if empata {
            out[lider].empate = true;
        }
    }
    out
}

/// The teams of a mission compared on its main measure.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparacaoEquipes {
    pub equipes: Vec<String>,
    pub metrica: String,
    pub unidade: String,
    pub objetivo: Objetivo,
    /// Mean and interval of each team.
    pub medias: Vec<Intervalo>,
    /// `diferencas[i][j]`: team i minus team j, paired by replication.
    pub diferencas: Vec<Vec<Intervalo>>,
    /// 1 + the number of teams significantly better (ties share the position).
    pub posicoes: Vec<usize>,
}

/// Variant seed shared by every team: all of them face the same factory.
pub const VARIANTE_EQUIPES: &str = "equipes";

/// Runs the solution of each team on the same factory, with the same random numbers, and
/// compares them on the measure with the largest weight.
pub fn comparar_equipes(
    m: &MissaoCarregada,
    solucoes: &[(String, Decisoes)],
    progresso: &(dyn Fn(f32) -> bool + Sync),
) -> Result<ComparacaoEquipes, ErroMotor> {
    let metrica = m
        .missao
        .metricas
        .iter()
        .max_by(|a, b| a.peso.total_cmp(&b.peso))
        .ok_or_else(|| ErroMotor::Modelo("a missão não tem medidas".into()))?;
    let lista: Vec<Decisoes> = solucoes.iter().map(|(_, d)| d.clone()).collect();
    let cenarios = rodar(m, VARIANTE_EQUIPES, &lista, progresso)?;
    let valores: Vec<Vec<f64>> = cenarios
        .iter()
        .map(|c| valor_metrica(&m.missao, metrica, c))
        .collect::<Result<_, _>>()?;
    let medias: Vec<Intervalo> = valores.iter().map(|v| Intervalo::de(v)).collect();
    let n = valores.len();
    let mut diferencas = vec![Vec::with_capacity(n); n];
    for (i, linha) in diferencas.iter_mut().enumerate() {
        for vj in &valores {
            let d: Vec<f64> = valores[i].iter().zip(vj).map(|(a, b)| a - b).collect();
            linha.push(Intervalo::de(&d));
        }
    }
    let melhor = |d: &Intervalo| match metrica.objetivo {
        Objetivo::Menor => d.alto() < 0.0,
        Objetivo::Maior => d.baixo() > 0.0,
    };
    let posicoes = (0..n)
        .map(|i| {
            1 + (0..n)
                .filter(|&j| j != i && melhor(&diferencas[j][i]))
                .count()
        })
        .collect();
    Ok(ComparacaoEquipes {
        equipes: solucoes.iter().map(|(n, _)| n.clone()).collect(),
        metrica: metrica.rotulo.clone(),
        unidade: metrica.unidade.clone(),
        objetivo: metrica.objetivo,
        medias,
        diferencas,
        posicoes,
    })
}

/// Reads teams from text, one per line: `Name: record, record`.
pub fn ler_equipes(texto: &str) -> Result<Vec<Equipe>, String> {
    let mut v = Vec::new();
    for (i, l) in texto.lines().enumerate() {
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        let (nome, membros) = l
            .split_once(':')
            .ok_or_else(|| format!("linha {}: use 'Equipe: registro, registro'", i + 1))?;
        let membros: Vec<String> = membros
            .split([',', ';'])
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect();
        if nome.trim().is_empty() || membros.is_empty() {
            return Err(format!("linha {}: equipe sem nome ou sem membros", i + 1));
        }
        v.push(Equipe {
            nome: nome.trim().to_string(),
            membros,
        });
    }
    Ok(v)
}
