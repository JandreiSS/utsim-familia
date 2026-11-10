//! The packs that come with UTSim (compiled in from `linhas/<id>/`): the Lean Office line
//! and the cross-cutting packs Quality and Safety, which add to any line through
//! `Model.packs`.

use crate::pacote::Pack;

/// Identifier of the Lean Office line.
pub const LEAN_OFFICE: &str = "lean_office";
/// Identifier of the line Serviços e atendimento.
pub const SERVICOS: &str = "servicos";
/// Identifier of the line Logística e armazém.
pub const LOGISTICA: &str = "logistica";
/// Identifier of the line Saúde e hospital.
pub const SAUDE: &str = "saude";
/// Identifier of the line Manufatura discreta.
pub const MANUFATURA: &str = "manufatura";
/// Identifier of the line Processos contínuos e batelada.
pub const PROCESSOS: &str = "processos";
/// Identifier of the cross-cutting Quality pack.
pub const QUALIDADE: &str = "qualidade";
/// Identifier of the cross-cutting Safety pack.
pub const SEGURANCA: &str = "seguranca";

/// `(library, vocabulary)` text of every built-in pack.
const BUILTIN: [(&str, &str); 8] = [
    (
        include_str!("../../../linhas/lean_office/biblioteca.ron"),
        include_str!("../../../linhas/lean_office/vocabulario.ron"),
    ),
    (
        include_str!("../../../linhas/servicos/biblioteca.ron"),
        include_str!("../../../linhas/servicos/vocabulario.ron"),
    ),
    (
        include_str!("../../../linhas/logistica/biblioteca.ron"),
        include_str!("../../../linhas/logistica/vocabulario.ron"),
    ),
    (
        include_str!("../../../linhas/saude/biblioteca.ron"),
        include_str!("../../../linhas/saude/vocabulario.ron"),
    ),
    (
        include_str!("../../../linhas/manufatura/biblioteca.ron"),
        include_str!("../../../linhas/manufatura/vocabulario.ron"),
    ),
    (
        include_str!("../../../linhas/processos/biblioteca.ron"),
        include_str!("../../../linhas/processos/vocabulario.ron"),
    ),
    (
        include_str!("../../../linhas/qualidade/biblioteca.ron"),
        include_str!("../../../linhas/qualidade/vocabulario.ron"),
    ),
    (
        include_str!("../../../linhas/seguranca/biblioteca.ron"),
        include_str!("../../../linhas/seguranca/vocabulario.ron"),
    ),
];

/// Every built-in pack, read and checked (lines first, then the cross-cutting packs).
pub fn builtin() -> Vec<Pack> {
    BUILTIN
        .iter()
        .map(|(lib, voc)| Pack::parse(lib, voc).expect("built-in pack is valid"))
        .collect()
}

/// The built-in pack `id` (`"lean_office"`, `"qualidade"`, `"seguranca"`).
pub fn find(id: &str) -> Option<Pack> {
    builtin().into_iter().find(|p| p.id() == id)
}

/// The line of a model: its `line` field, when it names a built-in line (a cross-cutting
/// pack is never a line).
pub fn of_model(model: &utsim_model::Model) -> Option<Pack> {
    model
        .line
        .as_deref()
        .and_then(find)
        .filter(|p| !p.is_cross_cutting())
}

/// Every pack active in a model: its line first, then each pack of `packs` (known
/// cross-cutting ones only, no repeats).
pub fn active(model: &utsim_model::Model) -> Vec<Pack> {
    let mut out: Vec<Pack> = of_model(model).into_iter().collect();
    for id in &model.packs {
        if out.iter().any(|p| p.id() == id) {
            continue;
        }
        if let Some(p) = find(id).filter(Pack::is_cross_cutting) {
            out.push(p);
        }
    }
    out
}
