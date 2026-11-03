//! The lines that come with UTSim (their packs are compiled in from `linhas/<line>/`).

use crate::pacote::Pack;

/// Identifier of the Lean Office line.
pub const LEAN_OFFICE: &str = "lean_office";

/// `(library, vocabulary)` text of every built-in line.
const BUILTIN: [(&str, &str); 1] = [(
    include_str!("../../../linhas/lean_office/biblioteca.ron"),
    include_str!("../../../linhas/lean_office/vocabulario.ron"),
)];

/// Every built-in line, read and checked.
pub fn builtin() -> Vec<Pack> {
    BUILTIN
        .iter()
        .map(|(lib, voc)| Pack::parse(lib, voc).expect("built-in line pack is valid"))
        .collect()
}

/// The built-in line `id` (`"lean_office"`).
pub fn find(id: &str) -> Option<Pack> {
    builtin().into_iter().find(|p| p.id() == id)
}

/// The line of a model: its `line` field, when it names a built-in line.
pub fn of_model(model: &utsim_model::Model) -> Option<Pack> {
    model.line.as_deref().and_then(find)
}
