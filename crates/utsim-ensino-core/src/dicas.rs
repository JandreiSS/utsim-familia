//! Automatic hints: they explain what happened in the attempt from the analyses of the
//! run, never the solution, so they cost no points.

use crate::missao::{Alavanca, Missao};
use crate::motor::{ErroMotor, Simulacao};
use crate::nota::{Intervalo, ResultadoMetrica, Situacao, fmt_num};

fn media(sim: &crate::motor::Cenario, chave: &str) -> Option<f64> {
    sim.metricas.get(chave).map(|xs| Intervalo::de(xs).media)
}

/// The resource with the highest utilization in a scenario.
pub fn mais_ocupado(c: &crate::motor::Cenario) -> Option<(String, f64)> {
    c.metricas
        .iter()
        .filter_map(|(k, xs)| {
            let obj = k.strip_suffix(".utilization")?;
            Some((obj.to_string(), Intervalo::de(xs).media))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Hints about the attempt of `sim`.
pub fn automaticas(
    missao: &Missao,
    sim: &Simulacao,
    metricas: &[ResultadoMetrica],
) -> Result<Vec<String>, ErroMotor> {
    let mut out = Vec::new();
    // Where the purchases went, with the utilization of the initial state.
    for a in &missao.alavancas {
        if let Alavanca::Quantidade {
            id, rotulo, chave, ..
        } = a
        {
            let Some(obj) = chave.strip_suffix(".capacity") else {
                continue;
            };
            let v = sim.aluno.decisoes.get(id).copied().unwrap_or(a.inicial());
            if v > a.inicial()
                && let Some(u) = media(&sim.base, &format!("{obj}.utilization"))
            {
                out.push(format!(
                    "Você aumentou \"{rotulo}\". No estado inicial, a utilização desse recurso era {}.",
                    fmt_num(u, 2)
                ));
            }
        }
    }
    if let Some((obj, u)) = mais_ocupado(&sim.aluno) {
        out.push(format!(
            "Na sua tentativa, o recurso mais ocupado é \"{obj}\", com utilização de {}.",
            fmt_num(u, 2)
        ));
    }
    for m in metricas {
        if m.situacao == Situacao::NaoAtingida
            && let Some(meta) = m.meta
        {
            out.push(format!(
                "{} ficou em {} ± {}{}, e a meta é {}.",
                m.rotulo,
                fmt_num(m.aluno.media, m.casas),
                fmt_num(m.aluno.meia_largura, m.casas),
                if m.unidade.is_empty() {
                    String::new()
                } else {
                    format!(" {}", m.unidade)
                },
                fmt_num(meta, m.casas)
            ));
        }
    }
    Ok(out)
}
