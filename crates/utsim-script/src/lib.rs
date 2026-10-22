//! User expressions and scripts (phase 6, contract).
//!
//! Expressions appear in token flows (labels to assign, decision conditions); object
//! scripts run when items enter a block (`on_entry`) or choose an output port
//! (`OutputRule::Script`). The scripts front replaces this reference implementation
//! with the Rhai language; the public API below stays.
//!
//! Reference grammar until then: a number (`3.5`), a quoted string (`"A"`), `true`,
//! `false`, `now`, `token.<label>` or `item.<label>`.

use std::collections::BTreeMap;

use utsim_core::rng::SimRng;
use utsim_core::script::{ItemScript, PortScript};
use utsim_core::{Time, Value};

/// What an expression can see while it is evaluated.
pub struct ExprEnv<'a> {
    pub now: Time,
    /// The owning block's random stream (keeps runs reproducible).
    pub rng: &'a mut SimRng,
    pub token: &'a BTreeMap<String, Value>,
    /// Labels of the item the token carries, if any.
    pub item: Option<&'a BTreeMap<String, Value>>,
}

/// A compiled expression.
#[derive(Clone, Debug)]
pub struct Expr {
    source: String,
    kind: Kind,
}

#[derive(Clone, Debug)]
enum Kind {
    Const(Value),
    Bool(bool),
    Now,
    Token(String),
    Item(String),
}

impl Expr {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn eval(&self, env: &mut ExprEnv) -> Result<Value, String> {
        let missing = |scope: &str, name: &str| format!("rótulo '{scope}.{name}' não existe");
        match &self.kind {
            Kind::Const(v) => Ok(v.clone()),
            Kind::Bool(b) => Ok(Value::Num(if *b { 1.0 } else { 0.0 })),
            Kind::Now => Ok(Value::Num(env.now)),
            Kind::Token(name) => env
                .token
                .get(name)
                .cloned()
                .ok_or_else(|| missing("token", name)),
            Kind::Item(name) => env
                .item
                .ok_or_else(|| "o token não carrega um item".to_string())?
                .get(name)
                .cloned()
                .ok_or_else(|| missing("item", name)),
        }
    }

    /// Truth value: non-zero numbers and non-empty strings are true.
    pub fn eval_bool(&self, env: &mut ExprEnv) -> Result<bool, String> {
        Ok(match self.eval(env)? {
            Value::Num(v) => v != 0.0,
            Value::Text(s) => !s.is_empty(),
        })
    }
}

/// Compiles an expression; the error message is shown to the modeller.
pub fn compile_expr(source: &str) -> Result<Expr, String> {
    let s = source.trim();
    let kind = if let Ok(v) = s.parse::<f64>() {
        Kind::Const(Value::Num(v))
    } else if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        Kind::Const(Value::Text(s[1..s.len() - 1].to_string()))
    } else if s == "true" || s == "false" {
        Kind::Bool(s == "true")
    } else if s == "now" {
        Kind::Now
    } else if let Some(name) = s.strip_prefix("token.") {
        Kind::Token(name.to_string())
    } else if let Some(name) = s.strip_prefix("item.") {
        Kind::Item(name.to_string())
    } else {
        return Err(format!("expressão não reconhecida: '{s}'"));
    };
    Ok(Expr {
        source: source.to_string(),
        kind,
    })
}

/// Compiles an `on_entry` script for an object.
pub fn compile_item_script(source: &str) -> Result<Box<dyn ItemScript>, String> {
    let _ = source;
    Err("scripts de objeto (on_entry): em construção na fase 6".into())
}

/// Compiles an `OutputRule::Script` port chooser.
pub fn compile_port_script(source: &str) -> Result<Box<dyn PortScript>, String> {
    let _ = source;
    Err("regra de saída por script: em construção na fase 6".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::rng::stream;

    #[test]
    fn reference_expressions() {
        let mut rng = stream(1, 0, 0);
        let token = BTreeMap::from([("tipo".to_string(), Value::Num(2.0))]);
        let item = BTreeMap::from([("cor".to_string(), Value::from("azul"))]);
        let mut env = ExprEnv {
            now: 7.5,
            rng: &mut rng,
            token: &token,
            item: Some(&item),
        };
        assert_eq!(
            compile_expr("3").unwrap().eval(&mut env),
            Ok(Value::Num(3.0))
        );
        assert_eq!(
            compile_expr("now").unwrap().eval(&mut env),
            Ok(Value::Num(7.5))
        );
        assert_eq!(
            compile_expr("token.tipo").unwrap().eval(&mut env),
            Ok(Value::Num(2.0))
        );
        assert_eq!(
            compile_expr("item.cor").unwrap().eval(&mut env),
            Ok(Value::from("azul"))
        );
        assert!(compile_expr("true").unwrap().eval_bool(&mut env).unwrap());
        assert!(compile_expr("token.nada").unwrap().eval(&mut env).is_err());
        assert!(compile_expr("2 +").is_err());
    }
}
