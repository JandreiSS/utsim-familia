//! Property editor for the selected object (phase 3). Pure egui: it reads the
//! [`Document`] and returns [`Command`]s; it never mutates the model itself, so the
//! editor applies them through the document (undo/redo, validation).

use bevy_egui::egui;
use utsim_edit::{Command, Document};

/// Draws the editable properties of object `selected` and returns what the user changed.
pub fn properties_panel(ui: &mut egui::Ui, doc: &Document, selected: &str) -> Vec<Command> {
    let _ = (doc, selected);
    ui.label("Propriedades: em construção");
    Vec::new()
}
