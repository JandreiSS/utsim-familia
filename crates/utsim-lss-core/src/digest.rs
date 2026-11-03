//! SHA-256 of JSON content, for the frozen hash of a tollgate
//! (`docs/plano_lean_seis_sigma.md`, 1.4, step 5). The content is a sorted map, so the same
//! content gives the same hash on every machine.

use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Hex SHA-256 of the canonical JSON of a sorted map.
pub fn hash_map(content: &BTreeMap<String, Value>) -> String {
    let text = serde_json::to_string(content).expect("JSON map");
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// One difference between two contents.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub key: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// Keys added, removed or changed from `before` to `after`, in key order.
pub fn diff(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Vec<Change> {
    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|k| {
            let (b, a) = (before.get(k), after.get(k));
            (b != a).then(|| Change {
                key: k.clone(),
                before: b.cloned(),
                after: a.cloned(),
            })
        })
        .collect()
}

/// A JSON value as text for the screen (strings without quotes).
pub fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A JSON object as a sorted map (anything else: empty).
pub fn as_map(v: Option<&Value>) -> BTreeMap<String, Value> {
    v.and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// A sorted map as a JSON object.
pub fn to_value(m: &BTreeMap<String, Value>) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_diff_lists_changes() {
        let mut a = BTreeMap::new();
        a.insert("x/titulo".to_string(), Value::from("A"));
        a.insert("y/nota".to_string(), Value::from(3));
        let h = hash_map(&a);
        assert_eq!(h.len(), 64);
        assert_eq!(h, hash_map(&a.clone()));
        let mut b = a.clone();
        b.insert("x/titulo".into(), Value::from("B"));
        b.remove("y/nota");
        b.insert("z/novo".into(), Value::Bool(true));
        assert_ne!(hash_map(&b), h);
        let d = diff(&a, &b);
        let keys: Vec<&str> = d.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["x/titulo", "y/nota", "z/novo"]);
        assert_eq!(d[1].after, None);
        assert_eq!(show(&Value::from("t")), "t");
        assert_eq!(show(&Value::from(2)), "2");
        assert_eq!(as_map(Some(&to_value(&b))), b);
    }
}
