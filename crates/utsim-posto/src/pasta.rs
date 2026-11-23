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
use serde_json::Value;

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
    /// Restricted folder for personal data, relative to the unit or absolute (absent:
    /// `restrita/` inside the unit). See [`crate::restrita`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restrita: Option<String>,
}

/// One record changed by an operation of this person: the value before and after
/// (`None`: the record did not exist, or was removed).
#[derive(Clone, Debug, PartialEq)]
struct Mudanca {
    colecao: String,
    registro: String,
    antes: Option<Value>,
    depois: Option<Value>,
}

/// How many steps [`Unidade::desfazer`] keeps.
pub const PASSOS_DE_DESFAZER: usize = 100;

/// What [`Unidade::desfazer`] and [`Unidade::refazer`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desfeito {
    /// Nothing to undo (or redo).
    Nada,
    /// The step went back (or forward), changing this many records.
    Feito(usize),
    /// Someone changed one of those records after this step: nothing was done, and the
    /// step stays for later.
    Mudou,
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
    /// Steps of this person in this session, to undo and redo (each step is one action,
    /// that may change several records, like a removal that takes the children along).
    desfazer: Vec<Vec<Mudanca>>,
    refazer: Vec<Vec<Mudanca>>,
    /// The step being gathered by [`Unidade::em_grupo`].
    grupo: Option<Vec<Mudanca>>,
    /// Writing an undo or a redo: the operations are not new steps.
    voltando: bool,
    /// A unit that was never saved: it lives in a temporary folder, deleted when the unit
    /// is dropped ([`Unidade::rascunho`], [`Unidade::salvar_como`]).
    temporaria: bool,
    /// With automatic saving off, the operation files wait here (path and text) for
    /// [`Unidade::gravar_pendentes`]. The state already has them.
    adiadas: Vec<(PathBuf, String)>,
    adiar: bool,
}

impl Drop for Unidade {
    fn drop(&mut self) {
        // The draft of a unit never saved goes with it. A saved unit is never touched.
        if self.temporaria {
            let _ = fs::remove_dir_all(&self.raiz);
        }
    }
}

/// Copies the folder `de` into `para`, file by file (subfolders too).
fn copiar_pasta(de: &Path, para: &Path) -> R<()> {
    fs::create_dir_all(para).map_err(|e| io(para, e))?;
    for e in fs::read_dir(de).map_err(|e| io(de, e))?.flatten() {
        let (origem, destino) = (e.path(), para.join(e.file_name()));
        if origem.is_dir() {
            copiar_pasta(&origem, &destino)?;
        } else {
            fs::copy(&origem, &destino).map_err(|e| io(&origem, e))?;
        }
    }
    Ok(())
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
            restrita: None,
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
            desfazer: Vec::new(),
            refazer: Vec::new(),
            grupo: None,
            voltando: false,
            temporaria: false,
            adiadas: Vec::new(),
            adiar: false,
        };
        u.atualizar()?;
        Ok(u)
    }

    /// A new unit that is not in any folder of the person yet: it lives in a temporary
    /// folder of the system, deleted when the unit is dropped, until
    /// [`Unidade::salvar_como`] gives it a place. Everything works on it as on a saved unit.
    pub fn rascunho(nome: &str, autor: &str) -> R<Unidade> {
        let raiz = std::env::temp_dir().join(format!("utsim-unidade-nova-{}", novo_id()));
        let mut u = Unidade::criar(&raiz, nome, autor)?;
        u.temporaria = true;
        Ok(u)
    }

    /// The unit was never saved (it is a draft in a temporary folder).
    pub fn eh_rascunho(&self) -> bool {
        self.temporaria
    }

    /// Saves the unit in the folder `destino` with the name `nome` and goes on working there.
    /// Everything of the folder goes (operations, models, reports, a restricted folder
    /// inside it), with the operations still waiting for the Save. `destino` cannot be a unit
    /// already, nor a folder inside this unit, nor a folder with files: nothing is
    /// overwritten. The draft folder of a unit never saved is deleted after the copy. The
    /// undo steps stay.
    pub fn salvar_como(&mut self, destino: &Path, nome: &str) -> R<()> {
        if Self::eh_unidade(destino) {
            return Err(UnidadeErro(format!(
                "{} já é uma pasta de unidade",
                destino.display()
            )));
        }
        if destino.starts_with(&self.raiz) {
            return Err(UnidadeErro(format!(
                "{} fica dentro da unidade aberta",
                destino.display()
            )));
        }
        if fs::read_dir(destino).is_ok_and(|mut d| d.next().is_some()) {
            return Err(UnidadeErro(format!(
                "{} já tem arquivos: escolha uma pasta nova ou vazia",
                destino.display()
            )));
        }
        copiar_pasta(&self.raiz, destino)?;
        let base = self.raiz.clone();
        for (p, texto) in &self.adiadas {
            let fim = destino.join(p.strip_prefix(&base).unwrap_or(p));
            utsim_pasta::write_new(&fim, texto.as_bytes())
                .map_err(|e| UnidadeErro(e.to_string()))?;
        }
        let mut manifesto = self.manifesto.clone();
        manifesto.nome = nome.trim().to_string();
        let texto = serde_json::to_string_pretty(&manifesto).expect("manifesto");
        utsim_pasta::write_replace(&destino.join(MANIFESTO), texto.as_bytes())
            .map_err(|e| UnidadeErro(e.to_string()))?;
        let mut nova = Unidade::abrir(destino, &self.autor)?;
        nova.seq = self.seq;
        nova.desfazer = std::mem::take(&mut self.desfazer);
        nova.refazer = std::mem::take(&mut self.refazer);
        nova.adiar = self.adiar;
        // The old one is dropped here: a draft takes its temporary folder with it.
        *self = nova;
        Ok(())
    }

    /// Automatic saving off (`true`): the operations wait in memory for
    /// [`Unidade::gravar_pendentes`]. Turning it back on writes what was waiting.
    pub fn adiar_gravacao(&mut self, adiar: bool) -> R<usize> {
        self.adiar = adiar;
        if adiar {
            Ok(0)
        } else {
            self.gravar_pendentes()
        }
    }

    pub fn gravacao_adiada(&self) -> bool {
        self.adiar
    }

    /// Operations made but not written to the folder yet.
    pub fn por_gravar(&self) -> usize {
        self.adiadas.len()
    }

    /// Writes the operations that were waiting (the Save with automatic saving off).
    /// Returns how many. On a failure, the ones not written keep waiting.
    pub fn gravar_pendentes(&mut self) -> R<usize> {
        let mut n = 0;
        while let Some((p, texto)) = self.adiadas.first() {
            utsim_pasta::write_new(p, texto.as_bytes()).map_err(|e| UnidadeErro(e.to_string()))?;
            self.adiadas.remove(0);
            n += 1;
        }
        Ok(n)
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
            if !utsim_pasta::is_candidate(&nome) {
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
        let ultima = self.ops.values().max_by(|a, b| a.chave().cmp(&b.chave()));
        let instante_ultima = ultima.map_or(0, |o| o.instante_ms);
        let op = Operacao {
            id: novo_id(),
            autor: self.autor.clone(),
            // Never before the last operation known, even if this clock is late.
            instante_ms: agora_ms().max(instante_ultima),
            seq: self.seq,
            colecao: colecao.to_string(),
            registro: registro.to_string(),
            acao,
        };
        // After every operation known (the usual case): it goes on top of the state,
        // without replaying (and copying) the whole log again.
        let no_topo = ultima.is_none_or(|u| op.chave() > u.chave());
        let dir = self.raiz.join(OPS);
        let nome = format!("{}-{}-{}.json", op.instante_ms, seguro(&op.autor), op.id);
        let fim = dir.join(&nome);
        let texto = serde_json::to_string_pretty(&op).expect("operação");
        if self.adiar {
            // Automatic saving off: the file waits for the Save.
            self.adiadas.push((fim.clone(), texto));
        } else {
            // Written once and never replaced (G6: the same primitive as the Seis Sigma).
            utsim_pasta::write_new(&fim, texto.as_bytes())
                .map_err(|e| UnidadeErro(e.to_string()))?;
        }
        let antes = self.estado.valor(colecao, registro).cloned();
        self.lidos.insert(fim);
        if no_topo {
            self.estado.aplicar(&op);
        }
        self.ops.insert(op.id.clone(), op.clone());
        if !no_topo {
            self.estado = Estado::reproduzir(self.ops.values());
        }
        let depois = match &op.acao {
            Acao::Gravar(v) => Some(v.clone()),
            Acao::Remover => None,
        };
        self.anotar(Mudanca {
            colecao: colecao.to_string(),
            registro: registro.to_string(),
            antes,
            depois,
        });
        Ok(op)
    }

    fn anotar(&mut self, m: Mudanca) {
        if self.voltando || m.antes == m.depois {
            return;
        }
        match &mut self.grupo {
            Some(g) => g.push(m),
            None => self.novo_passo(vec![m]),
        }
    }

    fn novo_passo(&mut self, passo: Vec<Mudanca>) {
        self.desfazer.push(passo);
        if self.desfazer.len() > PASSOS_DE_DESFAZER {
            self.desfazer.remove(0);
        }
        self.refazer.clear();
    }

    /// Runs `f` as one step of undo: everything it writes goes back together. Inside
    /// another group, it joins that one. A failure keeps what was written (as one step).
    pub fn em_grupo<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        let fora = self.grupo.is_none();
        if fora {
            self.grupo = Some(Vec::new());
        }
        let r = f(self);
        if fora
            && let Some(g) = self.grupo.take()
            && !g.is_empty()
        {
            self.novo_passo(g);
        }
        r
    }

    pub fn pode_desfazer(&self) -> bool {
        !self.desfazer.is_empty()
    }

    pub fn pode_refazer(&self) -> bool {
        !self.refazer.is_empty()
    }

    /// Undoes the last step of this person in this session: each record it changed gets
    /// back the value it had before, as new operations (the log keeps everything). When
    /// someone changed one of those records after the step, nothing is done.
    pub fn desfazer(&mut self) -> R<Desfeito> {
        let Some(passo) = self.desfazer.pop() else {
            return Ok(Desfeito::Nada);
        };
        if passo
            .iter()
            .any(|m| self.estado.valor(&m.colecao, &m.registro) != m.depois.as_ref())
        {
            self.desfazer.push(passo);
            return Ok(Desfeito::Mudou);
        }
        let n = passo.len();
        let r = self.voltar(passo.iter().rev().map(|m| (m, &m.antes)));
        self.refazer.push(passo);
        r.map(|()| Desfeito::Feito(n))
    }

    /// Writes again the last step undone (while nothing else was written).
    pub fn refazer(&mut self) -> R<Desfeito> {
        let Some(passo) = self.refazer.pop() else {
            return Ok(Desfeito::Nada);
        };
        if passo
            .iter()
            .any(|m| self.estado.valor(&m.colecao, &m.registro) != m.antes.as_ref())
        {
            self.refazer.push(passo);
            return Ok(Desfeito::Mudou);
        }
        let n = passo.len();
        let r = self.voltar(passo.iter().map(|m| (m, &m.depois)));
        self.desfazer.push(passo);
        r.map(|()| Desfeito::Feito(n))
    }

    fn voltar<'a>(
        &mut self,
        valores: impl Iterator<Item = (&'a Mudanca, &'a Option<Value>)>,
    ) -> R<()> {
        self.voltando = true;
        let mut r = Ok(());
        for (m, v) in valores {
            let acao = match v {
                Some(v) => Acao::Gravar(v.clone()),
                None => Acao::Remover,
            };
            if let Err(e) = self.registrar(&m.colecao, &m.registro, acao) {
                r = Err(e);
                break;
            }
        }
        self.voltando = false;
        r
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

    /// Writes `unidade.json` again (the manifest has a single writer: whoever creates or
    /// moves the restricted folder).
    pub fn gravar_manifesto(&self) -> R<()> {
        let texto = serde_json::to_string_pretty(&self.manifesto).expect("manifesto");
        utsim_pasta::write_replace(&self.raiz.join(MANIFESTO), texto.as_bytes())
            .map_err(|e| UnidadeErro(e.to_string()))
    }

    /// Forgets the operations matching `apagar` (their files were deleted on purpose, as in
    /// an anonymisation) and rebuilds the state.
    /// The undo steps go too: they could bring back what was deleted.
    pub fn esquecer(&mut self, apagar: impl Fn(&Operacao) -> bool) {
        self.desfazer.clear();
        self.refazer.clear();
        self.ops.retain(|_, op| !apagar(op));
        self.lidos.retain(|p| p.exists());
        self.estado = Estado::reproduzir(self.ops.values());
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

    fn nomes(u: &Unidade) -> Vec<String> {
        u.postos().into_iter().map(|p| p.nome).collect()
    }

    #[test]
    fn undo_and_redo_go_back_and_forth_and_stop_when_someone_else_changed() {
        let dir = tempfile::tempdir().unwrap();
        let mut ana = Unidade::criar(dir.path(), "Fábrica", "ana").unwrap();
        assert_eq!(ana.desfazer().unwrap(), Desfeito::Nada);
        let p = ana
            .gravar_posto(&Posto {
                nome: "Prensa".into(),
                ..Posto::default()
            })
            .unwrap();
        let mut posto: Posto = ana.estado().um(POSTOS, &p).unwrap();
        posto.nome = "Prensa 2".into();
        ana.gravar_posto(&posto).unwrap();
        ana.remover(POSTOS, &p).unwrap();
        assert!(nomes(&ana).is_empty());
        // Back one step at a time, then forward.
        assert_eq!(ana.desfazer().unwrap(), Desfeito::Feito(1));
        assert_eq!(nomes(&ana), ["Prensa 2"]);
        assert_eq!(ana.desfazer().unwrap(), Desfeito::Feito(1));
        assert_eq!(nomes(&ana), ["Prensa"]);
        assert_eq!(ana.refazer().unwrap(), Desfeito::Feito(1));
        assert_eq!(nomes(&ana), ["Prensa 2"]);
        // Another person renames it: Ana's step no longer undoes, and stays.
        let mut bia = Unidade::abrir(dir.path(), "bia").unwrap();
        posto.nome = "Prensa da Bia".into();
        bia.gravar_posto(&posto).unwrap();
        ana.atualizar().unwrap();
        assert_eq!(ana.desfazer().unwrap(), Desfeito::Mudou);
        assert!(ana.pode_desfazer());
        assert_eq!(nomes(&ana), ["Prensa da Bia"]);
        // A new write clears the redo.
        assert!(ana.refazer().is_ok());
        ana.gravar(POSTOS, "", &Posto::default()).unwrap();
        assert!(!ana.pode_refazer());
        // Undoing writes new operations: the log keeps everything, and others see it.
        bia.atualizar().unwrap();
        assert_eq!(bia.estado(), ana.estado());
    }

    #[test]
    fn a_group_goes_back_in_one_step() {
        let dir = tempfile::tempdir().unwrap();
        let mut u = Unidade::criar(dir.path(), "Fábrica", "ana").unwrap();
        u.em_grupo(|u| {
            u.gravar(POSTOS, "a", &Posto::default())?;
            u.em_grupo(|u| u.gravar(POSTOS, "b", &Posto::default()))?;
            Ok(())
        })
        .unwrap();
        assert_eq!(u.estado().conta(POSTOS), 2);
        assert_eq!(u.desfazer().unwrap(), Desfeito::Feito(2));
        assert_eq!(u.estado().conta(POSTOS), 0);
        assert!(!u.pode_desfazer());
    }

    #[test]
    fn writing_on_top_keeps_the_same_state_as_replaying() {
        let dir = tempfile::tempdir().unwrap();
        let mut u = Unidade::criar(dir.path(), "Fábrica", "ana").unwrap();
        for i in 0..20 {
            let id = format!("p{}", i % 7);
            if i % 5 == 4 {
                u.remover(POSTOS, &id).unwrap();
            } else {
                u.gravar(POSTOS, &id, &Posto::default()).unwrap();
            }
        }
        let replay = Estado::reproduzir(u.ops.values());
        assert_eq!(u.estado().conta(POSTOS), replay.conta(POSTOS));
        assert_eq!(u.estado(), &replay);
    }

    #[test]
    fn a_draft_lives_in_a_temporary_folder_until_saved_somewhere() {
        let destino = tempfile::tempdir().unwrap();
        let mut u = Unidade::rascunho("Unidade nova", "ana").unwrap();
        assert!(u.eh_rascunho());
        let temporaria = u.raiz().to_path_buf();
        assert!(temporaria.starts_with(std::env::temp_dir()));
        u.gravar_posto(&Posto {
            nome: "Prensa".into(),
            ..Posto::default()
        })
        .unwrap();
        // A folder with files, or a unit, is never overwritten.
        let cheia = destino.path().join("cheia");
        fs::create_dir_all(&cheia).unwrap();
        fs::write(cheia.join("x.txt"), "x").unwrap();
        assert!(u.salvar_como(&cheia, "Fábrica").is_err());
        let pasta = destino.path().join("Fabrica");
        u.salvar_como(&pasta, "Fábrica").unwrap();
        assert!(!u.eh_rascunho());
        assert_eq!(u.raiz(), pasta);
        assert_eq!(u.manifesto.nome, "Fábrica");
        assert!(!temporaria.exists());
        // The undo steps came along.
        assert!(u.pode_desfazer());
        assert_eq!(Unidade::abrir(&pasta, "bia").unwrap().postos().len(), 1);
        // Saving as again copies the saved unit, which stays where it was.
        let copia = destino.path().join("Copia");
        u.salvar_como(&copia, "Cópia").unwrap();
        assert!(Unidade::eh_unidade(&pasta) && Unidade::eh_unidade(&copia));
        // A draft dropped without saving takes its folder along.
        let outra = Unidade::rascunho("Unidade nova", "ana").unwrap();
        let raiz = outra.raiz().to_path_buf();
        drop(outra);
        assert!(!raiz.exists());
    }

    #[test]
    fn with_automatic_saving_off_the_operations_wait_for_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut ana = Unidade::criar(dir.path(), "Fábrica", "ana").unwrap();
        ana.adiar_gravacao(true).unwrap();
        ana.gravar_posto(&Posto {
            nome: "Prensa".into(),
            ..Posto::default()
        })
        .unwrap();
        assert_eq!(ana.postos().len(), 1);
        assert_eq!(ana.por_gravar(), 1);
        // Reading the folder does not lose what waits.
        ana.atualizar().unwrap();
        assert_eq!(ana.postos().len(), 1);
        assert!(
            Unidade::abrir(dir.path(), "bia")
                .unwrap()
                .postos()
                .is_empty()
        );
        assert_eq!(ana.gravar_pendentes().unwrap(), 1);
        assert_eq!(ana.por_gravar(), 0);
        assert_eq!(Unidade::abrir(dir.path(), "bia").unwrap().postos().len(), 1);
        // Turning it back on writes what waits.
        ana.gravar(POSTOS, "", &Posto::default()).unwrap();
        assert_eq!(ana.adiar_gravacao(false).unwrap(), 1);
        assert_eq!(Unidade::abrir(dir.path(), "bia").unwrap().postos().len(), 2);
    }
}
