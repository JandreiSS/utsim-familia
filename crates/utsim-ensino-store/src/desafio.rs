//! Challenges and teams on the teacher's side (`docs/plano_ensino_e_jogo.md`, 2.4): the
//! scoreboard of a mission from a graded class, written to `placares/` in the class folder
//! with nicknames only, and the solution of each team for the paired comparison.

use std::path::PathBuf;

use utsim_ensino_core::desafio::{Desafio, Entrada, Equipe, Linha, placar};
use utsim_ensino_core::missao::Decisoes;
use utsim_ensino_core::nota::{Resultado, Tentativa};
use utsim_ensino_core::pacote::Registro;
use utsim_ensino_core::turma::Aluno;

use crate::Result;
use crate::correcao::Correcao;
use crate::pasta::{PastaTurma, slug};

/// The attempt that counts for `registro` (the best or the last) among `v`.
fn conta(v: &[(Tentativa, Resultado)], registro: Registro) -> Option<&(Tentativa, Resultado)> {
    match registro {
        Registro::Ultima => v.last(),
        Registro::Melhor => v.iter().max_by(|a, b| a.1.nota.total_cmp(&b.1.nota)),
    }
}

/// The scoreboard of a challenge: for each student, the attempt that counts among those
/// made until the deadline.
pub fn placar_do_desafio(
    c: &Correcao,
    alunos: &[Aluno],
    d: &Desafio,
    registro: Registro,
) -> Vec<Linha> {
    let mut entradas = Vec::new();
    for a in alunos {
        let k = (a.registro.clone(), d.missao.clone());
        let Some(v) = c.resultados.get(&k) else {
            continue;
        };
        let quando = c.instantes.get(&k);
        let dentro: Vec<(Tentativa, Resultado)> = v
            .iter()
            .enumerate()
            .filter(|(i, _)| match (d.fim, quando) {
                (Some(f), Some(q)) => q.get(*i).is_some_and(|t| *t <= f),
                _ => true,
            })
            .map(|(_, x)| x.clone())
            .collect();
        if let Some((_, r)) = conta(&dentro, registro) {
            entradas.push(Entrada {
                apelido: if a.apelido.trim().is_empty() {
                    a.registro.clone()
                } else {
                    a.apelido.clone()
                },
                resultado: r.clone(),
            });
        }
    }
    placar(entradas)
}

/// The scoreboard as CSV, with nicknames only (never the name or the record).
pub fn placar_csv(linhas: &[Linha]) -> String {
    let mut s = String::from("posicao;apelido;nota;custo;empate\n");
    for l in linhas {
        s.push_str(&format!(
            "{};{};{:.1};{:.0};{}\n",
            l.posicao,
            l.apelido.replace(';', ","),
            l.nota,
            l.custo,
            if l.empate { "sim" } else { "" }
        ));
    }
    s
}

/// Writes the scoreboard to `placares/<missao>.csv` in the class folder, for the students.
pub fn gravar_placar(turma: &PastaTurma, missao: &str, linhas: &[Linha]) -> Result<PathBuf> {
    let dir = turma.dir.join("placares");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.csv", slug(missao)));
    std::fs::write(&path, placar_csv(linhas))?;
    Ok(path)
}

/// The scoreboards published in a class folder: (mission file name, CSV text).
pub fn placares(turma: &std::path::Path) -> Vec<(String, String)> {
    let Ok(d) = std::fs::read_dir(turma.join("placares")) else {
        return Vec::new();
    };
    let mut v: Vec<(String, String)> = d
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "csv"))
        .filter_map(|e| {
            let nome = e.path().file_stem()?.to_string_lossy().into_owned();
            Some((nome, std::fs::read_to_string(e.path()).ok()?))
        })
        .collect();
    v.sort();
    v
}

/// The solution of each team in a mission: the attempt that counts among those of its
/// members. Teams without attempts are left out (and listed in the second value).
pub fn solucoes_das_equipes(
    c: &Correcao,
    equipes: &[Equipe],
    missao: &str,
    registro: Registro,
) -> (Vec<(String, Decisoes)>, Vec<String>) {
    let mut ok = Vec::new();
    let mut sem = Vec::new();
    for e in equipes {
        let todas: Vec<(Tentativa, Resultado)> = e
            .membros
            .iter()
            .filter_map(|m| c.resultados.get(&(m.clone(), missao.to_string())))
            .flatten()
            .cloned()
            .collect();
        match conta(&todas, registro) {
            Some((t, _)) => ok.push((e.nome.clone(), t.decisoes.clone())),
            None => sem.push(e.nome.clone()),
        }
    }
    (ok, sem)
}
