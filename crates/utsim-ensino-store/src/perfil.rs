//! The local profile: identity, keys, progress per package, the signed journal and the
//! class the student joined. In shared lab computers the profile can live in a folder the
//! student chooses (a pen drive), through `UTSIM_ENSINO_PERFIL`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use utsim_ensino_core::VERSAO_NUCLEO;
use utsim_ensino_core::nota::Tentativa;
use utsim_ensino_core::pacote::PacoteCarregado;
use utsim_ensino_core::progresso::Progresso;
use utsim_ensino_core::turma::Aluno;

use crate::arquivos::{Convite, Entrega, FORMATO_ENTREGA};
use crate::chave::Chave;
use crate::diario::{RegistroTentativa, TentativaAssinada, assinar};
use crate::pasta::{PastaTurma, slug};
use crate::{Erro, Result};

/// Overrides the profile folder.
pub const PERFIL_ENV: &str = "UTSIM_ENSINO_PERFIL";

/// The class the student joined.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Vinculo {
    pub convite: Convite,
    /// The synced class folder, when the class uses one (else deliveries go by file).
    #[serde(default)]
    pub pasta: Option<PathBuf>,
}

/// Preferences and identity.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub identidade: Option<Aluno>,
    #[serde(default)]
    pub turma: Option<Vinculo>,
    /// Teacher mode on (a local profile with the teacher's key).
    #[serde(default)]
    pub professor: bool,
    /// Class folders the teacher opened.
    #[serde(default)]
    pub turmas_professor: Vec<PathBuf>,
    /// Text size multiplier (1.0 is the default).
    #[serde(default)]
    pub escala_texto: Option<f32>,
}

pub struct Perfil {
    pub dir: PathBuf,
}

impl Perfil {
    /// `UTSIM_ENSINO_PERFIL`, else the user's application data folder.
    pub fn padrao() -> Option<Perfil> {
        if let Some(d) = std::env::var_os(PERFIL_ENV).filter(|d| !d.is_empty()) {
            return Some(Perfil::em(Path::new(&d)));
        }
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let dir = if cfg!(windows) {
            var("APPDATA")?.join("UTSim").join("ensino")
        } else {
            var("XDG_CONFIG_HOME")
                .or_else(|| var("HOME").map(|h| h.join(".config")))?
                .join("utsim")
                .join("ensino")
        };
        Some(Perfil::em(&dir))
    }

    pub fn em(dir: &Path) -> Perfil {
        Perfil {
            dir: dir.to_path_buf(),
        }
    }

    fn ler_json<T: for<'a> Deserialize<'a> + Default>(&self, nome: &str) -> T {
        std::fs::read_to_string(self.dir.join(nome))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn gravar_json<T: Serialize>(&self, nome: &str, v: &T) -> Result<()> {
        let path = self.dir.join(nome);
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string_pretty(v).map_err(|e| Erro::Invalido(e.to_string()))?,
        )?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn config(&self) -> Config {
        self.ler_json("config.json")
    }

    pub fn salvar_config(&self, c: &Config) -> Result<()> {
        self.gravar_json("config.json", c)
    }

    /// The student's key, created on first use.
    pub fn chave(&self) -> Result<Chave> {
        Chave::carregar_ou_criar(&self.dir.join("chave_aluno.txt"))
    }

    /// The teacher's key, created when teacher mode is turned on.
    pub fn chave_professor(&self) -> Result<Chave> {
        Chave::carregar_ou_criar(&self.dir.join("professor").join("chave.txt"))
    }

    pub fn progresso(&self, pacote: &str) -> Progresso {
        self.ler_json(&format!("progresso/{}.json", slug(pacote)))
    }

    pub fn salvar_progresso(&self, pacote: &str, p: &Progresso) -> Result<()> {
        self.gravar_json(&format!("progresso/{}.json", slug(pacote)), p)
    }

    /// The signed journal, in order.
    pub fn diario(&self) -> Vec<TentativaAssinada> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(self.dir.join("diario"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        files
            .iter()
            .filter_map(|f| std::fs::read_to_string(f).ok())
            .filter_map(|t| serde_json::from_str(&t).ok())
            .collect()
    }

    /// Signs and stores an attempt; with a synced class folder, writes it there too.
    /// Returns the signed attempt and, when the copy to the class folder failed, why.
    pub fn registrar(
        &self,
        pacote: &PacoteCarregado,
        missao: &str,
        tentativa: &Tentativa,
        nota_informada: Option<f64>,
        quando: i64,
    ) -> Result<(TentativaAssinada, Option<String>)> {
        let config = self.config();
        let aluno = config
            .identidade
            .clone()
            .ok_or_else(|| Erro::Invalido("identifique-se antes de jogar".into()))?;
        let chave = self.chave()?;
        let diario = self.diario();
        let r = RegistroTentativa {
            aluno: aluno.registro.clone(),
            pacote: pacote.pacote.id.clone(),
            pacote_hash: pacote.hash.clone(),
            missao: missao.to_string(),
            versao_nucleo: VERSAO_NUCLEO.to_string(),
            quando,
            sequencia: 0,
            anterior: String::new(),
            tentativa: tentativa.clone(),
            nota_informada,
        };
        let t = assinar(&chave, r, diario.last())?;
        let seq = t.registro()?.sequencia;
        self.gravar_json(&format!("diario/{seq:06}.json"), &t)?;
        let mut aviso = None;
        if let Some(Vinculo { pasta: Some(p), .. }) = &config.turma {
            let pasta = PastaTurma { dir: p.clone() };
            if let Err(e) = pasta.gravar_tentativa(&aluno, &chave.publica(), &t) {
                aviso = Some(format!("não foi possível gravar na pasta da turma: {e}"));
            }
        }
        Ok((t, aviso))
    }

    /// The offline delivery with the whole journal.
    pub fn entrega(&self) -> Result<Entrega> {
        let config = self.config();
        let aluno = config
            .identidade
            .clone()
            .ok_or_else(|| Erro::Invalido("identifique-se antes de entregar".into()))?;
        Ok(Entrega {
            formato: FORMATO_ENTREGA,
            turma: config.turma.map(|v| v.convite.turma).unwrap_or_default(),
            aluno,
            chave_publica: self.chave()?.publica(),
            tentativas: self.diario(),
        })
    }

    /// Copies every attempt of the journal to the class folder (after joining it late).
    pub fn sincronizar_pasta(&self) -> Result<usize> {
        let config = self.config();
        let (Some(aluno), Some(Vinculo { pasta: Some(p), .. })) = (config.identidade, config.turma)
        else {
            return Ok(0);
        };
        let chave = self.chave()?.publica();
        let pasta = PastaTurma { dir: p };
        let d = self.diario();
        for t in &d {
            pasta.gravar_tentativa(&aluno, &chave, t)?;
        }
        Ok(d.len())
    }
}
