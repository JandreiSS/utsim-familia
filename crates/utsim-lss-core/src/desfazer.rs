//! Desfazer e refazer num diário: nada é apagado. Desfazer um passo é uma edição nova, a
//! compensatória, que devolve cada campo ao valor de antes do passo (e apaga as entidades
//! que o passo criou); ela entra no diário, assinada, como qualquer outra. Refazer é a
//! compensação do desfazer.
//!
//! O app guarda, a cada passo, a compensação calculada **antes** de gravar
//! ([`compensar`]), e na hora de desfazer grava essa lista.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::Id;
use crate::op::{Edit, FIELD_DELETED};
use crate::state::State;

/// As edições que devolvem o estado de agora depois de `edits` serem gravadas: para cada
/// campo, o valor de antes (vazio quando ele não existia); para cada entidade que não
/// existia, a marca de apagada. Na ordem inversa, para o último a mudar voltar primeiro.
/// Os campos de controle (começam com `$`) ficam de fora.
pub fn compensar(state: &State, edits: &[Edit]) -> Vec<Edit> {
    let mut novas: BTreeSet<Id> = BTreeSet::new();
    let mut vistos: BTreeSet<(Id, String)> = BTreeSet::new();
    let mut out = Vec::new();
    for e in edits.iter().rev() {
        if e.field.starts_with('$') {
            continue;
        }
        let id = e.entity.id;
        match state.entity(id) {
            None => {
                if novas.insert(id) {
                    out.push(Edit::new(e.entity, FIELD_DELETED, true));
                }
            }
            Some(ent) => {
                if !vistos.insert((id, e.field.clone())) {
                    continue;
                }
                let antes = ent.get(&e.field).cloned().unwrap_or(Value::Null);
                out.push(Edit::new(e.entity, e.field.clone(), antes));
            }
        }
    }
    // A mesma edição repetida no passo volta ao valor de antes do passo (o primeiro): a
    // varredura de trás para a frente já pegou o estado de antes de qualquer uma delas.
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fields;
    use crate::op::{EntityRef, Kind};
    use crate::tools::testing::Fixture;

    fn texto(f: &Fixture, id: Id, campo: &str) -> String {
        f.state
            .entity(id)
            .map(|e| e.text(campo))
            .unwrap_or_default()
    }

    #[test]
    fn desfazer_devolve_o_valor_e_refazer_traz_de_novo() {
        let mut f = Fixture::new();
        let p = EntityRef::new(Kind::Project, f.project);
        let antes = texto(&f, f.project, fields::project::PROBLEM);
        let passo = vec![Edit::new(p, fields::project::PROBLEM, "Refugo alto")];
        let desfaz = compensar(&f.state, &passo);
        f.commit(passo);
        assert_eq!(
            texto(&f, f.project, fields::project::PROBLEM),
            "Refugo alto"
        );
        let refaz = compensar(&f.state, &desfaz);
        f.commit(desfaz);
        assert_eq!(texto(&f, f.project, fields::project::PROBLEM), antes);
        f.commit(refaz);
        assert_eq!(
            texto(&f, f.project, fields::project::PROBLEM),
            "Refugo alto"
        );
    }

    #[test]
    fn desfazer_uma_criacao_apaga_e_refazer_traz_de_volta() {
        let mut f = Fixture::new();
        let r = f.id();
        let risco = EntityRef::new(Kind::Risk, r);
        let passo = vec![
            Edit::new(risco, "projeto", f.project.to_string()),
            Edit::new(risco, "texto", "Fornecedor atrasa"),
        ];
        let desfaz = compensar(&f.state, &passo);
        assert_eq!(desfaz.len(), 1, "uma marca de apagada para a entidade nova");
        f.commit(passo);
        let refaz = compensar(&f.state, &desfaz);
        f.commit(desfaz);
        assert!(f.state.entity(r).unwrap().is_deleted());
        f.commit(refaz);
        assert!(!f.state.entity(r).unwrap().is_deleted());
        assert_eq!(texto(&f, r, "texto"), "Fornecedor atrasa");
    }

    #[test]
    fn desfazer_um_apagar_restaura() {
        let mut f = Fixture::new();
        let p = EntityRef::new(Kind::Project, f.project);
        let passo = vec![Edit::new(p, FIELD_DELETED, true)];
        let desfaz = compensar(&f.state, &passo);
        f.commit(passo);
        assert!(f.state.entity(f.project).unwrap().is_deleted());
        f.commit(desfaz);
        assert!(!f.state.entity(f.project).unwrap().is_deleted());
    }
}
