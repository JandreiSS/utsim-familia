//! The language picker shared by every app: a list with each language in its own name. The
//! app saves the choice in its configuration and calls [`crate::definir`].

use std::fmt::Debug;
use std::hash::Hash;

use crate::Idioma;

/// A drop-down list of the languages; whether the choice changed.
pub fn seletor(ui: &mut egui::Ui, id: impl Hash + Debug, idioma: &mut Idioma) -> bool {
    let mut mudou = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(idioma.nome())
        .show_ui(ui, |ui| {
            for i in Idioma::TODOS {
                if ui.selectable_value(idioma, i, i.nome()).changed() {
                    mudou = true;
                }
            }
        });
    mudou
}
