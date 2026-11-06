//! The student's variant: each `Faixa` of a mission drawn from the seed
//! `hash(missao, aluno)`. The same student always gets the same numbers; another student
//! gets others, so a colleague's good decision is rarely the best one.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::missao::{Faixa, Missao, arredondar, numero_ron};

/// The seed of `aluno` in mission `missao` (an empty `aluno` means no variant).
pub fn semente(missao: &str, aluno: &str) -> u64 {
    let mut h = Sha256::new();
    h.update(b"utsim-ensino/variante/1\0");
    h.update(missao.as_bytes());
    h.update([0]);
    h.update(aluno.as_bytes());
    let d = h.finalize();
    u64::from_le_bytes(d[..8].try_into().expect("8 bytes"))
}

/// SplitMix64: small, portable and the same on every machine.
fn proximo(estado: &mut u64) -> u64 {
    *estado = estado.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *estado;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The drawn numbers, one per range, in the order of the mission.
pub fn valores(missao: &Missao, aluno: &str) -> Vec<f64> {
    if aluno.is_empty() {
        // No student (calibration, golden): the middle of each range.
        return missao
            .variante
            .iter()
            .map(|f| arredondar((f.de + f.ate) / 2.0, f.casas))
            .collect();
    }
    let mut estado = semente(&missao.id, aluno);
    missao
        .variante
        .iter()
        .map(|f| {
            let u = (proximo(&mut estado) >> 11) as f64 / (1u64 << 53) as f64;
            arredondar(f.de + u * (f.ate - f.de), f.casas)
        })
        .collect()
}

/// The model keys of the variant of `aluno`.
pub fn ajustes(missao: &Missao, aluno: &str) -> BTreeMap<String, String> {
    missao
        .variante
        .iter()
        .zip(valores(missao, aluno))
        .map(|(f, v)| (f.chave.clone(), texto_faixa(f, v)))
        .collect()
}

/// The value text of range `f` at `v`.
pub fn texto_faixa(f: &Faixa, v: f64) -> String {
    let n = numero_ron(v);
    match &f.modelo {
        Some(m) => m.replace("{}", &n),
        None => n,
    }
}
