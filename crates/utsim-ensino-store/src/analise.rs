//! "Analisar no UTSim Estatística / Qualidade": the data of a mission are written as CSV
//! files (the student's own data, from the variant seed) and the installed app of the
//! family is opened with `--utsim-abrir=<arquivo.csv>`, one per data set. The student
//! analyses there and types the answers back in the game, which grades them from the same
//! data. An app that does not read the files yet just opens, and the files stay in the
//! profile for its import screen.
//!
//! UTSim Qualidade also gets the tool and the specification limits of the mission
//! (`--utsim-ferramenta`, `--utsim-lie`, `--utsim-lse`, from the first data set that names
//! them), one study with one tool per file (contract of section 2.2 of
//! `docs/plano_integracao_familia.md`).

use std::path::{Path, PathBuf};

use utsim_ensino_core::pacote::MissaoCarregada;

use crate::pasta::slug;
use crate::{Erro, Result};

/// The installed app `produto` (launcher id: `estatistica`, `qualidade`): program, default
/// arguments and folder, when the launcher has it.
pub fn produto(produto: &str) -> Option<(PathBuf, Vec<String>, PathBuf)> {
    use utsim_launcher_core::install::{Layout, launch_command};
    let root = Layout::default_root()?;
    launch_command(&Layout::new(root), produto).ok()
}

/// The apps of [`utsim_ensino_core::missao::APPS_DE_ANALISE`] installed on this computer.
pub fn instalados() -> Vec<String> {
    utsim_ensino_core::missao::APPS_DE_ANALISE
        .iter()
        .filter(|p| produto(p).is_some())
        .map(|p| p.to_string())
        .collect()
}

/// Writes the data sets of `m` for `aluno` as `<dir>/<missao>/<conjunto>.csv` and returns
/// the paths, in the order of the mission.
pub fn gravar_dados(dir: &Path, m: &MissaoCarregada, aluno: &str) -> Result<Vec<PathBuf>> {
    let pasta = dir.join(slug(&m.missao.id));
    std::fs::create_dir_all(&pasta)?;
    let mut out = Vec::new();
    for cj in &m.missao.dados {
        let p = pasta.join(format!("{}.csv", slug(&cj.id)));
        std::fs::write(&p, cj.gerar(&m.missao.id, aluno).csv())?;
        out.push(p);
    }
    Ok(out)
}

/// The arguments for the app: its default ones, then `--utsim-abrir=<arquivo>` per file.
pub fn argumentos(padrao: &[String], arquivos: &[PathBuf]) -> Vec<String> {
    let mut a = padrao.to_vec();
    a.extend(
        arquivos
            .iter()
            .map(|p| format!("--utsim-abrir={}", p.display())),
    );
    a
}

/// The options of the mission for the app `produto`: for `qualidade`, the tool and the
/// limits of the first data set that names them (they apply to every file).
pub fn opcoes(produto: &str, m: &MissaoCarregada) -> Vec<String> {
    if produto != "qualidade" {
        return Vec::new();
    }
    let mut a = Vec::new();
    if let Some(f) = m.missao.dados.iter().find_map(|c| c.ferramenta.as_ref()) {
        a.push(format!("--utsim-ferramenta={f}"));
    }
    if let Some(v) = m.missao.dados.iter().find_map(|c| c.lie) {
        a.push(format!("--utsim-lie={v}"));
    }
    if let Some(v) = m.missao.dados.iter().find_map(|c| c.lse) {
        a.push(format!("--utsim-lse={v}"));
    }
    a
}

/// Writes the data of `m` in `dir` and opens them in the installed app `produto`. The paths
/// of the files.
pub fn analisar(
    produto_id: &str,
    dir: &Path,
    m: &MissaoCarregada,
    aluno: &str,
) -> Result<Vec<PathBuf>> {
    let (exe, padrao, pasta) = produto(produto_id).ok_or_else(|| {
        Erro::Invalido(format!(
            "o app '{produto_id}' não está instalado pelo launcher neste computador"
        ))
    })?;
    let arquivos = gravar_dados(dir, m, aluno)?;
    std::process::Command::new(&exe)
        .args(argumentos(&padrao, &arquivos))
        .args(opcoes(produto_id, m))
        .current_dir(pasta)
        .spawn()?;
    Ok(arquivos)
}
