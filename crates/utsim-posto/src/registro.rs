//! Operations and the state they build. Pure: no file access.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What an operation does to one record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Acao {
    /// Writes the whole record (creates or replaces it).
    Gravar(Value),
    /// Removes the record.
    Remover,
}

/// One change, written once and never edited.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Operacao {
    /// Random id (hex), also the tie-breaker of the order.
    pub id: String,
    pub autor: String,
    /// Milliseconds since 1970 on the author's computer.
    pub instante_ms: i64,
    /// Counter of the author's operations in this session (orders two operations of the
    /// same millisecond).
    pub seq: u64,
    /// Collection (`postos`, `perigos`...) and record id.
    pub colecao: String,
    pub registro: String,
    pub acao: Acao,
}

impl Operacao {
    /// The fixed order of replay.
    pub fn chave(&self) -> (i64, &str, u64, &str) {
        (self.instante_ms, &self.autor, self.seq, &self.id)
    }
}

/// Every record, by collection and id, with the key of the operation that wrote it last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Estado {
    colecoes: BTreeMap<String, BTreeMap<String, (Value, String)>>,
    /// Ids of the operations applied.
    pub aplicadas: usize,
}

impl Estado {
    /// Replays `ops` in the fixed order (the input order does not matter).
    pub fn reproduzir<'a>(ops: impl IntoIterator<Item = &'a Operacao>) -> Estado {
        let mut v: Vec<&Operacao> = ops.into_iter().collect();
        v.sort_by(|a, b| a.chave().cmp(&b.chave()));
        v.dedup_by(|a, b| a.id == b.id);
        let mut e = Estado::default();
        for op in v {
            e.aplicar(op);
        }
        e
    }

    fn aplicar(&mut self, op: &Operacao) {
        let col = self.colecoes.entry(op.colecao.clone()).or_default();
        match &op.acao {
            Acao::Gravar(v) => {
                col.insert(op.registro.clone(), (v.clone(), op.autor.clone()));
            }
            Acao::Remover => {
                col.remove(&op.registro);
            }
        }
        self.aplicadas += 1;
    }

    /// Raw records of a collection, by id.
    pub fn brutos(&self, colecao: &str) -> impl Iterator<Item = (&String, &Value)> {
        self.colecoes
            .get(colecao)
            .into_iter()
            .flat_map(|c| c.iter().map(|(k, (v, _))| (k, v)))
    }

    /// Records of a collection read as `T`, in id order. Records that do not read (a newer
    /// version of the app wrote them) are skipped.
    pub fn lista<T: DeserializeOwned>(&self, colecao: &str) -> Vec<T> {
        self.brutos(colecao)
            .filter_map(|(_, v)| serde_json::from_value(v.clone()).ok())
            .collect()
    }

    pub fn um<T: DeserializeOwned>(&self, colecao: &str, id: &str) -> Option<T> {
        let v = self.colecoes.get(colecao)?.get(id)?;
        serde_json::from_value(v.0.clone()).ok()
    }

    /// Who wrote the record last.
    pub fn autor(&self, colecao: &str, id: &str) -> Option<&str> {
        Some(self.colecoes.get(colecao)?.get(id)?.1.as_str())
    }

    pub fn conta(&self, colecao: &str) -> usize {
        self.colecoes.get(colecao).map_or(0, BTreeMap::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: &str, autor: &str, t: i64, reg: &str, acao: Acao) -> Operacao {
        Operacao {
            id: id.into(),
            autor: autor.into(),
            instante_ms: t,
            seq: 0,
            colecao: "postos".into(),
            registro: reg.into(),
            acao,
        }
    }

    #[test]
    fn order_of_arrival_does_not_matter() {
        let a = op(
            "a",
            "ana",
            10,
            "p1",
            Acao::Gravar(serde_json::json!({"nome": "A"})),
        );
        let b = op(
            "b",
            "bia",
            20,
            "p1",
            Acao::Gravar(serde_json::json!({"nome": "B"})),
        );
        let c = op(
            "c",
            "ana",
            30,
            "p2",
            Acao::Gravar(serde_json::json!({"nome": "C"})),
        );
        let d = op("d", "bia", 40, "p2", Acao::Remover);
        let x = Estado::reproduzir([&a, &b, &c, &d]);
        let y = Estado::reproduzir([&d, &c, &b, &a, &b]);
        assert_eq!(x.colecoes, y.colecoes);
        assert_eq!(x.conta("postos"), 1);
        assert_eq!(x.autor("postos", "p1"), Some("bia"));
    }
}
