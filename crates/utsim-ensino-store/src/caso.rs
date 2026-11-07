//! The case builder on disk: a draft package folder from a UTSim model, the check of a
//! package folder or `.utse`, and the folder zipped as `.utse`. Used by the teacher's
//! screen and by `utsim-ensino caso | conferir | pacote`.

use std::path::{Path, PathBuf};

use utsim_ensino_core::conferir::conferir;
use utsim_ensino_core::construtor::{
    calibrar, pacote, rascunho, resumo, sugerir_alavancas, sugerir_metricas, texto_inicial,
};
use utsim_ensino_core::pacote::{Arquivos, carregar};
use utsim_model::Model;

use crate::arquivos::{gravar_utse, ler_pacote};
use crate::{Erro, Result};

/// Writes every file of `arquivos` under `dir`.
pub fn gravar_pasta(arquivos: &Arquivos, dir: &Path) -> Result<()> {
    for (k, v) in arquivos {
        let p = k
            .split('/')
            .fold(dir.to_path_buf(), |acc, parte| acc.join(parte));
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, v)?;
    }
    Ok(())
}

fn slug(t: &str) -> String {
    let s: String = t
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_ascii_alphanumeric() => c,
            _ => '_',
        })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "missao".into() } else { s }
}

/// A draft mission from the model at `modelo`: suggested levers and measures, the
/// reference and goals by the calibration, written to `saida` with the check in
/// `conferencia.txt`. Returns the text of the check.
pub fn criar_caso(modelo: &Path, saida: &Path, titulo: &str) -> Result<String> {
    let txt = std::fs::read_to_string(modelo)?;
    let m = Model::parse(&txt).map_err(|e| Erro::Invalido(format!("{}: {e}", modelo.display())))?;
    let alavancas = sugerir_alavancas(&m);
    if alavancas.is_empty() {
        return Err(Erro::Invalido(
            "o modelo não tem capacidades, filas limitadas nem equipes para virar alavancas".into(),
        ));
    }
    let metricas = sugerir_metricas(&m);
    let id = format!("professor/{}", slug(titulo));
    let missao = rascunho(&id, titulo, &m, alavancas, metricas);
    let texto = texto_inicial(titulo, &m, &missao.alavancas);
    let arquivos = pacote(titulo, &missao, &txt, &texto);
    let (calibrado, c) = calibrar(&arquivos).map_err(|e| Erro::Invalido(e.join("; ")))?;
    gravar_pasta(&calibrado, saida)?;
    let r = format!(
        "Pacote em {}\nRevise o texto, as faixas, os custos e as metas; depois confira de novo.\n{}\n",
        saida.display(),
        resumo(&c)
    );
    std::fs::write(saida.join("conferencia.txt"), &r)?;
    Ok(r)
}

/// Checks every mission of a package folder or `.utse`.
pub fn conferir_pacote(path: &Path) -> Result<String> {
    let arquivos = ler_pacote(path)?;
    let p = carregar(&arquivos)
        .map_err(|e| Erro::Invalido(format!("pacote inválido:\n{}", e.join("\n"))))?;
    let mut s = format!("{} ({} missões)\n", p.pacote.titulo, p.missoes.len());
    for m in &p.missoes {
        let c = conferir(m).map_err(|e| Erro::Invalido(e.to_string()))?;
        s.push_str(&format!(
            "\n{}: {}\n{}\n",
            m.missao.id,
            m.missao.titulo,
            resumo(&c)
        ));
    }
    Ok(s)
}

/// Zips a package folder as `.utse`, after checking that it loads.
pub fn empacotar(pasta: &Path, destino: &Path) -> Result<PathBuf> {
    let arquivos = ler_pacote(pasta)?;
    let arquivos: Arquivos = arquivos
        .into_iter()
        .filter(|(k, _)| k != "conferencia.txt")
        .collect();
    carregar(&arquivos)
        .map_err(|e| Erro::Invalido(format!("pacote inválido:\n{}", e.join("\n"))))?;
    gravar_utse(&arquivos, destino)?;
    Ok(destino.to_path_buf())
}
