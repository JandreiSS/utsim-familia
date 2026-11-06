//! The student's progress in a package: best and last grade per mission, stars, and
//! which missions are unlocked (each solved mission unlocks the next one).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::nota::Resultado;
use crate::pacote::{PacoteCarregado, Registro};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RegistroMissao {
    pub tentativas: u32,
    pub melhor_nota: f64,
    pub ultima_nota: f64,
    /// Best stars reached.
    pub estrelas: u8,
    pub resolvida: bool,
    /// Highest hint level ever opened.
    pub dicas: u8,
}

impl RegistroMissao {
    pub fn nota(&self, registro: Registro) -> f64 {
        match registro {
            Registro::Melhor => self.melhor_nota,
            Registro::Ultima => self.ultima_nota,
        }
    }
}

/// Progress per package hash is kept by the caller; this is one package.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Progresso {
    pub missoes: BTreeMap<String, RegistroMissao>,
}

impl Progresso {
    /// Records a graded attempt.
    pub fn registrar(&mut self, missao: &str, r: &Resultado, dicas: u8) {
        let e = self.missoes.entry(missao.to_string()).or_default();
        e.tentativas += 1;
        e.ultima_nota = r.nota;
        e.melhor_nota = e.melhor_nota.max(r.nota);
        e.estrelas = e.estrelas.max(r.estrelas);
        e.resolvida |= r.resolvida;
        e.dicas = e.dicas.max(dicas);
    }

    pub fn registro(&self, missao: &str) -> Option<&RegistroMissao> {
        self.missoes.get(missao)
    }

    pub fn resolvida(&self, missao: &str) -> bool {
        self.missoes.get(missao).is_some_and(|r| r.resolvida)
    }

    /// Whether mission `i` of `pacote` is unlocked: the first of each act is open when
    /// the previous act is finished (the first act is always open); the others need the
    /// previous mission solved.
    pub fn liberada(&self, pacote: &PacoteCarregado, i: usize) -> bool {
        if i == 0 {
            return true;
        }
        let Some(m) = pacote.missoes.get(i) else {
            return false;
        };
        let anterior = &pacote.missoes[i - 1];
        if anterior.ato == m.ato {
            return self.resolvida(&anterior.missao.id);
        }
        // First of a new act: every mission of the previous act solved.
        pacote
            .missoes
            .iter()
            .filter(|x| x.ato == anterior.ato)
            .all(|x| self.resolvida(&x.missao.id))
    }

    /// Total stars in the package.
    pub fn estrelas(&self) -> u32 {
        self.missoes.values().map(|r| r.estrelas as u32).sum()
    }
}
