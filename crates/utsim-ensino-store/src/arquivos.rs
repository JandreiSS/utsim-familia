//! Packages on disk: a folder with `pacote.ron`, or a `.utse` (zip of that folder).
//! Deliveries (`.utsr`) and invitations (`.utsc`) are JSON files.

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use utsim_ensino_core::pacote::Arquivos;
use utsim_ensino_core::turma::Aluno;

use crate::diario::TentativaAssinada;
use crate::{Erro, Result};

/// Largest file accepted inside a package (models and texts are small).
const MAX_ARQUIVO: u64 = 16 * 1024 * 1024;

/// Every file under `dir`, by relative path with `/`.
pub fn ler_pasta(dir: &Path) -> Result<Arquivos> {
    fn walk(d: &Path, base: &Path, out: &mut Arquivos) -> Result<()> {
        for e in std::fs::read_dir(d)? {
            let p = e?.path();
            if p.is_dir() {
                walk(&p, base, out)?;
            } else {
                let rel: Vec<String> = p
                    .strip_prefix(base)
                    .map_err(|e| Erro::Invalido(e.to_string()))?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.insert(rel.join("/"), std::fs::read(&p)?);
            }
        }
        Ok(())
    }
    let mut out = Arquivos::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// Reads a `.utse`. Paths that leave the package (`..`, absolute) are refused.
pub fn ler_utse(path: &Path) -> Result<Arquivos> {
    let f = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| Erro::Invalido(format!("pacote: {e}")))?;
    let mut out = Arquivos::new();
    for i in 0..z.len() {
        let mut e = z
            .by_index(i)
            .map_err(|e| Erro::Invalido(format!("pacote: {e}")))?;
        if e.is_dir() {
            continue;
        }
        let nome = e.name().replace('\\', "/");
        if nome.starts_with('/') || nome.split('/').any(|p| p == "..") || nome.contains(':') {
            return Err(Erro::Invalido(format!(
                "caminho não permitido no pacote: {nome}"
            )));
        }
        if e.size() > MAX_ARQUIVO {
            return Err(Erro::Invalido(format!(
                "arquivo grande demais no pacote: {nome}"
            )));
        }
        let mut buf = Vec::new();
        e.by_ref().take(MAX_ARQUIVO + 1).read_to_end(&mut buf)?;
        out.insert(nome, buf);
    }
    Ok(out)
}

/// Writes `arquivos` as a `.utse`.
pub fn gravar_utse(arquivos: &Arquivos, path: &Path) -> Result<()> {
    let f = std::fs::File::create(path)?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for (k, v) in arquivos {
        z.start_file(k, opts)
            .map_err(|e| Erro::Invalido(e.to_string()))?;
        z.write_all(v)?;
    }
    z.finish().map_err(|e| Erro::Invalido(e.to_string()))?;
    Ok(())
}

/// Reads a package from a folder or a `.utse`.
pub fn ler_pacote(path: &Path) -> Result<Arquivos> {
    if path.is_dir() {
        ler_pasta(path)
    } else {
        ler_utse(path)
    }
}

/// Delivery format read by this version.
pub const FORMATO_ENTREGA: u32 = 1;

/// The offline delivery (`.utsr`): who the student is, the public key and the signed
/// journal.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entrega {
    pub formato: u32,
    /// Class id (from the invitation), empty outside a class.
    #[serde(default)]
    pub turma: String,
    pub aluno: Aluno,
    pub chave_publica: String,
    pub tentativas: Vec<TentativaAssinada>,
}

impl Entrega {
    pub fn gravar(&self, path: &Path) -> Result<()> {
        let txt = serde_json::to_string_pretty(self).map_err(|e| Erro::Invalido(e.to_string()))?;
        std::fs::write(path, txt)?;
        Ok(())
    }
    pub fn ler(path: &Path) -> Result<Entrega> {
        let e: Entrega = serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|e| Erro::Invalido(format!("{}: {e}", path.display())))?;
        if e.formato > FORMATO_ENTREGA {
            return Err(Erro::Invalido(format!(
                "{}: entrega no formato {}; esta versão lê até {FORMATO_ENTREGA}",
                path.display(),
                e.formato
            )));
        }
        Ok(e)
    }
}

/// The class invitation (`.utsc`): what a student needs to join.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Convite {
    pub turma: String,
    pub nome: String,
    #[serde(default)]
    pub periodo: String,
    pub chave_professor: String,
    /// Packages of the class (`campanha` for the built-in one).
    #[serde(default)]
    pub pacotes: Vec<String>,
}

impl Convite {
    pub fn gravar(&self, path: &Path) -> Result<()> {
        let txt = serde_json::to_string_pretty(self).map_err(|e| Erro::Invalido(e.to_string()))?;
        std::fs::write(path, txt)?;
        Ok(())
    }
    pub fn ler(path: &Path) -> Result<Convite> {
        serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|e| Erro::Invalido(format!("{}: {e}", path.display())))
    }
}
