//! Problem-solving tools shared by the apps of the family (stage QS7 and decision G3 of
//! `docs/plano_qualidade_e_seguranca.md`), without any project state and without screens:
//!
//! - [`ap`]: the project's own action priority table of the FMEA;
//! - [`fmea`]: the FMEA row, RPN, AP before and after, ranking, critical rows;
//! - [`ishikawa`]: the cause and effect diagram (6M and the team's categories);
//! - [`cinco_porques`]: five whys chains and their root causes;
//! - [`arvore`]: forests of items robust to cycles (Ishikawa and five whys use it);
//! - [`planos`]: data collection, control and response (reaction) plans;
//! - [`a3`]: the sections of the A3;
//! - [`pareto`]: the Pareto analysis;
//! - [`selecao`]: weighted criteria and Pugh matrices, effort × impact;
//! - [`cinco_s`]: 5S audits by sense;
//! - [`smed`]: quick changeover, internal and external activities;
//! - [`trabalho_padrao`]: standardised work, combination table and line balancing;
//! - [`relatorios`]: the report generator with its PDF and CSV.
//!
//! The structures are generic over the identifier of the app; the field names are the
//! ones the apps save. Reading them from an app's file (in the Seis Sigma, the journal) and
//! writing the edits stay in the app (`utsim_lss_core::tools`).

pub mod a3;
pub mod ap;
pub mod arvore;
pub mod cinco_porques;
pub mod cinco_s;
pub mod fmea;
pub mod ishikawa;
pub mod pareto;
pub mod planos;
pub mod relatorios;
pub mod selecao;
pub mod smed;
pub mod trabalho_padrao;

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
