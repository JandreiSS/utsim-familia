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
            // Copies of a machine in a layout mission are named `<maquina>__...`.
            let obj = obj.split("__").next().unwrap_or(obj);
            Some((obj.to_string(), Intervalo::de(xs).media))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Short forms and unaccented words of the model ids, as the student reads them.
const PALAVRAS: &[(&str, &str)] = &[
    ("maq", "máquina"),
    ("est", "estação"),
    ("estacao", "estação"),
    ("insp", "inspeção"),
    ("inspecao", "inspeção"),
    ("emp", "empilhadeira"),
    ("agv", "AGV"),
    ("cal", "calibração"),
    ("calibracao", "calibração"),
    ("aprovacao", "aprovação"),
    ("separacao", "separação"),
    ("expedicao", "expedição"),
    ("furacao", "furação"),
    ("retifica", "retífica"),
    ("camara", "câmara"),
    ("medicos", "médicos"),
    ("lancamento", "lançamento"),
];

/// The name of a model object as the student reads it: `maq1` is "Máquina 1",
/// `fila_divisora` is "Fila divisora", `torno__eixos_2` (a layout copy) is "Torno".
pub fn nome_visivel(id: &str) -> String {
    let id = id.split("__").next().unwrap_or(id);
    let mut palavras = Vec::new();
    for w in id.split('_').filter(|w| !w.is_empty()) {
        let corte = w.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let (letras, digitos) = w.split_at(corte);
        let troca = PALAVRAS.iter().find(|(k, _)| *k == letras).map(|(_, v)| *v);
        match troca {
            Some(t) => {
                palavras.push(t.to_string());
                if !digitos.is_empty() {
                    palavras.push(digitos.to_string());
                }
            }
            None => palavras.push(w.to_string()),
        }
    }
    let s = palavras.join(" ");
    let mut cs = s.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().chain(cs).collect(),
        None => s,
    }
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
            "Na sua tentativa, o recurso mais ocupado é \"{}\", com utilização de {}.",
            nome_visivel(&obj),
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

#[cfg(test)]
mod tests {
    use super::nome_visivel;

    #[test]
    fn nomes_como_o_aluno_le() {
        assert_eq!(nome_visivel("maq1"), "Máquina 1");
        assert_eq!(nome_visivel("est10"), "Estação 10");
        assert_eq!(nome_visivel("fila_divisora"), "Fila divisora");
        assert_eq!(nome_visivel("torno__eixos_2"), "Torno");
        assert_eq!(nome_visivel("agv_3"), "AGV 3");
        assert_eq!(nome_visivel("ana"), "Ana");
        assert_eq!(nome_visivel("medicos_S1"), "Médicos S1");
        assert_eq!(nome_visivel("camara"), "Câmara");
    }
}
