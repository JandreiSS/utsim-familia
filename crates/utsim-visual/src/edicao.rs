//! A edição de um texto no próprio desenho: um campo posto em cima do cartão, que grava ao
//! sair (Enter ou clique fora) e desiste com Esc.

use egui::{Id, Key, Rect, TextEdit, Ui};

/// O que está sendo editado (`A` diz qual cartão, nó ou osso) e o texto digitado.
#[derive(Clone, Debug)]
pub struct Edicao<A> {
    pub alvo: A,
    pub texto: String,
    /// Pede o foco no primeiro quadro.
    pub foco: bool,
}

impl<A> Edicao<A> {
    pub fn nova(alvo: A, texto: &str) -> Self {
        Self {
            alvo,
            texto: texto.to_string(),
            foco: true,
        }
    }
}

/// Como a edição terminou.
#[derive(Clone, Debug, PartialEq)]
pub enum Fim {
    /// Ainda editando.
    Nada,
    /// Gravar o texto; `enter` quando terminou com Enter (para seguir com o próximo).
    Gravar {
        texto: String,
        enter: bool,
    },
    Cancelar,
}

/// Mostra o campo em `rect` e diz se a edição terminou.
pub fn campo<A>(ui: &mut Ui, id: Id, rect: Rect, ed: &mut Edicao<A>, dica: &str) -> Fim {
    let resp = ui.put(
        rect,
        TextEdit::singleline(&mut ed.texto)
            .id(id)
            .hint_text(dica)
            .desired_width(rect.width()),
    );
    if ed.foco {
        resp.request_focus();
        ed.foco = false;
        return Fim::Nada;
    }
    if resp.lost_focus() {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            return Fim::Cancelar;
        }
        let enter = ui.input(|i| i.key_pressed(Key::Enter));
        return Fim::Gravar {
            texto: ed.texto.trim().to_string(),
            enter,
        };
    }
    if !resp.has_focus() && !resp.gained_focus() && !ui.memory(|m| m.has_focus(id)) {
        // O foco foi para outro lugar sem passar por aqui (outra janela): grava.
        return Fim::Gravar {
            texto: ed.texto.trim().to_string(),
            enter: false,
        };
    }
    Fim::Nada
}
