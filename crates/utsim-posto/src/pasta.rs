//! The unit folder on disk:
//!
//! ```text
//! <unidade>/
//!   unidade.json        name and format
//!   ops/<ms>-<autor>-<id>.json   one operation per file, never edited
//!   relatorios/         reports (made again at any time)
//!   modelos/            models copied into the unit (optional)
//! ```
//!
//! Reading tolerates what sync clients leave behind: hidden and temporary files are
//! skipped, a file that does not parse yet stays pending and is read again on the next
//! [`Unidade::atualizar`], and a conflict copy (`x (1).json`) holds an operation already
//! known and is ignored by its id.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::registro::{Acao, Estado, Operacao};
use crate::{POSTOS, Posto};

pub const MANIFESTO: &str = "unidade.json";
pub const OPS: &str = "ops";
pub const RELATORIOS: &str = "relatorios";
pub const MODELOS: &str = "modelos";
/// Format of the unit folder.
pub const FORMATO: u32 = 1;

#[derive(Debug)]
pub struct UnidadeErro(pub String);

impl std::fmt::Display for UnidadeErro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnidadeErro {}

type R<T> = Result<T, UnidadeErro>;

fn io(p: &Path, e: std::io::Error) -> UnidadeErro {
    UnidadeErro(format!("{}: {e}", p.display()))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifesto {
    pub formato: u32,
    pub nome: String,
}

/// A unit folder opened by one person.
pub struct Unidade {
    raiz: PathBuf,
    pub manifesto: Manifesto,
    autor: String,
    seq: u64,
    ops: BTreeMap<String, Operacao>,
    lidos: BTreeSet<PathBuf>,
    /// Files that could not be read yet, with the reason.
    pub pendentes: BTreeMap<PathBuf, String>,
    estado: Estado,
}

fn agora_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Random id of 16 hex digits.
pub fn novo_id() -> String {
    let mut b = [0u8; 8];
    if getrandom::getrandom(&mut b).is_err() {
        b = (agora_ms() as u64).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A file name piece without characters that some file systems refuse.
fn seguro(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .take(24)
        .collect();
    if t.is_empty() { "anonimo".into() } else { t }
}

impl Unidade {
    pub fn eh_unidade(raiz: &Path) -> bool {
        raiz.join(MANIFESTO).is_file()
    }

    /// Creates a new unit folder (the folder may exist, but not a unit in it).
    pub fn criar(raiz: &Path, nome: &str, autor: &str) -> R<Unidade> {
        if Self::eh_unidade(raiz) {
            return Err(UnidadeErro(format!(
                "{} já é uma pasta de unidade",
                raiz.display()
            )));
        }
        fs::create_dir_all(raiz.join(OPS)).map_err(|e| io(raiz, e))?;
        let m = Manifesto {
            formato: FORMATO,
            nome: nome.to_string(),
        };
        let texto = serde_json::to_string_pretty(&m).expect("manifesto");
        fs::write(raiz.join(MANIFESTO), texto).map_err(|e| io(raiz, e))?;
        Self::abrir(raiz, autor)
    }

    pub fn abrir(raiz: &Path, autor: &str) -> R<Unidade> {
        let p = raiz.join(MANIFESTO);
        let texto = fs::read_to_string(&p).map_err(|e| io(&p, e))?;
        let manifesto: Manifesto = serde_json::from_str(&texto)
            .map_err(|e| UnidadeErro(format!("{}: {e}", p.display())))?;
        if manifesto.formato > FORMATO {
            return Err(UnidadeErro(format!(
                "pasta de unidade no formato {}; este programa lê até {FORMATO}",
                manifesto.formato
            )));
        }
        let mut u = Unidade {
            raiz: raiz.to_path_buf(),
            manifesto,
            autor: autor.to_string(),
            seq: 0,
            ops: BTreeMap::new(),
            lidos: BTreeSet::new(),
            pendentes: BTreeMap::new(),
            estado: Estado::default(),
        };
        u.atualizar()?;
        Ok(u)
    }

    pub fn raiz(&self) -> &Path {
        &self.raiz
    }

    pub fn autor(&self) -> &str {
        &self.autor
    }

    pub fn estado(&self) -> &Estado {
        &self.estado
    }

    /// Reads the operation files not read yet. Returns how many new operations came.
    pub fn atualizar(&mut self) -> R<usize> {
        let dir = self.raiz.join(OPS);
        fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
        let mut novas = 0;
        let entradas = fs::read_dir(&dir).map_err(|e| io(&dir, e))?;
        for e in entradas.flatten() {
            let p = e.path();
            let nome = e.file_name().to_string_lossy().to_string();
            if nome.starts_with('.') || nome.starts_with('~') || !nome.ends_with(".json") {
                continue;
            }
            if self.lidos.contains(&p) {
                continue;
            }
            match fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Operacao>(&t).map_err(|e| e.to_string()))
            {
                Ok(op) => {
                    self.pendentes.remove(&p);
                    self.lidos.insert(p);
                    if !self.ops.contains_key(&op.id) {
                        self.ops.insert(op.id.clone(), op);
                        novas += 1;
                    }
                }
                Err(err) => {
                    self.pendentes.insert(p, err);
                }
            }
        }
        if novas > 0 {
            self.estado = Estado::reproduzir(self.ops.values());
        }
        Ok(novas)
    }

    /// Writes one operation file and applies it.
    pub fn registrar(&mut self, colecao: &str, registro: &str, acao: Acao) -> R<Operacao> {
        self.seq += 1;
        let op = Operacao {
            id: novo_id(),
            autor: self.autor.clone(),
            // Never before the last operation known, even if this clock is late.
            instante_ms: agora_ms()
                .max(self.ops.values().map(|o| o.instante_ms).max().unwrap_or(0)),
            seq: self.seq,
            colecao: colecao.to_string(),
            registro: registro.to_string(),
            acao,
        };
        let dir = self.raiz.join(OPS);
        let nome = format!("{}-{}-{}.json", op.instante_ms, seguro(&op.autor), op.id);
        let tmp = dir.join(format!(".{nome}.tmp"));
        let fim = dir.join(&nome);
        let texto = serde_json::to_string_pretty(&op).expect("operação");
        fs::write(&tmp, texto).map_err(|e| io(&tmp, e))?;
        fs::rename(&tmp, &fim).map_err(|e| io(&fim, e))?;
        self.lidos.insert(fim);
        self.ops.insert(op.id.clone(), op.clone());
        self.estado = Estado::reproduzir(self.ops.values());
        Ok(op)
    }

    /// Writes a record (`id` empty: a new id is made). Returns the id.
    pub fn gravar<T: Serialize>(&mut self, colecao: &str, id: &str, valor: &T) -> R<String> {
        let id = if id.is_empty() {
            novo_id()
        } else {
            id.to_string()
        };
        let mut v = serde_json::to_value(valor).map_err(|e| UnidadeErro(e.to_string()))?;
        if let Some(o) = v.as_object_mut() {
            o.insert("id".into(), serde_json::Value::String(id.clone()));
        }
        self.registrar(colecao, &id, Acao::Gravar(v))?;
        Ok(id)
    }

    pub fn remover(&mut self, colecao: &str, id: &str) -> R<()> {
        self.registrar(colecao, id, Acao::Remover).map(|_| ())
    }

    pub fn postos(&self) -> Vec<Posto> {
        let mut v: Vec<Posto> = self.estado.lista(POSTOS);
        v.sort_by(|a, b| a.nome.cmp(&b.nome));
        v
    }

    pub fn gravar_posto(&mut self, p: &Posto) -> R<String> {
        self.gravar(POSTOS, &p.id.clone(), p)
    }

    /// Path of a model named in a station: relative to the unit folder, or absolute.
    pub fn caminho_modelo(&self, modelo: &str) -> PathBuf {
        let p = Path::new(modelo);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.raiz.join(p)
        }
    }

    /// Writes a report into `relatorios/` and returns its path.
    pub fn gravar_relatorio(&self, nome: &str, bytes: &[u8]) -> R<PathBuf> {
        let dir = self.raiz.join(RELATORIOS);
        fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
        let p = dir.join(nome);
        fs::write(&p, bytes).map_err(|e| io(&p, e))?;
        Ok(p)
    }

    pub fn total_operacoes(&self) -> usize {
        self.ops.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_people_on_the_same_folder_lose_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut ana = Unidade::criar(dir.path(), "Fábrica piloto", "ana").unwrap();
        let mut bia = Unidade::abrir(dir.path(), "bia").unwrap();
        // Both edit at the same time without seeing each other.
        let p1 = ana
            .gravar_posto(&Posto {
                nome: "Prensa".into(),
                ..Posto::default()
            })
            .unwrap();
        let p2 = bia
            .gravar_posto(&Posto {
                nome: "Solda".into(),
                ..Posto::default()
            })
            .unwrap();
        for i in 0..20 {
            ana.gravar_posto(&Posto {
                id: p1.clone(),
                nome: format!("Prensa {i}"),
                ..Posto::default()
            })
            .unwrap();
            bia.gravar_posto(&Posto {
                id: p2.clone(),
                nome: format!("Solda {i}"),
                ..Posto::default()
            })
            .unwrap();
        }
        ana.atualizar().unwrap();
        bia.atualizar().unwrap();
        assert_eq!(ana.total_operacoes(), 42);
        assert_eq!(bia.total_operacoes(), 42);
        assert_eq!(ana.postos(), bia.postos());
        let nomes: Vec<String> = ana.postos().into_iter().map(|p| p.nome).collect();
        assert_eq!(nomes, vec!["Prensa 19", "Solda 19"]);
        // A conflict copy of the sync client and a half-written file change nothing.
        let ops = dir.path().join(OPS);
        let um = fs::read_dir(&ops).unwrap().next().unwrap().unwrap().path();
        fs::copy(&um, ops.join("copia (1).json")).unwrap();
        fs::write(ops.join("chegando.json"), "{\"id\": ").unwrap();
        let mut caio = Unidade::abrir(dir.path(), "caio").unwrap();
        assert_eq!(caio.total_operacoes(), 42);
        assert_eq!(caio.pendentes.len(), 1);
        assert_eq!(caio.postos(), ana.postos());
        caio.remover(POSTOS, &p2).unwrap();
        ana.atualizar().unwrap();
        assert_eq!(ana.postos().len(), 1);
    }
}
