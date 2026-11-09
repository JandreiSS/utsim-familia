//! Tela inicial: criar ou abrir a pasta de unidade e dizer quem é o autor.

use std::path::PathBuf;

use crate::sessao::Sessao;

#[derive(Default)]
pub struct TelaInicial {
    pub caminho: String,
    pub nome: String,
    pub autor: String,
    iniciada: bool,
}

impl TelaInicial {
    /// Desenha a tela. Devolve `true` no quadro em que a unidade foi aberta ou criada.
    pub fn ui(&mut self, ui: &mut egui::Ui, s: &mut Sessao) -> bool {
        if !self.iniciada {
            self.iniciada = true;
            if self.autor.is_empty() {
                self.autor = s.config.autor.clone();
            }
            if self.caminho.is_empty() {
                self.caminho = s.config.ultima_unidade.clone();
            }
        }
        let mut aberta = false;
        ui.add_space(24.0);
        ui.heading("Pasta de unidade");
        ui.label(
            "A unidade é uma pasta (da fábrica ou do laboratório) que a equipe compartilha \
             pelo Drive. Os apps da família que trabalham com unidade (Qualidade, Segurança do \
             Trabalho e Estatística) abrem a mesma pasta.",
        );
        ui.add_space(8.0);
        egui::Grid::new("inicial").num_columns(2).show(ui, |ui| {
            ui.label("Seu nome (autor):");
            ui.add(egui::TextEdit::singleline(&mut self.autor).desired_width(320.0));
            ui.end_row();
            ui.label("Caminho da pasta:");
            ui.add(egui::TextEdit::singleline(&mut self.caminho).desired_width(480.0));
            ui.end_row();
            ui.label("Nome da unidade (só para criar):");
            ui.add(egui::TextEdit::singleline(&mut self.nome).desired_width(320.0));
            ui.end_row();
        });
        let pronto = !self.autor.trim().is_empty() && !self.caminho.trim().is_empty();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(pronto, egui::Button::new("Abrir unidade"))
                .clicked()
            {
                aberta = s.abrir(&PathBuf::from(self.caminho.trim()), &self.autor);
            }
            if ui
                .add_enabled(
                    pronto && !self.nome.trim().is_empty(),
                    egui::Button::new("Criar unidade"),
                )
                .clicked()
            {
                aberta = s.criar(&PathBuf::from(self.caminho.trim()), &self.nome, &self.autor);
            }
        });
        if !pronto {
            ui.weak("Preencha o nome do autor e o caminho da pasta.");
        }
        aberta
    }
}
