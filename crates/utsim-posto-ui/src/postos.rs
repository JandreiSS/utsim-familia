//! Tela de postos de trabalho: lista, criar, editar, remover e importar do modelo.

use utsim_posto::{Posto, sugerir_postos};

use crate::sessao::Sessao;
use crate::util;

#[derive(Default)]
pub struct TelaPostos {
    /// Id do posto em edição (vazio com `editando` verdadeiro: posto novo).
    pub selecionado: String,
    pub editando: bool,
    pub rascunho: Posto,
    atividades: String,
    epi: String,
    modelo: String,
    objeto: String,
    /// Caminho do modelo a importar.
    pub importar: String,
    pub sugestoes: Vec<Posto>,
    escolhidas: Vec<bool>,
}

impl TelaPostos {
    fn abrir_posto(&mut self, p: &Posto) {
        self.selecionado = p.id.clone();
        self.editando = true;
        self.atividades = p.atividades.join("\n");
        self.epi = p.epi.join(", ");
        self.modelo = p.modelo.clone().unwrap_or_default();
        self.objeto = p.objeto.clone().unwrap_or_default();
        self.rascunho = p.clone();
    }

    /// Posto do rascunho com os campos de texto aplicados.
    pub fn montar(&self) -> Posto {
        let mut p = self.rascunho.clone();
        p.atividades = self
            .atividades
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        p.epi = self
            .epi
            .split([',', '\n'])
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        let m = self.modelo.trim();
        p.modelo = (!m.is_empty()).then(|| m.to_string());
        let o = self.objeto.trim();
        p.objeto = (!o.is_empty()).then(|| o.to_string());
        p
    }

    /// Lê o modelo de `importar` e preenche as sugestões.
    pub fn sugerir(&mut self, s: &mut Sessao) {
        let Some(u) = s.unidade.as_ref() else {
            return;
        };
        let caminho = self.importar.trim().to_string();
        match util::carregar_modelo(u, &caminho) {
            Ok(m) => {
                self.sugestoes = sugerir_postos(&m, &u.postos(), &caminho);
                self.escolhidas = vec![true; self.sugestoes.len()];
                let n = self.sugestoes.len();
                s.avisar(format!("{n} postos sugeridos pelo modelo."));
            }
            Err(e) => s.falhar(e),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        if s.unidade.is_none() {
            ui.label("Abra uma unidade na tela inicial.");
            return;
        }
        ui.heading("Postos de trabalho");
        ui.horizontal(|ui| {
            if ui.button("Atualizar").clicked() {
                let n = s.atualizar();
                if n == 0 {
                    s.avisar("Nada novo na pasta.");
                }
            }
            if ui.button("Novo posto").clicked() {
                self.abrir_posto(&Posto {
                    pessoas_expostas: 1,
                    ..Posto::default()
                });
            }
        });
        let postos = s.unidade.as_ref().map(|u| u.postos()).unwrap_or_default();
        ui.columns(2, |cols| {
            self.lista(&mut cols[0], s, &postos);
            self.formulario(&mut cols[1], s);
        });
        ui.separator();
        self.importacao(ui, s);
    }

    fn lista(&mut self, ui: &mut egui::Ui, s: &mut Sessao, postos: &[Posto]) {
        ui.strong(format!("{} postos na unidade", postos.len()));
        egui::ScrollArea::vertical()
            .id_salt("lista_postos")
            .max_height(320.0)
            .show(ui, |ui| {
                for p in postos {
                    let marcado = self.editando && self.selecionado == p.id;
                    let rotulo = if p.area.is_empty() {
                        p.nome.clone()
                    } else {
                        format!("{} ({})", p.nome, p.area)
                    };
                    if ui.selectable_label(marcado, rotulo).clicked() {
                        self.abrir_posto(p);
                    }
                }
            });
        let _ = s;
    }

    fn formulario(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        if !self.editando {
            ui.weak("Escolha um posto ou crie um novo.");
            return;
        }
        ui.strong(if self.selecionado.is_empty() {
            "Novo posto"
        } else {
            "Editar posto"
        });
        egui::Grid::new("form_posto").num_columns(2).show(ui, |ui| {
            ui.label("Nome");
            ui.text_edit_singleline(&mut self.rascunho.nome);
            ui.end_row();
            ui.label("Área");
            ui.text_edit_singleline(&mut self.rascunho.area);
            ui.end_row();
            ui.label("Descrição");
            ui.text_edit_singleline(&mut self.rascunho.descricao);
            ui.end_row();
            ui.label("Pessoas expostas por turno");
            ui.add(egui::DragValue::new(&mut self.rascunho.pessoas_expostas).range(0..=999));
            ui.end_row();
            ui.label("EPI (separados por vírgula)");
            ui.text_edit_singleline(&mut self.epi);
            ui.end_row();
            ui.label("Modelo (.ron)");
            crate::util::campo_modelo(ui, &mut self.modelo, s.unidade.as_ref(), 260.0);
            ui.end_row();
            ui.label("Objeto do modelo");
            ui.text_edit_singleline(&mut self.objeto);
            ui.end_row();
        });
        ui.label("Atividades (uma por linha)");
        ui.add(
            egui::TextEdit::multiline(&mut self.atividades)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            let ok = !self.rascunho.nome.trim().is_empty();
            if ui.add_enabled(ok, egui::Button::new("Gravar")).clicked() {
                let p = self.montar();
                if let Some(u) = s.unidade.as_mut() {
                    let r = u.gravar_posto(&p);
                    if let Some(id) = s.resultado(r, "Posto gravado.") {
                        self.selecionado = id.clone();
                        self.rascunho.id = id;
                    }
                }
            }
            if !self.selecionado.is_empty() && ui.button("Remover").clicked() {
                let id = self.selecionado.clone();
                if let Some(u) = s.unidade.as_mut() {
                    let r = u.remover(utsim_posto::POSTOS, &id);
                    if s.resultado(r, "Posto removido.").is_some() {
                        self.editando = false;
                        self.selecionado.clear();
                    }
                }
            }
            if ui.button("Fechar").clicked() {
                self.editando = false;
            }
        });
    }

    fn importacao(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        ui.strong("Importar postos do modelo");
        ui.horizontal(|ui| {
            ui.label("Modelo (.ron):");
            crate::util::campo_modelo(ui, &mut self.importar, s.unidade.as_ref(), 420.0);
            if ui
                .add_enabled(
                    !self.importar.trim().is_empty(),
                    egui::Button::new("Importar postos do modelo"),
                )
                .clicked()
            {
                self.sugerir(s);
            }
        });
        if self.sugestoes.is_empty() {
            return;
        }
        for (p, marcada) in self.sugestoes.iter().zip(self.escolhidas.iter_mut()) {
            ui.checkbox(
                marcada,
                format!(
                    "{} (objeto {}, EPI: {})",
                    p.nome,
                    p.objeto.as_deref().unwrap_or("-"),
                    if p.epi.is_empty() {
                        "nenhum".to_string()
                    } else {
                        p.epi.join(", ")
                    }
                ),
            );
        }
        if ui.button("Criar os postos marcados").clicked() {
            let novos: Vec<Posto> = self
                .sugestoes
                .iter()
                .zip(&self.escolhidas)
                .filter(|(_, m)| **m)
                .map(|(p, _)| p.clone())
                .collect();
            let mut n = 0;
            if let Some(u) = s.unidade.as_mut() {
                for p in &novos {
                    match u.gravar_posto(p) {
                        Ok(_) => n += 1,
                        Err(e) => {
                            s.falhar(e.to_string());
                            break;
                        }
                    }
                }
            }
            if n > 0 {
                s.mudou();
                s.avisar(format!("{n} postos criados."));
                self.sugestoes.clear();
                self.escolhidas.clear();
            }
        }
    }
}
