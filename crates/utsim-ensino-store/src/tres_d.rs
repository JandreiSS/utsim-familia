//! "Ver em 3D" (`docs/plano_ensino_e_jogo.md`, 5.4 and 7.4): the model of the attempt and
//! its replay are written to a folder and the UTSim client installed by the launcher opens
//! them. The 3D client (`utsim-3d`, Godot) gets `--utsim-abrir=<replay.json>` after `--`;
//! the older viewer (`utsim-view`) gets the model path.
//!
//! "Levar para o UTSim" (7.4) writes only the model, in a folder that stays
//! (`modelos/` of the profile), and the client opens it to edit: the student goes on from
//! the attempt in the full simulator.

use std::path::{Path, PathBuf};

use utsim_ensino_core::missao::Decisoes;
use utsim_ensino_core::motor::{modelo_da_tentativa, replay};
use utsim_ensino_core::pacote::MissaoCarregada;

use crate::pasta::slug;
use crate::{Erro, Result};

/// The launcher product of the UTSim client.
pub const PRODUTO: &str = "utsim";

/// Writes `<missao>.ron` (the model with the student's variant and decisions) and
/// `<missao>.replay.json` (one replication, as `utsim run --replay` writes it) in `dir`.
pub fn gravar_tentativa(
    dir: &Path,
    m: &MissaoCarregada,
    aluno: &str,
    decisoes: &Decisoes,
) -> Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let nome = slug(&m.missao.id);
    let modelo =
        modelo_da_tentativa(m, aluno, decisoes).map_err(|e| Erro::Invalido(e.to_string()))?;
    let r = replay(m, aluno, decisoes).map_err(|e| Erro::Invalido(e.to_string()))?;
    let pm = dir.join(format!("{nome}.ron"));
    let pr = dir.join(format!("{nome}.replay.json"));
    let txt = ron::ser::to_string_pretty(
        &modelo,
        ron::ser::PrettyConfig::default().struct_names(true),
    )
    .map_err(|e| Erro::Invalido(e.to_string()))?;
    std::fs::write(&pm, txt)?;
    std::fs::write(
        &pr,
        serde_json::to_string(&r).map_err(|e| Erro::Invalido(e.to_string()))?,
    )?;
    Ok((pm, pr))
}

/// The arguments for the installed client: the 3D client opens the replay, the viewer
/// opens the model.
pub fn argumentos(exe: &Path, padrao: &[String], modelo: &Path, replay: &Path) -> Vec<String> {
    let nome = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if nome.starts_with("utsim-3d") {
        let mut a = padrao.to_vec();
        if !a.iter().any(|x| x == "--") {
            a.push("--".into());
        }
        a.push(format!("--utsim-abrir={}", replay.display()));
        a
    } else {
        vec![modelo.display().to_string()]
    }
}

/// The arguments to open a model (`.ron`) to edit: the 3D client gets
/// `--utsim-abrir=<modelo.ron>`, the viewer the path.
pub fn argumentos_modelo(exe: &Path, padrao: &[String], modelo: &Path) -> Vec<String> {
    argumentos(exe, padrao, modelo, modelo)
}

/// Writes the model of the attempt (`<missao>.ron`, with the student's variant and
/// decisions) in `dir`, without the replay.
pub fn gravar_modelo(
    dir: &Path,
    m: &MissaoCarregada,
    aluno: &str,
    decisoes: &Decisoes,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let modelo =
        modelo_da_tentativa(m, aluno, decisoes).map_err(|e| Erro::Invalido(e.to_string()))?;
    let txt = ron::ser::to_string_pretty(
        &modelo,
        ron::ser::PrettyConfig::default().struct_names(true),
    )
    .map_err(|e| Erro::Invalido(e.to_string()))?;
    let pm = dir.join(format!("{}.ron", slug(&m.missao.id)));
    std::fs::write(&pm, txt)?;
    Ok(pm)
}

/// "Levar para o UTSim": writes the model of the attempt in `dir` and opens it in the
/// installed client. The path of the model.
pub fn levar_para_utsim(
    dir: &Path,
    m: &MissaoCarregada,
    aluno: &str,
    decisoes: &Decisoes,
) -> Result<PathBuf> {
    let (exe, padrao, pasta) = cliente().ok_or_else(|| {
        Erro::Invalido("o UTSim não está instalado pelo launcher neste computador".into())
    })?;
    let pm = gravar_modelo(dir, m, aluno, decisoes)?;
    std::process::Command::new(&exe)
        .args(argumentos_modelo(&exe, &padrao, &pm))
        .current_dir(pasta)
        .spawn()?;
    Ok(pm)
}

/// The installed client (program, default arguments, folder), when the launcher has it.
pub fn cliente() -> Option<(PathBuf, Vec<String>, PathBuf)> {
    use utsim_launcher_core::install::{Layout, launch_command};
    let root = Layout::default_root()?;
    launch_command(&Layout::new(root), PRODUTO).ok()
}

/// Writes the attempt and opens it in the installed client.
pub fn abrir_em_3d(
    dir: &Path,
    m: &MissaoCarregada,
    aluno: &str,
    decisoes: &Decisoes,
) -> Result<PathBuf> {
    let (exe, padrao, pasta) = cliente().ok_or_else(|| {
        Erro::Invalido("o UTSim não está instalado pelo launcher neste computador".into())
    })?;
    let (pm, pr) = gravar_tentativa(dir, m, aluno, decisoes)?;
    std::process::Command::new(&exe)
        .args(argumentos(&exe, &padrao, &pm, &pr))
        .current_dir(pasta)
        .spawn()?;
    Ok(pr)
}
