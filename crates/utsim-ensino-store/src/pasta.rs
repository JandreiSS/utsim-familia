//! The class folder (local or synced by Google Drive for desktop; the app never calls the
//! Drive API, it only reads and writes files):
//!
//! ```text
//! turma.ron                     manifest
//! alunos.csv                    student list (teacher only)
//! pacotes/                      published packages (students only read)
//! entregas/<registro>/          one folder per student
//!   aluno.json                  name, record and public key
//!   <missao>/<sequencia>.json   one signed attempt per file (append only)
//! correcao/                     grades and reports (teacher only)
//! ```

use std::path::{Path, PathBuf};

use utsim_ensino_core::pacote::Arquivos;
use utsim_ensino_core::turma::{
    Aluno, FORMATO_TURMA, Turma, alunos_csv, importar_alunos, registro_valido,
};

use crate::arquivos::{Convite, Entrega, FORMATO_ENTREGA, gravar_utse};
use crate::diario::TentativaAssinada;
use crate::{Erro, Result};

pub struct PastaTurma {
    pub dir: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FichaAluno {
    aluno: Aluno,
    chave_publica: String,
}

/// `ato1/furadeira` as a folder name.
pub fn slug(missao: &str) -> String {
    missao
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl PastaTurma {
    /// Creates the folder layout and the manifest.
    pub fn criar(dir: &Path, turma: &Turma, alunos: &[Aluno]) -> Result<PastaTurma> {
        for sub in ["pacotes", "entregas", "correcao"] {
            std::fs::create_dir_all(dir.join(sub))?;
        }
        let p = PastaTurma {
            dir: dir.to_path_buf(),
        };
        p.salvar_turma(turma)?;
        p.salvar_alunos(alunos)?;
        Ok(p)
    }

    pub fn abrir(dir: &Path) -> Result<PastaTurma> {
        let p = PastaTurma {
            dir: dir.to_path_buf(),
        };
        p.turma()?;
        Ok(p)
    }

    pub fn turma(&self) -> Result<Turma> {
        let path = self.dir.join("turma.ron");
        let t: Turma = ron::from_str(&std::fs::read_to_string(&path)?)
            .map_err(|e| Erro::Invalido(format!("turma.ron: {e}")))?;
        if t.formato > FORMATO_TURMA {
            return Err(Erro::Invalido(format!(
                "turma.ron no formato {}; esta versão lê até {FORMATO_TURMA}",
                t.formato
            )));
        }
        Ok(t)
    }

    pub fn salvar_turma(&self, t: &Turma) -> Result<()> {
        let txt = ron::ser::to_string_pretty(t, ron::ser::PrettyConfig::default())
            .map_err(|e| Erro::Invalido(e.to_string()))?;
        std::fs::write(
            self.dir.join("turma.ron"),
            format!("// Turma do UTSim Ensino\n{txt}\n"),
        )?;
        Ok(())
    }

    /// The student list (empty when the file is missing, as in a student's copy).
    pub fn alunos(&self) -> Result<Vec<Aluno>> {
        let path = self.dir.join("alunos.csv");
        if !path.exists() {
            return Ok(Vec::new());
        }
        importar_alunos(&std::fs::read_to_string(path)?).map_err(|e| Erro::Invalido(e.join("; ")))
    }

    pub fn salvar_alunos(&self, alunos: &[Aluno]) -> Result<()> {
        std::fs::write(self.dir.join("alunos.csv"), alunos_csv(alunos))?;
        Ok(())
    }

    pub fn convite(&self) -> Result<Convite> {
        let t = self.turma()?;
        Ok(Convite {
            turma: t.id,
            nome: t.nome,
            periodo: t.periodo,
            chave_professor: t.chave_professor,
            pacotes: t.pacotes.iter().map(|p| p.arquivo.clone()).collect(),
        })
    }

    /// Writes a package to `pacotes/<nome>.utse`.
    pub fn publicar_pacote(&self, nome: &str, arquivos: &Arquivos) -> Result<PathBuf> {
        let path = self
            .dir
            .join("pacotes")
            .join(format!("{}.utse", slug(nome)));
        gravar_utse(arquivos, &path)?;
        Ok(path)
    }

    fn pasta_aluno(&self, registro: &str) -> Result<PathBuf> {
        if !registro_valido(registro) {
            return Err(Erro::Invalido(format!("registro '{registro}' inválido")));
        }
        Ok(self.dir.join("entregas").join(registro))
    }

    /// Student side: writes one signed attempt (never overwrites).
    pub fn gravar_tentativa(
        &self,
        aluno: &Aluno,
        chave_publica: &str,
        t: &TentativaAssinada,
    ) -> Result<PathBuf> {
        let base = self.pasta_aluno(&aluno.registro)?;
        std::fs::create_dir_all(&base)?;
        let ficha = base.join("aluno.json");
        if !ficha.exists() {
            let f = FichaAluno {
                aluno: aluno.clone(),
                chave_publica: chave_publica.to_string(),
            };
            std::fs::write(
                &ficha,
                serde_json::to_string_pretty(&f).map_err(|e| Erro::Invalido(e.to_string()))?,
            )?;
        }
        let r = t.registro()?;
        let dir = base.join(slug(&r.missao));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{:06}.json", r.sequencia));
        if !path.exists() {
            std::fs::write(
                &path,
                serde_json::to_string_pretty(t).map_err(|e| Erro::Invalido(e.to_string()))?,
            )?;
        }
        Ok(path)
    }

    /// Teacher side: every student folder as a delivery, plus the problems found reading.
    pub fn entregas(&self) -> (Vec<Entrega>, Vec<String>) {
        let mut out = Vec::new();
        let mut avisos = Vec::new();
        let Ok(rd) = std::fs::read_dir(self.dir.join("entregas")) else {
            return (out, avisos);
        };
        let mut pastas: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        pastas.sort();
        for p in pastas {
            let nome = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ficha: FichaAluno = match std::fs::read_to_string(p.join("aluno.json"))
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
            {
                Ok(f) => f,
                Err(e) => {
                    avisos.push(format!("entregas/{nome}: sem aluno.json válido ({e})"));
                    continue;
                }
            };
            let mut tentativas = Vec::new();
            for sub in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                let sp = sub.path();
                if !sp.is_dir() {
                    continue;
                }
                for f in std::fs::read_dir(&sp).into_iter().flatten().flatten() {
                    let fp = f.path();
                    if fp.extension().is_some_and(|e| e == "json") {
                        match std::fs::read_to_string(&fp)
                            .map_err(|e| e.to_string())
                            .and_then(|t| {
                                serde_json::from_str::<TentativaAssinada>(&t)
                                    .map_err(|e| e.to_string())
                            }) {
                            Ok(t) => tentativas.push(t),
                            Err(e) => avisos.push(format!("{}: {e}", fp.display())),
                        }
                    }
                }
            }
            out.push(Entrega {
                formato: FORMATO_ENTREGA,
                turma: String::new(),
                aluno: ficha.aluno,
                chave_publica: ficha.chave_publica,
                tentativas,
            });
        }
        (out, avisos)
    }

    /// Writes a file under `correcao/`.
    pub fn gravar_correcao(&self, nome: &str, bytes: &[u8]) -> Result<PathBuf> {
        let dir = self.dir.join("correcao");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(nome);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }
}

/// Every `.utsr` in `dir` (offline deliveries), plus the files that could not be read.
pub fn importar_entregas(dir: &Path) -> (Vec<Entrega>, Vec<String>) {
    let mut out = Vec::new();
    let mut avisos = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "utsr"))
        .collect();
    files.sort();
    for f in files {
        match Entrega::ler(&f) {
            Ok(e) => out.push(e),
            Err(e) => avisos.push(e.to_string()),
        }
    }
    (out, avisos)
}
