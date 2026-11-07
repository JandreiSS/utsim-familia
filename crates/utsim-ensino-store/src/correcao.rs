//! Batch grading on the teacher's machine: every delivery checked (signature, hash,
//! chain), every attempt simulated again with the student's seed and graded. The grade
//! the student's app showed is never used.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rayon::prelude::*;
use utsim_ensino_core::VERSAO_NUCLEO;
use utsim_ensino_core::missao::Decisoes;
use utsim_ensino_core::motor::{Simulacao, rodar};
use utsim_ensino_core::nota::{Resultado, Tentativa, corrigir};
use utsim_ensino_core::pacote::{PacoteCarregado, Registro};
use utsim_ensino_core::turma::{Aluno, Celula, ErroComum, LinhaMatriz, Matriz, erros_comuns};

use crate::arquivos::Entrega;
use crate::diario::{RegistroTentativa, verificar_diario};

/// The graded class.
#[derive(Clone, Debug, Default)]
pub struct Correcao {
    pub matriz: Matriz,
    /// Per mission id.
    pub erros_comuns: BTreeMap<String, Vec<ErroComum>>,
    /// Per student record and mission id, every graded attempt in order.
    pub resultados: BTreeMap<(String, String), Vec<(Tentativa, Resultado)>>,
    pub avisos: Vec<String>,
    pub cancelada: bool,
}

/// A graded (student, mission) pair, or why it could not be graded.
type Saida = (String, String, Result<Vec<(Tentativa, Resultado)>, String>);

struct Trabalho {
    registro: String,
    missao: String,
    tentativas: Vec<Tentativa>,
}

/// Grades every delivery of `alunos` (students not on the list are graded too, with a
/// warning). `progresso(feito, total)` counts (student, mission) pairs.
pub fn corrigir_turma(
    pacote: &PacoteCarregado,
    registro: Registro,
    alunos: &[Aluno],
    entregas: &[Entrega],
    progresso: &(dyn Fn(usize, usize) + Sync),
    cancelar: &AtomicBool,
) -> Correcao {
    let mut c = Correcao::default();
    let mut avisos_aluno: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut lista: Vec<Aluno> = alunos.to_vec();

    // Merge the deliveries of each student (folder and files), by record.
    let mut por_aluno: BTreeMap<String, (String, Vec<crate::diario::TentativaAssinada>)> =
        BTreeMap::new();
    for e in entregas {
        let reg = e.aluno.registro.clone();
        if !lista.iter().any(|a| a.registro == reg) {
            avisos_aluno
                .entry(reg.clone())
                .or_default()
                .push("aluno fora da lista da turma".into());
            lista.push(e.aluno.clone());
        }
        let ent = por_aluno
            .entry(reg.clone())
            .or_insert_with(|| (e.chave_publica.clone(), Vec::new()));
        if ent.0 != e.chave_publica {
            avisos_aluno
                .entry(reg.clone())
                .or_default()
                .push("entregas com chaves diferentes: só a primeira chave vale".into());
            continue;
        }
        for t in &e.tentativas {
            if !ent.1.iter().any(|x| x.hash == t.hash) {
                ent.1.push(t.clone());
            }
        }
    }

    let mut trabalhos = Vec::new();
    for (reg, (chave, tentativas)) in &por_aluno {
        let v = verificar_diario(chave, reg, tentativas);
        let av = avisos_aluno.entry(reg.clone()).or_default();
        av.extend(v.avisos);
        let deste: Vec<&RegistroTentativa> = v
            .validas
            .iter()
            .filter(|r| r.pacote == pacote.pacote.id)
            .collect();
        if deste.iter().any(|r| r.pacote_hash != pacote.hash) {
            av.push("tentativas feitas com outra versão do pacote".into());
        }
        if deste.iter().any(|r| r.versao_nucleo != VERSAO_NUCLEO) {
            av.push(format!(
                "tentativas feitas com outra versão do núcleo; ressimuladas na versão {VERSAO_NUCLEO}"
            ));
        }
        for m in &pacote.missoes {
            let ts: Vec<Tentativa> = deste
                .iter()
                .filter(|r| r.missao == m.missao.id)
                .map(|r| r.tentativa.clone())
                .collect();
            if !ts.is_empty() {
                trabalhos.push(Trabalho {
                    registro: reg.clone(),
                    missao: m.missao.id.clone(),
                    tentativas: ts,
                });
            }
        }
    }

    let total = trabalhos.len();
    let feitos = AtomicUsize::new(0);
    progresso(0, total);
    let saidas: Vec<Saida> = trabalhos
        .par_iter()
        .map(|t| {
            if cancelar.load(Ordering::Relaxed) {
                return (
                    t.registro.clone(),
                    t.missao.clone(),
                    Err("cancelada".into()),
                );
            }
            let r = corrigir_trabalho(pacote, t);
            let n = feitos.fetch_add(1, Ordering::Relaxed) + 1;
            progresso(n, total);
            (t.registro.clone(), t.missao.clone(), r)
        })
        .collect();
    c.cancelada = cancelar.load(Ordering::Relaxed);
    for (reg, missao, r) in saidas {
        match r {
            Ok(v) => {
                c.resultados.insert((reg, missao), v);
            }
            Err(e) => avisos_aluno
                .entry(reg)
                .or_default()
                .push(format!("{missao}: não foi possível corrigir ({e})")),
        }
    }

    c.matriz.missoes = pacote.missoes.iter().map(|m| m.missao.id.clone()).collect();
    c.matriz.titulos = pacote
        .missoes
        .iter()
        .map(|m| m.missao.titulo.clone())
        .collect();
    lista.sort_by(|a, b| a.nome.cmp(&b.nome).then(a.registro.cmp(&b.registro)));
    for a in &lista {
        let celulas = c
            .matriz
            .missoes
            .iter()
            .map(|m| {
                c.resultados
                    .get(&(a.registro.clone(), m.clone()))
                    .and_then(|v| Celula::de(v, registro))
            })
            .collect();
        c.matriz.linhas.push(LinhaMatriz {
            aluno: a.clone(),
            celulas,
            avisos: avisos_aluno.remove(&a.registro).unwrap_or_default(),
        });
    }
    for m in &pacote.missoes {
        let por: Vec<Vec<(Tentativa, Resultado)>> = c
            .resultados
            .iter()
            .filter(|((_, mid), _)| mid == &m.missao.id)
            .map(|(_, v)| v.clone())
            .collect();
        if !por.is_empty() {
            c.erros_comuns
                .insert(m.missao.id.clone(), erros_comuns(&m.missao, &por));
        }
    }
    c
}

/// One student in one mission: the initial state and the reference once, then each
/// distinct set of decisions once.
fn corrigir_trabalho(
    pacote: &PacoteCarregado,
    t: &Trabalho,
) -> Result<Vec<(Tentativa, Resultado)>, String> {
    let m = pacote.missao(&t.missao).ok_or("missão não encontrada")?;
    if m.missao.modelagem.is_some() {
        return t
            .tentativas
            .iter()
            .map(|tent| {
                utsim_ensino_core::modelagem::corrigir_modelagem(m, &pacote.pacote.pesos, tent)
                    .map(|r| (tent.clone(), r))
                    .map_err(|e| e.to_string())
            })
            .collect();
    }
    let mut lista: Vec<Decisoes> = vec![
        m.missao.decisoes_iniciais(),
        m.missao.completar(&m.missao.referencia),
    ];
    let mut indice = Vec::new();
    for tent in &t.tentativas {
        let d = m.missao.completar(&tent.decisoes);
        let i = match lista.iter().skip(2).position(|x| *x == d) {
            Some(i) => i + 2,
            None => {
                lista.push(d);
                lista.len() - 1
            }
        };
        indice.push(i);
    }
    let cenarios = rodar(m, &t.registro, &lista, &|_| true).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (tent, i) in t.tentativas.iter().zip(indice) {
        let mut c = cenarios[i].clone();
        // The raw decisions decide the gate (an out-of-range value must fail it).
        c.decisoes = m.missao.completar(&tent.decisoes);
        let sim = Simulacao {
            base: cenarios[0].clone(),
            aluno: c,
            referencia: cenarios[1].clone(),
        };
        let r = corrigir(&m.missao, &pacote.pacote.pesos, &sim, tent).map_err(|e| e.to_string())?;
        out.push((tent.clone(), r));
    }
    Ok(out)
}
