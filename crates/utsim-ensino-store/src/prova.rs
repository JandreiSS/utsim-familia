//! Exams on disk (`docs/plano_ensino_e_jogo.md`, 4.5): the student's package goes to the
//! class folder (`pacotes/<id>.utse`), and what rebuilds the full exam (base package, rules
//! and seed) stays in the teacher's profile (`provas/<id>.ron`), out of the shared folder.

use std::path::{Path, PathBuf};

use utsim_ensino_core::pacote::{Arquivos, PacoteCarregado};
use utsim_ensino_core::prova::{Montagem, montar, pacote_do_professor};
use utsim_ensino_core::turma::PacotePublicado;

use crate::pasta::PastaTurma;
use crate::perfil::Perfil;
use crate::{Erro, Result};

fn pasta_provas(perfil: &Perfil) -> PathBuf {
    perfil.dir.join("provas")
}

/// Draws the exam, publishes the student's package in the class folder (listed in
/// `turma.ron` with the end of the window as deadline) and keeps the assembly in the
/// teacher's profile. Returns the path of the published package.
pub fn publicar_prova(
    turma: &PastaTurma,
    perfil: &Perfil,
    base_arquivos: &Arquivos,
    base: &PacoteCarregado,
    m: &Montagem,
) -> Result<PathBuf> {
    let pm = montar(base_arquivos, base, m).map_err(Erro::Invalido)?;
    let dir = pasta_provas(perfil);
    std::fs::create_dir_all(&dir)?;
    let txt = ron::ser::to_string_pretty(m, ron::ser::PrettyConfig::default())
        .map_err(|e| Erro::Invalido(e.to_string()))?;
    std::fs::write(dir.join(format!("{}.ron", m.id)), txt)?;
    let path = turma.publicar_pacote(&m.id, &pm.aluno)?;
    let mut t = turma.turma()?;
    let arquivo = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    t.pacotes.retain(|p| p.arquivo != arquivo);
    t.pacotes.push(PacotePublicado {
        arquivo,
        prazo: Some(utsim_ensino_core::prova::data_hora(
            m.prova.fim,
            m.prova.fuso_min,
        )),
    });
    turma.salvar_turma(&t)?;
    Ok(path)
}

/// The exams this teacher assembled, by id.
pub fn provas_salvas(perfil: &Perfil) -> Vec<Montagem> {
    let Ok(d) = std::fs::read_dir(pasta_provas(perfil)) else {
        return Vec::new();
    };
    let mut v: Vec<Montagem> = d
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| ron::from_str(&t).ok())
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Rebuilds the teacher's package of an exam from the seed, refusing when the base
/// package changed since the assembly (the draw or the missions would differ).
pub fn pacote_da_prova(
    base_arquivos: &Arquivos,
    base: &PacoteCarregado,
    m: &Montagem,
) -> std::result::Result<PacoteCarregado, String> {
    if base.pacote.id != m.pacote_base || base.hash != m.hash_base {
        return Err(format!(
            "a prova foi montada com {} ({}); o pacote aberto é {} ({})",
            m.pacote_base,
            &m.hash_base[..m.hash_base.len().min(12)],
            base.pacote.id,
            &base.hash[..base.hash.len().min(12)]
        ));
    }
    let pm = montar(base_arquivos, base, m)?;
    pacote_do_professor(&pm).map_err(|e| e.join("; "))
}

/// Reads one saved assembly.
pub fn ler_montagem(path: &Path) -> Result<Montagem> {
    let t = std::fs::read_to_string(path)?;
    ron::from_str(&t).map_err(|e| Erro::Invalido(e.to_string()))
}
