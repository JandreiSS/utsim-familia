//! Problem-solving tools shared by the apps of the family (stage QS7 of
//! `docs/plano_qualidade_e_seguranca.md`): the project's own action priority table of the
//! FMEA ([`ap`]), the Pareto analysis ([`pareto`]) and the report generator with its PDF
//! and CSV ([`relatorios`]). Nothing here knows the state of a Seis Sigma project: the
//! tools bound to it (FMEA rows, Ishikawa, 5 porquês, A3, plans) stay in
//! `utsim_lss_core::tools` and use these pieces.

pub mod ap;
pub mod pareto;
pub mod relatorios;

/// A number typed by a person: `1,5`, `1.5`, ` 2 `; `None` when empty or not a number.
pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let t = if t.contains(',') && !t.contains('.') {
        t.replace(',', ".")
    } else {
        t.replace(',', "")
    };
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}
