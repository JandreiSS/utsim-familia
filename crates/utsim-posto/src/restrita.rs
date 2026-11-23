//! Restricted folder for personal data (decision G6 and requirement QS-RF-10 of
//! `docs/plano_qualidade_e_seguranca.md`, risk R5, LGPD).
//!
//! The unit keeps people only by **code** (`P-1a2b3c`). Names, sectors and health notes live
//! in a separate unit folder, the restricted one: by default `restrita/` inside the unit,
//! better a folder of the Drive shared only with the people allowed to see them (the path
//! goes in `unidade.json`, field `restrita`). The real protection is the sharing of that
//! folder. The app also refuses to write there for authors outside `acesso.json`.
//!
//! Anonymising a person rewrites the record without the personal data and deletes, from
//! the restricted folder, every older operation file that held them.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pasta::{OPS, Unidade, UnidadeErro, novo_id};
use crate::registro::Acao;

/// Collection of the people in the restricted folder.
pub const PESSOAS: &str = "pessoas";
/// Who may write in the restricted folder.
pub const ACESSO: &str = "acesso.json";
/// Name kept after anonymisation.
pub const ANONIMIZADO: &str = "[anonimizado]";

type R<T> = Result<T, UnidadeErro>;

/// A person, in the restricted folder only.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pessoa {
    /// The code used everywhere else (`P-` and 6 hex digits).
    #[serde(default)]
    pub codigo: String,
    pub nome: String,
    #[serde(default)]
    pub setor: String,
    /// Health notes (sensitive personal data).
    #[serde(default)]
    pub saude: String,
    /// Consent recorded for the use of the data.
    #[serde(default)]
    pub consentimento: bool,
    #[serde(default)]
    pub anonimizada: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Acesso {
    /// Authors allowed to write (empty: anyone who reaches the folder).
    pub escritores: Vec<String>,
}

/// The restricted folder of a unit, open.
pub struct Restrita {
    pub unidade: Unidade,
    pub acesso: Acesso,
}

impl Unidade {
    /// Where the restricted folder of this unit is (it may not exist).
    pub fn caminho_restrita(&self) -> PathBuf {
        match &self.manifesto.restrita {
            Some(p) if Path::new(p).is_absolute() => PathBuf::from(p),
            Some(p) => self.raiz().join(p),
            None => self.raiz().join("restrita"),
        }
    }

    /// Opens the restricted folder; `None` when this person cannot reach it (the folder
    /// is not shared with them, or was never created).
    pub fn abrir_restrita(&self) -> Option<Restrita> {
        let raiz = self.caminho_restrita();
        let unidade = Unidade::abrir(&raiz, self.autor()).ok()?;
        let acesso = fs::read_to_string(raiz.join(ACESSO))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Some(Restrita { unidade, acesso })
    }

    /// Creates the restricted folder at `caminho` (absent: `restrita/` inside the unit),
    /// with the authors allowed to write, and records its place in `unidade.json`.
    pub fn criar_restrita(&mut self, caminho: Option<&Path>, escritores: &[String]) -> R<Restrita> {
        let raiz = caminho.map_or_else(|| self.raiz().join("restrita"), Path::to_path_buf);
        let nome = format!("{} (dados pessoais, restrita)", self.manifesto.nome);
        let unidade = Unidade::criar(&raiz, &nome, self.autor())?;
        let acesso = Acesso {
            escritores: escritores.to_vec(),
        };
        let texto = serde_json::to_string_pretty(&acesso).expect("acesso");
        fs::write(raiz.join(ACESSO), texto).map_err(|e| UnidadeErro(e.to_string()))?;
        if let Some(c) = caminho {
            self.manifesto.restrita = Some(c.display().to_string());
            self.gravar_manifesto()?;
        }
        Ok(Restrita { unidade, acesso })
    }
}

impl Restrita {
    pub fn pode_escrever(&self) -> bool {
        self.acesso.escritores.is_empty()
            || self
                .acesso
                .escritores
                .iter()
                .any(|e| e == self.unidade.autor())
    }

    fn exigir_escrita(&self) -> R<()> {
        if self.pode_escrever() {
            Ok(())
        } else {
            Err(UnidadeErro(format!(
                "{} não está entre as pessoas que podem gravar dados pessoais nesta unidade",
                self.unidade.autor()
            )))
        }
    }

    pub fn pessoas(&self) -> Vec<Pessoa> {
        let mut v: Vec<Pessoa> = self.unidade.estado().lista(PESSOAS);
        v.sort_by(|a, b| a.codigo.cmp(&b.codigo));
        v
    }

    pub fn pessoa(&self, codigo: &str) -> Option<Pessoa> {
        self.unidade.estado().um(PESSOAS, codigo)
    }

    /// Registers a person and returns the code to use in the unit.
    pub fn cadastrar(&mut self, mut p: Pessoa) -> R<String> {
        self.exigir_escrita()?;
        if p.codigo.is_empty() {
            p.codigo = format!("P-{}", &novo_id()[..6]);
        }
        let codigo = p.codigo.clone();
        self.unidade.gravar(PESSOAS, &codigo, &p)?;
        Ok(codigo)
    }

    /// Removes the personal data of a person: the record keeps only the code, and every
    /// older operation file of that record is deleted from the restricted folder.
    pub fn anonimizar(&mut self, codigo: &str) -> R<()> {
        self.exigir_escrita()?;
        let p = Pessoa {
            codigo: codigo.to_string(),
            nome: ANONIMIZADO.to_string(),
            anonimizada: true,
            ..Pessoa::default()
        };
        let nova = self.unidade.registrar(
            PESSOAS,
            codigo,
            Acao::Gravar(serde_json::to_value(&p).expect("pessoa")),
        )?;
        self.apagar_anteriores(codigo, &nova.id)
    }

    /// Corrects the name, sector, health notes or consent of a person (the code stays).
    /// As in [`Restrita::anonimizar`], only the new version stays: the older operation files
    /// of the person are deleted, so a wrong name or sector does not linger in the folder.
    /// An anonymised person cannot get data back.
    pub fn atualizar(&mut self, p: &Pessoa) -> R<()> {
        self.exigir_escrita()?;
        let atual = self
            .pessoa(&p.codigo)
            .ok_or_else(|| UnidadeErro(format!("{} não está na pasta restrita", p.codigo)))?;
        if atual.anonimizada {
            return Err(UnidadeErro(format!(
                "{} foi anonimizada: os dados pessoais não voltam",
                p.codigo
            )));
        }
        let nova = Pessoa {
            anonimizada: false,
            ..p.clone()
        };
        let op = self.unidade.registrar(
            PESSOAS,
            &p.codigo,
            Acao::Gravar(serde_json::to_value(&nova).expect("pessoa")),
        )?;
        self.apagar_anteriores(&p.codigo, &op.id)
    }

    /// Deletes every operation file of person `codigo` but `manter`, and forgets them.
    fn apagar_anteriores(&mut self, codigo: &str, manter: &str) -> R<()> {
        let dir = self.unidade.raiz().join(OPS);
        for e in fs::read_dir(&dir)
            .map_err(|e| UnidadeErro(e.to_string()))?
            .flatten()
        {
            let path = e.path();
            let Ok(texto) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(op) = serde_json::from_str::<crate::registro::Operacao>(&texto) else {
                continue;
            };
            if op.colecao == PESSOAS && op.registro == codigo && op.id != manter {
                fs::remove_file(&path).map_err(|e| UnidadeErro(e.to_string()))?;
            }
        }
        self.unidade
            .esquecer(|op| op.colecao == PESSOAS && op.registro == codigo && op.id != manter);
        Ok(())
    }
}

/// Names of `pessoas` (not anonymised) found in `texto`: a report must return none.
pub fn nomes_no_texto(texto: &str, pessoas: &[Pessoa]) -> Vec<String> {
    let baixo = texto.to_lowercase();
    pessoas
        .iter()
        .filter(|p| !p.anonimizada && p.nome.trim().len() >= 3)
        .filter(|p| baixo.contains(&p.nome.to_lowercase()))
        .map(|p| p.nome.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Posto;

    #[test]
    fn names_stay_in_the_restricted_folder_and_anonymising_deletes_them() {
        let unidade = tempfile::tempdir().unwrap();
        let drive_restrito = tempfile::tempdir().unwrap();
        let mut u = Unidade::criar(unidade.path(), "Fábrica", "tecnica").unwrap();
        let caminho = drive_restrito.path().join("pessoal");
        let mut r = u
            .criar_restrita(Some(&caminho), &["tecnica".to_string()])
            .unwrap();
        let codigo = r
            .cadastrar(Pessoa {
                nome: "Maria Aparecida Souza".into(),
                setor: "prensas".into(),
                saude: "lesão no punho direito".into(),
                consentimento: true,
                ..Pessoa::default()
            })
            .unwrap();
        // The unit only keeps the code.
        u.gravar_posto(&Posto {
            nome: "Prensa".into(),
            atividades: vec![format!("operada por {codigo}")],
            ..Posto::default()
        })
        .unwrap();
        let tudo_da_unidade: String = std::fs::read_dir(unidade.path().join(OPS))
            .unwrap()
            .flatten()
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .collect();
        assert!(nomes_no_texto(&tudo_da_unidade, &r.pessoas()).is_empty());
        assert!(tudo_da_unidade.contains(&codigo));
        // Someone else opens the unit: finds the restricted folder through unidade.json.
        let outra = Unidade::abrir(unidade.path(), "aluno").unwrap();
        let r2 = outra.abrir_restrita().unwrap();
        assert_eq!(r2.pessoa(&codigo).unwrap().setor, "prensas");
        // ...but may not write there.
        let mut r2 = r2;
        assert!(!r2.pode_escrever());
        assert!(r2.cadastrar(Pessoa::default()).is_err());
        // Without the shared folder, nothing opens.
        std::fs::rename(&caminho, drive_restrito.path().join("movida")).unwrap();
        assert!(outra.abrir_restrita().is_none());
        std::fs::rename(drive_restrito.path().join("movida"), &caminho).unwrap();
        // Anonymising leaves no file with the name or the health note.
        r.anonimizar(&codigo).unwrap();
        let resto: String = std::fs::read_dir(caminho.join(OPS))
            .unwrap()
            .flatten()
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .collect();
        assert!(!resto.contains("Maria"));
        assert!(!resto.contains("punho"));
        let p = r.pessoa(&codigo).unwrap();
        assert_eq!(p.nome, ANONIMIZADO);
        let reaberta = Unidade::abrir(unidade.path(), "tecnica")
            .unwrap()
            .abrir_restrita()
            .unwrap();
        assert_eq!(reaberta.pessoa(&codigo).unwrap().nome, ANONIMIZADO);
    }

    #[test]
    fn name_and_sector_can_be_corrected_and_the_old_ones_are_gone() {
        let unidade = tempfile::tempdir().unwrap();
        let mut u = Unidade::criar(unidade.path(), "Fábrica", "tecnica").unwrap();
        let mut r = u.criar_restrita(None, &[]).unwrap();
        let codigo = r
            .cadastrar(Pessoa {
                nome: "Joao Sliva".into(),
                setor: "pintura".into(),
                ..Pessoa::default()
            })
            .unwrap();
        let mut p = r.pessoa(&codigo).unwrap();
        p.nome = "João Silva".into();
        p.setor = "solda".into();
        r.atualizar(&p).unwrap();
        let lida = u.abrir_restrita().unwrap().pessoa(&codigo).unwrap();
        assert_eq!(
            (lida.nome.as_str(), lida.setor.as_str()),
            ("João Silva", "solda")
        );
        // The wrong name is in no file of the restricted folder any more.
        for e in fs::read_dir(r.unidade.raiz().join(OPS)).unwrap().flatten() {
            let t = fs::read_to_string(e.path()).unwrap();
            assert!(!t.contains("Sliva") && !t.contains("pintura"), "{t}");
        }
        // An unknown code, or an anonymised person, is refused.
        assert!(
            r.atualizar(&Pessoa {
                codigo: "P-000000".into(),
                ..p.clone()
            })
            .is_err()
        );
        r.anonimizar(&codigo).unwrap();
        assert!(r.atualizar(&p).is_err());
        assert!(r.pessoa(&codigo).unwrap().anonimizada);
    }
}
