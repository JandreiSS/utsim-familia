//! Papel de cada camada e sugestão automática por nome.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plan::Plan;

/// Papel de uma camada na simulação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Só desenho de fundo.
    Fundo,
    /// Vira obstáculo (parede).
    Parede,
    /// Vira obstáculo (coluna).
    Coluna,
    /// Abertura nas paredes (passagem), guardada à parte.
    Porta,
    /// Não usada (cotas, hachuras, textos).
    Ignorar,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Fundo => "fundo",
            Role::Parede => "parede",
            Role::Coluna => "coluna",
            Role::Porta => "porta",
            Role::Ignorar => "ignorar",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        Some(match s.trim().to_lowercase().as_str() {
            "fundo" => Role::Fundo,
            "parede" | "paredes" => Role::Parede,
            "coluna" | "colunas" => Role::Coluna,
            "porta" | "portas" => Role::Porta,
            "ignorar" => Role::Ignorar,
            _ => return None,
        })
    }
}

fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'Á' | 'À' | 'Â' | 'Ã' => 'a',
        'é' | 'ê' | 'É' | 'Ê' => 'e',
        'í' | 'Í' => 'i',
        'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'o',
        'ú' | 'Ú' => 'u',
        'ç' | 'Ç' => 'c',
        c => c.to_ascii_lowercase(),
    }
}

/// Sugere o papel pelo nome comum da camada (português e inglês). Ordem de teste: ignorar,
/// porta, coluna, parede; o resto é fundo.
pub fn suggest_role(name: &str) -> Role {
    let folded: String = name.chars().map(fold).collect();
    let tokens: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let has = |words: &[&str]| tokens.iter().any(|t| words.contains(t));
    // prefixos (aceita plural e sufixos numéricos: "parede2", "walls")
    let starts = |words: &[&str]| {
        tokens.iter().any(|t| {
            let t = t.trim_end_matches(|c: char| c.is_ascii_digit());
            words.iter().any(|w| t.starts_with(w))
        })
    };
    if has(&["defpoints"])
        || starts(&[
            "cota", "dim", "hatch", "hachura", "texto", "text", "anno", "nota", "grid", "eixo",
            "legenda", "carimbo", "viewport",
        ])
    {
        Role::Ignorar
    } else if starts(&["porta", "door", "portao"]) {
        Role::Porta
    } else if starts(&["coluna", "pilar", "column", "pillar"]) {
        Role::Coluna
    } else if starts(&["parede", "wall", "alvenaria", "muro"]) {
        Role::Parede
    } else {
        Role::Fundo
    }
}

/// Papel por camada: sugestão automática com sobrescrita do usuário.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RoleMap {
    roles: BTreeMap<String, Role>,
}

impl RoleMap {
    /// Mapa com a sugestão automática para todas as camadas da planta.
    pub fn suggest(plan: &Plan) -> Self {
        RoleMap {
            roles: plan
                .layers
                .iter()
                .map(|l| (l.name.clone(), suggest_role(&l.name)))
                .collect(),
        }
    }

    /// Sobrescreve o papel de uma camada.
    pub fn set(&mut self, layer: &str, role: Role) {
        self.roles.insert(layer.to_string(), role);
    }

    /// Papel da camada (camada desconhecida: sugestão pelo nome).
    pub fn role_of(&self, layer: &str) -> Role {
        self.roles
            .get(layer)
            .copied()
            .unwrap_or_else(|| suggest_role(layer))
    }

    /// Pares (camada, papel) em ordem de nome.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Role)> {
        self.roles.iter().map(|(k, v)| (k.as_str(), *v))
    }
}
