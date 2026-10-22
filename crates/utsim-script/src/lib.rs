//! User expressions and scripts (phase 6), written in the Rhai language.
//!
//! Expressions appear in token flows (labels to assign, decision conditions); object
//! scripts run when items enter a block (`on_entry`) or choose an output port
//! (`OutputRule::Script`).
//!
//! Scopes:
//! - expressions: `now`, `token` (map), `item` (map, or `()` when the token carries none);
//! - `on_entry`: `item` (mutable map, written back), `now`, `block`;
//! - port scripts: `item` (read-only copy), `outputs`, `now`, `block`; the result is a
//!   1-based port number.
//!
//! Functions: `uniform(a, b)`, `exponential(mean)`, `normal(mean, sd)`,
//! `triangular(min, mode, max)`, `random()` (all draw from the owning block's stream, so
//! runs stay reproducible) and `label(map, name, default)`.
//!
//! Safety: one shared, sandboxed engine (no I/O, no clock, no modules, `print` discarded)
//! with limits on operations, depth and sizes; scripts are compiled once to an AST.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use rhai::{
    AST, Dynamic, Engine, EvalAltResult, ImmutableString, Map, ParseError, Position, Scope,
};
use utsim_core::rng::SimRng;
use utsim_core::script::{ItemScript, PortScript, ScriptEnv};
use utsim_core::{Dist, Item, Time, Value};

/// Maximum operations a single script call may perform.
pub const MAX_OPERATIONS: u64 = 100_000;

/// What an expression can see while it is evaluated.
pub struct ExprEnv<'a> {
    pub now: Time,
    /// The owning block's random stream (keeps runs reproducible).
    pub rng: &'a mut SimRng,
    pub token: &'a BTreeMap<String, Value>,
    /// Labels of the item the token carries, if any.
    pub item: Option<&'a BTreeMap<String, Value>>,
}

// ---------------------------------------------------------------- engine and rng

thread_local! {
    /// The stream of the call in progress (copied in and out around each evaluation).
    static RNG: RefCell<Option<SimRng>> = const { RefCell::new(None) };
}

/// Lends a block's stream to the registered random functions for one call.
struct RngGuard<'a>(&'a mut SimRng);

impl<'a> RngGuard<'a> {
    fn new(rng: &'a mut SimRng) -> Self {
        RNG.with(|c| *c.borrow_mut() = Some(rng.clone()));
        RngGuard(rng)
    }
}

impl Drop for RngGuard<'_> {
    fn drop(&mut self) {
        if let Some(r) = RNG.with(|c| c.borrow_mut().take()) {
            *self.0 = r;
        }
    }
}

fn draw(dist: Dist) -> Result<f64, Box<EvalAltResult>> {
    RNG.with(|c| match c.borrow_mut().as_mut() {
        Some(rng) => Ok(dist.sample(rng)),
        None => Err("sorteio fora de uma execução".into()),
    })
}

fn num(name: &str, v: &Dynamic) -> Result<f64, Box<EvalAltResult>> {
    if let Ok(i) = v.as_int() {
        Ok(i as f64)
    } else if let Ok(f) = v.as_float() {
        Ok(f)
    } else {
        Err(format!(
            "'{name}': os argumentos devem ser números, mas recebeu {}",
            v.type_name()
        )
        .into())
    }
}

fn engine() -> &'static Arc<Engine> {
    static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut e = Engine::new();
        e.set_max_operations(MAX_OPERATIONS);
        e.set_max_expr_depths(64, 32);
        e.set_max_call_levels(32);
        e.set_max_string_size(10_000);
        e.set_max_array_size(1_000);
        e.set_max_map_size(1_000);
        e.set_fail_on_invalid_map_property(true);
        e.disable_symbol("eval");
        e.on_print(|_| {});
        e.on_debug(|_, _, _| {});

        e.register_fn("random", || draw(Dist::Uniform { min: 0.0, max: 1.0 }));
        e.register_fn("uniform", |a: Dynamic, b: Dynamic| {
            let (min, max) = (num("uniform", &a)?, num("uniform", &b)?);
            if min > max {
                return Err("uniform(a, b): exige a <= b".into());
            }
            draw(Dist::Uniform { min, max })
        });
        e.register_fn("exponential", |m: Dynamic| {
            let mean = num("exponential", &m)?;
            if mean <= 0.0 {
                return Err("exponential(média): a média deve ser positiva".into());
            }
            draw(Dist::Exp { mean })
        });
        e.register_fn("normal", |m: Dynamic, s: Dynamic| {
            let (mean, sd) = (num("normal", &m)?, num("normal", &s)?);
            if sd < 0.0 {
                return Err("normal(média, dp): o desvio-padrão não pode ser negativo".into());
            }
            draw(Dist::Normal { mean, sd })
        });
        e.register_fn("triangular", |a: Dynamic, m: Dynamic, b: Dynamic| {
            let (min, mode, max) = (
                num("triangular", &a)?,
                num("triangular", &m)?,
                num("triangular", &b)?,
            );
            if !(min <= mode && mode <= max) {
                return Err("triangular(mín, moda, máx): exige mín <= moda <= máx".into());
            }
            draw(Dist::Triangular { min, mode, max })
        });
        e.register_fn("label", |map: &mut Map, name: &str, default: Dynamic| {
            map.get(name).cloned().unwrap_or(default)
        });
        Arc::new(e)
    })
}

// ---------------------------------------------------------------- conversions

fn to_dynamic(v: &Value) -> Dynamic {
    match v {
        Value::Num(n) => Dynamic::from_float(*n),
        Value::Text(s) => Dynamic::from(ImmutableString::from(s.as_str())),
    }
}

fn to_map(labels: &BTreeMap<String, Value>) -> Map {
    labels
        .iter()
        .map(|(k, v)| (k.as_str().into(), to_dynamic(v)))
        .collect()
}

fn type_pt(d: &Dynamic) -> String {
    if d.is_unit() {
        "nada (sem valor)".into()
    } else {
        format!("'{}'", d.type_name())
    }
}

fn to_value(d: &Dynamic) -> Result<Value, String> {
    if let Ok(i) = d.as_int() {
        Ok(Value::Num(i as f64))
    } else if let Ok(f) = d.as_float() {
        Ok(Value::Num(f))
    } else if let Ok(b) = d.as_bool() {
        Ok(Value::Num(if b { 1.0 } else { 0.0 }))
    } else if d.is_string() {
        Ok(Value::Text(d.clone().into_string().unwrap_or_default()))
    } else {
        Err(format!(
            "o resultado é {}; esperado número, texto ou lógico",
            type_pt(d)
        ))
    }
}

fn write_back(map: Map, item: &mut Item) -> Result<(), String> {
    let kept = map.len();
    for (k, v) in map.iter() {
        let value = to_value(v).map_err(|e| format!("rótulo '{k}': {e}"))?;
        match item.labels.get_mut(k.as_str()) {
            Some(slot) => {
                if *slot != value {
                    *slot = value;
                }
            }
            None => {
                item.labels.insert(k.to_string(), value);
            }
        }
    }
    if item.labels.len() != kept {
        // The script removed some labels (`item.remove(..)`): drop the ones that are gone.
        item.labels.retain(|k, _| map.contains_key(k.as_str()));
    }
    Ok(())
}

// ---------------------------------------------------------------- errors

fn parse_error(source: &str, e: &ParseError) -> String {
    let pos = e.1;
    let at = if pos.is_none() {
        String::new()
    } else {
        format!(
            "linha {}, coluna {}: ",
            pos.line().unwrap_or(1),
            pos.position().unwrap_or(1)
        )
    };
    format!(
        "erro de sintaxe em {at}{}\n  código: {}",
        e.0,
        source.trim()
    )
}

fn at(pos: Position) -> String {
    if pos.is_none() {
        String::new()
    } else {
        format!(
            " (linha {}, coluna {})",
            pos.line().unwrap_or(1),
            pos.position().unwrap_or(1)
        )
    }
}

fn run_error(e: &EvalAltResult) -> String {
    use EvalAltResult as R;
    match e {
        R::ErrorPropertyNotFound(name, p) => format!("rótulo '{name}' não existe{}", at(*p)),
        R::ErrorVariableNotFound(name, p) => format!("variável '{name}' não existe{}", at(*p)),
        R::ErrorTooManyOperations(p) => format!(
            "o script excedeu o limite de {MAX_OPERATIONS} operações (laço infinito?){}",
            at(*p)
        ),
        R::ErrorStackOverflow(p) => format!("recursão profunda demais{}", at(*p)),
        R::ErrorDataTooLarge(what, p) => format!("{what} grande demais{}", at(*p)),
        R::ErrorFunctionNotFound(sig, p) => format!("função desconhecida: {sig}{}", at(*p)),
        R::ErrorRuntime(v, p) => format!("{v}{}", at(*p)),
        R::ErrorInFunctionCall(name, _, inner, _) => {
            format!("na função '{name}': {}", run_error(inner))
        }
        other => format!("erro ao executar o script: {other}"),
    }
}

// ---------------------------------------------------------------- expressions

/// A compiled expression.
#[derive(Clone)]
pub struct Expr {
    source: String,
    ast: Arc<AST>,
}

impl std::fmt::Debug for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Expr")
            .field("source", &self.source)
            .finish()
    }
}

impl Expr {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn eval(&self, env: &mut ExprEnv) -> Result<Value, String> {
        let mut scope = Scope::new();
        scope.push_constant("now", env.now);
        scope.push_constant("token", to_map(env.token));
        scope.push_constant(
            "item",
            env.item
                .map_or(Dynamic::UNIT, |m| Dynamic::from_map(to_map(m))),
        );
        let guard = RngGuard::new(env.rng);
        let result = engine().eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast);
        drop(guard);
        match result {
            Ok(d) => to_value(&d).map_err(|e| format!("'{}': {e}", self.source.trim())),
            Err(e) => {
                if env.item.is_none()
                    && matches!(
                        *e,
                        EvalAltResult::ErrorPropertyNotFound(..) | EvalAltResult::ErrorDotExpr(..)
                    )
                    && self.source.contains("item")
                {
                    return Err(format!(
                        "'{}': o token não carrega um item",
                        self.source.trim()
                    ));
                }
                Err(format!("'{}': {}", self.source.trim(), run_error(&e)))
            }
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
    let ast = engine()
        .compile_expression(source)
        .map_err(|e| parse_error(source, &e))?;
    Ok(Expr {
        source: source.to_string(),
        ast: Arc::new(ast),
    })
}

// ---------------------------------------------------------------- object scripts

fn block_name(cache: &mut Option<ImmutableString>, name: &str) -> ImmutableString {
    match cache {
        Some(c) if c.as_str() == name => c.clone(),
        _ => {
            let s = ImmutableString::from(name);
            *cache = Some(s.clone());
            s
        }
    }
}

struct RhaiItemScript {
    source: String,
    ast: AST,
    block: Option<ImmutableString>,
    scope: Scope<'static>,
}

impl ItemScript for RhaiItemScript {
    fn run(&mut self, item: &mut Item, env: &mut ScriptEnv) -> Result<(), String> {
        let mut scope = std::mem::take(&mut self.scope);
        scope.clear();
        scope.push_constant("now", env.now);
        scope.push_constant("block", block_name(&mut self.block, env.block));
        scope.push("item", to_map(&item.labels));
        let guard = RngGuard::new(env.rng);
        let result = engine().run_ast_with_scope(&mut scope, &self.ast);
        drop(guard);
        let map = scope.remove::<Map>("item");
        self.scope = scope;
        let fail = |m: String| {
            format!(
                "on_entry de '{}': {m}\n  código: {}",
                env.block,
                self.source.trim()
            )
        };
        result.map_err(|e| fail(run_error(&e)))?;
        match map {
            Some(map) => write_back(map, item).map_err(fail),
            None => Err(fail("a variável 'item' deixou de ser um mapa".into())),
        }
    }
}

/// Compiles an `on_entry` script for an object.
pub fn compile_item_script(source: &str) -> Result<Box<dyn ItemScript>, String> {
    let ast = engine()
        .compile(source)
        .map_err(|e| parse_error(source, &e))?;
    Ok(Box::new(RhaiItemScript {
        source: source.to_string(),
        ast,
        block: None,
        scope: Scope::new(),
    }))
}

struct RhaiPortScript {
    source: String,
    ast: AST,
    block: Option<ImmutableString>,
    scope: Scope<'static>,
}

impl PortScript for RhaiPortScript {
    fn port(&mut self, item: &Item, outputs: usize, env: &mut ScriptEnv) -> Result<usize, String> {
        let mut scope = std::mem::take(&mut self.scope);
        scope.clear();
        scope.push_constant("now", env.now);
        scope.push_constant("block", block_name(&mut self.block, env.block));
        scope.push_constant("outputs", outputs as i64);
        scope.push_constant("item", to_map(&item.labels));
        let guard = RngGuard::new(env.rng);
        let result = engine().eval_ast_with_scope::<Dynamic>(&mut scope, &self.ast);
        drop(guard);
        self.scope = scope;
        let fail = |m: String| {
            format!(
                "regra de saída de '{}': {m}\n  código: {}",
                env.block,
                self.source.trim()
            )
        };
        let d = result.map_err(|e| fail(run_error(&e)))?;
        let port = if let Ok(i) = d.as_int() {
            i
        } else if let Ok(f) = d.as_float() {
            if f.fract() != 0.0 || !f.is_finite() {
                return Err(fail(format!("a porta deve ser um inteiro, mas veio {f}")));
            }
            f as i64
        } else {
            return Err(fail(format!(
                "o script deve devolver o número da porta (inteiro), mas devolveu {}",
                type_pt(&d)
            )));
        };
        if port < 1 || port as usize > outputs {
            return Err(fail(format!(
                "porta {port} inválida: o bloco tem {outputs} saída(s), numeradas a partir de 1"
            )));
        }
        Ok(port as usize - 1)
    }
}

/// Compiles an `OutputRule::Script` port chooser (the script returns a 1-based port).
pub fn compile_port_script(source: &str) -> Result<Box<dyn PortScript>, String> {
    let ast = engine()
        .compile(source)
        .map_err(|e| parse_error(source, &e))?;
    Ok(Box::new(RhaiPortScript {
        source: source.to_string(),
        ast,
        block: None,
        scope: Scope::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::rng::stream;

    fn eval_with(src: &str, rng: &mut SimRng) -> Result<Value, String> {
        let token = BTreeMap::from([("tipo".to_string(), Value::Num(2.0))]);
        let item = BTreeMap::from([("cor".to_string(), Value::from("azul"))]);
        let mut env = ExprEnv {
            now: 7.5,
            rng,
            token: &token,
            item: Some(&item),
        };
        compile_expr(src)?.eval(&mut env)
    }

    fn eval(src: &str) -> Result<Value, String> {
        eval_with(src, &mut stream(1, 0, 0))
    }

    fn blank_item() -> Item {
        Item {
            labels: BTreeMap::new(),
            created_at: 0.0,
            entered_at: 0.0,
            location: None,
            container: None,
            contents: Vec::new(),
            awaiting_transport: false,
        }
    }

    #[test]
    fn reference_expressions_keep_working() {
        assert_eq!(eval("3"), Ok(Value::Num(3.0)));
        assert_eq!(eval("3.5"), Ok(Value::Num(3.5)));
        assert_eq!(eval("now"), Ok(Value::Num(7.5)));
        assert_eq!(eval("token.tipo"), Ok(Value::Num(2.0)));
        assert_eq!(eval("item.cor"), Ok(Value::from("azul")));
        assert_eq!(eval("\"A\""), Ok(Value::from("A")));
        assert_eq!(eval("true"), Ok(Value::Num(1.0)));
        assert!(eval("token.nada").unwrap_err().contains("não existe"));
        assert!(compile_expr("2 +").is_err());
    }

    #[test]
    fn math_and_conditionals() {
        assert_eq!(eval("token.tipo * 2 + now"), Ok(Value::Num(11.5)));
        assert_eq!(
            eval("if item.cor == \"azul\" { 10 } else { 20 }"),
            Ok(Value::Num(10.0))
        );
        assert_eq!(eval("label(item, \"x\", 5)"), Ok(Value::Num(5.0)));
        assert_eq!(eval("token.tipo == 2"), Ok(Value::Num(1.0)));
    }

    #[test]
    fn random_is_deterministic_and_uses_the_stream() {
        let a = eval("uniform(1, 2) + exponential(3.0) + normal(0, 1)").unwrap();
        let b = eval("uniform(1, 2) + exponential(3.0) + normal(0, 1)").unwrap();
        assert_eq!(a, b);
        let mut rng = stream(1, 0, 0);
        let first = eval_with("random()", &mut rng).unwrap();
        let second = eval_with("random()", &mut rng).unwrap();
        assert_ne!(first, second, "the stream advances between calls");
        let c = eval_with("random()", &mut stream(1, 0, 1)).unwrap();
        assert_ne!(first, c, "different blocks use different streams");
    }

    #[test]
    fn bad_results_and_args_are_reported() {
        assert!(eval("[1, 2]").unwrap_err().contains("esperado número"));
        assert!(eval("uniform(\"a\", 2)").unwrap_err().contains("números"));
        assert!(eval("triangular(3, 2, 1)").unwrap_err().contains("exige"));
    }

    #[test]
    fn item_script_writes_labels_back() {
        let mut s = compile_item_script("item.dobro = now * 2; item.nome = \"x\";").unwrap();
        let mut item = blank_item();
        let mut rng = stream(1, 0, 0);
        let mut env = ScriptEnv {
            now: 4.0,
            rng: &mut rng,
            block: "b",
        };
        s.run(&mut item, &mut env).unwrap();
        assert_eq!(item.label("dobro"), Some(&Value::Num(8.0)));
        assert_eq!(item.label("nome"), Some(&Value::from("x")));
    }

    #[test]
    fn port_script_is_one_based() {
        let mut s = compile_port_script("if item.p > 1 { 2 } else { 1 }").unwrap();
        let mut item = blank_item();
        item.set_label("p", 5.0);
        let mut rng = stream(1, 0, 0);
        let mut env = ScriptEnv {
            now: 0.0,
            rng: &mut rng,
            block: "b",
        };
        assert_eq!(s.port(&item, 2, &mut env), Ok(1));
        item.set_label("p", 0.0);
        assert_eq!(s.port(&item, 2, &mut env), Ok(0));
        let mut bad = compile_port_script("3").unwrap();
        assert!(
            bad.port(&item, 2, &mut env)
                .unwrap_err()
                .contains("inválida")
        );
    }

    #[test]
    fn operation_limit_stops_runaway_scripts() {
        let mut s = compile_item_script("loop { }").unwrap();
        let mut item = blank_item();
        let mut rng = stream(1, 0, 0);
        let mut env = ScriptEnv {
            now: 0.0,
            rng: &mut rng,
            block: "b",
        };
        let err = s.run(&mut item, &mut env).unwrap_err();
        assert!(err.contains("limite"), "{err}");
    }

    #[test]
    fn compile_errors_have_position() {
        let err = compile_item_script("let x = 1;\nlet y = ;").err().unwrap();
        assert!(err.contains("linha 2"), "{err}");
        assert!(err.contains("let y"), "{err}");
    }
}
