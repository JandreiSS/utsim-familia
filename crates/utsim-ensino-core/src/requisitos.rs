//! What a package needs (`requer`, `docs/plano_ensino_e_jogo.md`, 6.6 and EN12).
//!
//! A need is `nome [op versao]`:
//! - `nucleo >= 0.3`: the version of the simulation core this app was built with.
//! - `linha:lean_office`: a built-in line or cross-cutting pack of `utsim-linhas`
//!   (`lean_office`, `servicos`, `logistica`, `qualidade`, `seguranca`).
//!
//! A need this version does not meet is a warning, never an error: the package opens and
//! the app says what is missing, so a package made for a newer version still works where
//! it can.

/// The version of the simulation core (the workspace version).
pub const NUCLEO: &str = env!("CARGO_PKG_VERSION");

/// A need that this version does not meet, with the reason in Portuguese.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Falta {
    /// The need as the package wrote it.
    pub requisito: String,
    pub razao: String,
}

impl std::fmt::Display for Falta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.requisito, self.razao)
    }
}

/// The needs of `requer` that this version does not meet.
pub fn faltando(requer: &[String]) -> Vec<Falta> {
    requer
        .iter()
        .filter_map(|r| {
            verificar(r).err().map(|razao| Falta {
                requisito: r.trim().to_string(),
                razao,
            })
        })
        .collect()
}

/// Ok when this version meets the need, else the reason.
pub fn verificar(requisito: &str) -> Result<(), String> {
    let r = requisito.trim();
    let (nome, resto) = match r.find(|c: char| c.is_whitespace() || "<>=".contains(c)) {
        Some(i) => (&r[..i], r[i..].trim()),
        None => (r, ""),
    };
    if nome.is_empty() {
        return Err("requisito vazio".into());
    }
    if let Some(linha) = nome.strip_prefix("linha:") {
        if !resto.is_empty() {
            return Err("uma linha não leva versão".into());
        }
        return match utsim_linhas::find(linha) {
            Some(_) => Ok(()),
            None => Err(format!("esta versão não tem a linha '{linha}'")),
        };
    }
    if nome != "nucleo" {
        return Err(format!("'{nome}' não é conhecido por esta versão"));
    }
    if resto.is_empty() {
        return Ok(());
    }
    let (op, versao) = operador(resto)?;
    let pedida = versao_de(versao).ok_or_else(|| format!("versão '{versao}' ilegível"))?;
    let tem = versao_de(NUCLEO).unwrap_or_default();
    let ok = match op {
        ">=" => tem >= pedida,
        ">" => tem > pedida,
        "<=" => tem <= pedida,
        "<" => tem < pedida,
        _ => tem == pedida,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("pede o núcleo {op} {versao}, esta versão tem o {NUCLEO}"))
    }
}

fn operador(s: &str) -> Result<(&'static str, &str), String> {
    for (op, nome) in [(">=", ">="), ("<=", "<="), ("==", "="), (">", ">"), ("<", "<"), ("=", "=")] {
        if let Some(v) = s.strip_prefix(op) {
            return Ok((nome, v.trim()));
        }
    }
    Err(format!("operador ilegível em '{s}' (use >=, >, =, <= ou <)"))
}

/// `0.3`, `0.3.1` or `1` as three numbers; a suffix such as `-dev.15` is ignored.
fn versao_de(s: &str) -> Option<[u64; 3]> {
    let s = s.split(['-', '+']).next()?.trim();
    if s.is_empty() {
        return None;
    }
    let mut v = [0u64; 3];
    for (i, p) in s.split('.').enumerate() {
        if i >= 3 {
            return None;
        }
        v[i] = p.parse().ok()?;
    }
    Some(v)
}
