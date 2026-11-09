//! The opening screen, in the family's common design ([`utsim_ui_tema::abertura`]): who is
//! playing, the big actions (continue the campaign, the class, another package, the
//! glossary), the installed packages and where the progress is kept. Every text comes from
//! the catalogue ([`crate::texto`]), in the three languages.

use egui::{Color32, RichText, Ui};
use utsim_ui_tema::abertura::{Abertura, Acao, Escolha, Onde, Recente, Rotulos, Simbolo};
use utsim_ui_tema::arquivos::{self, Filtro, Tipo};

use crate::{App, Tela, texto};

/// The colour of the app (the same as in the launcher).
pub const COR: Color32 = Color32::from_rgb(0x8A, 0x5C, 0xE6);
/// The icon of the app (the same as in the launcher).
pub const ICONE: &[u8] = include_bytes!("../../../assets/icone/familia/ensino_256.png");

fn rotulos() -> Rotulos<'static> {
    Rotulos {
        o_que_fazer: texto::ab_o_que_fazer(),
        onde_salvo: texto::ab_onde_salvo(),
        nada_aberto: texto::ab_nada_aberto(),
        nada_aberto_texto: texto::ab_nada_aberto_texto(),
        aberto_agora: texto::ab_aberto_agora(),
        mostrar_no_explorador: texto::ab_mostrar(),
        abrir: texto::ab_abrir(),
        nao_encontrada: texto::ab_nao_encontrada(),
        tirar_da_lista: texto::ab_tirar(),
    }
}

fn editando_id() -> egui::Id {
    egui::Id::new("ensino-abertura-editando-identidade")
}

impl App {
    /// The opening screen (also the first screen, where the student says who is playing).
    pub(crate) fn tela_abertura(&mut self, ui: &mut Ui) {
        let sem_identidade = self.config.identidade.is_none();
        let editando = sem_identidade
            || ui
                .ctx()
                .data(|d| d.get_temp::<bool>(editando_id()))
                .unwrap_or(false);
        let titulo_pacote = self
            .pacote
            .as_ref()
            .map(|p| p.pacote.titulo.clone())
            .unwrap_or_else(|| texto::aba_campanha().to_string());
        let continuar = texto::f(texto::ab_continuar(), &[&titulo_pacote]);
        let bloqueio = texto::ab_identidade_bloqueio();
        let mut acoes = vec![
            Acao::nova(
                "continuar",
                Simbolo::Continuar,
                &continuar,
                texto::ab_continuar_texto(),
            )
            .principal()
            .bloqueada_se(sem_identidade, bloqueio),
            Acao::nova(
                "turma",
                Simbolo::Pessoas,
                texto::aba_turma(),
                texto::ab_turma_texto(),
            )
            .bloqueada_se(sem_identidade, bloqueio),
            Acao::nova(
                "pacote",
                Simbolo::Abrir,
                texto::ab_pacote(),
                texto::ab_pacote_texto(),
            )
            .bloqueada_se(sem_identidade, bloqueio),
            Acao::nova(
                "glossario",
                Simbolo::Exemplo,
                texto::aba_glossario(),
                texto::ab_glossario_texto(),
            ),
        ];
        if self.config.professor {
            acoes.push(Acao::nova(
                "professor",
                Simbolo::Planilha,
                texto::aba_professor(),
                texto::ab_professor_texto(),
            ));
        }
        let recentes: Vec<Recente> = self
            .pacotes_instalados
            .iter()
            .map(|p| Recente {
                titulo: p.titulo.clone(),
                caminho: p.caminho.clone(),
            })
            .collect();
        let pasta = self.perfil.dir.clone();
        let abertura = Abertura {
            nome: texto::NOME_APP,
            subtitulo: texto::subtitulo(),
            cor: COR,
            icone: Some(ICONE),
            acoes: &acoes,
            titulo_recentes: texto::ab_pacotes(),
            recentes: &recentes,
            onde: Onde {
                caminho: Some(&pasta),
                explicacao: texto::ab_onde_texto(),
            },
            rotulos: rotulos(),
        };
        let mensagem = self.mensagem.clone();
        let escolha = abertura.mostrar(
            ui,
            |ui| self.identidade_ui(ui, editando),
            |ui| {
                if let Some(m) = mensagem {
                    ui.add_space(10.0);
                    ui.colored_label(ui.visuals().error_fg_color, m);
                }
            },
        );
        match escolha {
            Some(Escolha::Acao("continuar")) => self.tela = Tela::Mapa,
            Some(Escolha::Acao("turma")) => self.tela = Tela::Turma,
            Some(Escolha::Acao("glossario")) => self.tela = Tela::Glossario,
            Some(Escolha::Acao("professor")) => self.tela = Tela::Professor,
            Some(Escolha::Acao("pacote")) => {
                let filtro = Filtro {
                    nome: texto::ab_pacote_filtro(),
                    extensoes: &["utse"],
                };
                let atual = self.caminho_pacote.clone();
                if let Some(p) =
                    arquivos::procurar(Tipo::Abrir, texto::ab_pacote(), &[filtro], &atual)
                {
                    self.abrir_e_ir(&p);
                }
            }
            Some(Escolha::Recente(i)) => {
                if let Some(p) = self.pacotes_instalados.get(i).map(|p| p.caminho.clone()) {
                    self.abrir_e_ir(&p);
                }
            }
            Some(Escolha::MostrarPasta(p)) => {
                if let Err(e) = arquivos::mostrar_no_explorador(&p) {
                    self.mensagem = Some(e.to_string());
                }
            }
            Some(Escolha::Esquecer(_) | Escolha::Acao(_)) | None => {}
        }
    }

    fn abrir_e_ir(&mut self, p: &std::path::Path) {
        match self.abrir_pacote(p) {
            Ok(()) => {
                self.mensagem = None;
                self.tela = Tela::Mapa;
            }
            Err(e) => self.mensagem = Some(texto::f(texto::pacote_nao_abriu(), &[&e])),
        }
    }

    /// Who is playing: the form (first time, or "Change"), or one line with the name.
    fn identidade_ui(&mut self, ui: &mut Ui, editando: bool) {
        if !editando {
            let (nome, registro) = self
                .config
                .identidade
                .as_ref()
                .map(|a| (a.nome.clone(), a.registro.clone()))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(texto::ab_voce_esta()).color(ui.visuals().weak_text_color()),
                );
                ui.label(RichText::new(nome).strong());
                ui.label(RichText::new(registro).weak());
                if ui.small_button(texto::ab_trocar()).clicked() {
                    if let Some(a) = &self.config.identidade {
                        self.nome = a.nome.clone();
                        self.registro = a.registro.clone();
                        self.apelido = a.apelido.clone();
                    }
                    ui.ctx().data_mut(|d| d.insert_temp(editando_id(), true));
                }
            });
            ui.add_space(16.0);
            return;
        }
        utsim_ui_tema::abertura::quadro_destaque(COR).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(texto::identidade_titulo())
                    .strong()
                    .size(16.0),
            );
            ui.label(
                RichText::new(texto::ab_identidade_antes()).color(ui.visuals().weak_text_color()),
            );
            ui.label(
                RichText::new(texto::identidade_texto()).color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            egui::Grid::new("identidade")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(texto::nome());
                    ui.add(egui::TextEdit::singleline(&mut self.nome).desired_width(300.0));
                    ui.end_row();
                    ui.label(texto::registro_academico());
                    ui.add(egui::TextEdit::singleline(&mut self.registro).desired_width(300.0));
                    ui.end_row();
                    ui.label(texto::apelido_opcional());
                    ui.add(egui::TextEdit::singleline(&mut self.apelido).desired_width(300.0));
                    ui.end_row();
                });
            ui.add_space(6.0);
            if ui
                .add(utsim_ui_tema::widgets::primary(texto::comecar()))
                .clicked()
            {
                let (n, r, a) = (
                    self.nome.clone(),
                    self.registro.clone(),
                    self.apelido.clone(),
                );
                if self.salvar_identidade(&n, &r, &a) {
                    ui.ctx().data_mut(|d| d.insert_temp(editando_id(), false));
                }
            }
        });
        ui.add_space(16.0);
    }
}
