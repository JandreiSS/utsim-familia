//! The text catalogue of the interface (one place, for a future translation) and a small
//! renderer of the statements' Markdown (headings, lists, bold).

use egui::{RichText, Ui};

pub const NOME_APP: &str = "UTSim Ensino";
pub const SUBTITULO: &str = "Ensino pela simulação";
pub const IDENTIDADE_TITULO: &str = "Quem está jogando?";
pub const IDENTIDADE_TEXTO: &str = "O registro acadêmico identifica as suas entregas e gera a variante dos seus dados. Use o mesmo registro da lista do professor.";
pub const PREVER_ANTES: &str = "Responda às previsões antes de simular. Elas valem pontos e ficam travadas depois da primeira rodada.";
pub const SEM_IDENTIDADE: &str = "Preencha nome e registro (letras, números, - e _).";
pub const MEDIR: &str = "Medir o estado atual";
pub const SIMULAR: &str = "Simular";
pub const CANCELAR: &str = "Cancelar";
pub const AREIA_TEXTO: &str = "Sem meta e sem orçamento. Compare dois cenários com os mesmos números aleatórios. Nada aqui conta nota.";
pub const LIMITE_COLA: &str = "Sem servidor, nada impede a consulta. A entrega leva as decisões e as tentativas assinadas, e o professor simula de novo: editar o arquivo não muda a nota e deixa a alteração visível.";

/// Draws a statement: `#` headings, `-` and `1.` lists, `**bold**` as strong text.
pub fn markdown(ui: &mut Ui, md: &str) {
    for bloco in md.split("\n\n") {
        let bloco = bloco.trim();
        if bloco.is_empty() {
            continue;
        }
        for linha in bloco.lines() {
            let l = linha.trim_end();
            if let Some(t) = l.strip_prefix("# ") {
                ui.label(RichText::new(t).heading());
            } else if let Some(t) = l.strip_prefix("## ") {
                ui.add_space(4.0);
                ui.label(RichText::new(t).strong().size(16.0));
            } else if let Some(t) = l.trim_start().strip_prefix("- ") {
                ui.horizontal_wrapped(|ui| {
                    ui.label("  •");
                    linha_rica(ui, t);
                });
            } else if l
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
                && l.trim_start().contains(". ")
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label("  ");
                    linha_rica(ui, l.trim_start());
                });
            } else {
                ui.horizontal_wrapped(|ui| linha_rica(ui, l));
            }
        }
        ui.add_space(6.0);
    }
}

fn linha_rica(ui: &mut Ui, t: &str) {
    ui.spacing_mut().item_spacing.x = 0.0;
    for (i, parte) in t.split("**").enumerate() {
        if parte.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            ui.label(RichText::new(parte).strong());
        } else {
            ui.label(parte);
        }
    }
}
