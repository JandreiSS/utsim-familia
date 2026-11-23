//! Começar sem pasta e salvar depois, nos apps da unidade (Qualidade e Segurança): a unidade
//! é o documento. O app abre numa unidade em branco ([`Sessao::rascunho`]) e já deixa
//! trabalhar. A barra do canto de cima à esquerda tem a chave "Salvamento automático",
//! Salvar (Ctrl+S) e Salvar como (Ctrl+Shift+S). Fechar, abrir outra unidade ou começar uma
//! nova com trabalho não salvo pergunta antes ("Salvar / Não salvar / Cancelar"), pela
//! guarda comum de `utsim_ui_tema::documento`.
//!
//! O app guarda um [`Documento`] e chama, a cada quadro: [`Documento::barra`] num painel de
//! cima, [`Documento::atalhos`] e, no fim, [`Documento::quadro`]. Abrir e começar pela tela
//! inicial passam por [`Documento::pedir`]. Depois de uma troca de unidade,
//! [`Documento::trocou`] diz ao app para levar a pessoa ao trabalho.

use std::path::PathBuf;

use utsim_ui_tema::arquivos::{self, Tipo};
use utsim_ui_tema::documento::{self, Clique, Evento, Guarda, Pedido, Situacao};

use crate::Sessao;
use crate::inicial::nome_de_pasta;
use crate::texto::{fx, tx, vx};

/// O que espera a resposta de "Salvar / Não salvar / Cancelar".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Depois {
    /// Começar uma unidade em branco.
    Nova,
    /// Abrir a unidade desta pasta.
    Abrir(PathBuf),
}

/// A janela "Salvar como": o nome da unidade e onde fica a pasta.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JanelaSalvar {
    pub nome: String,
    pub pasta: String,
}

impl JanelaSalvar {
    /// A pasta que será criada (`None`: falta o nome ou o lugar).
    pub fn destino(&self) -> Option<PathBuf> {
        let (n, p) = (self.nome.trim(), self.pasta.trim());
        (!n.is_empty() && !p.is_empty()).then(|| PathBuf::from(p).join(nome_de_pasta(n)))
    }
}

#[derive(Default)]
pub struct Documento {
    /// Segura o fechar, o abrir e o começar enquanto há trabalho não salvo.
    pub guarda: Guarda<Depois>,
    /// A janela "Salvar como" aberta.
    pub janela: Option<JanelaSalvar>,
    /// O título posto na janela por último.
    titulo: String,
    /// A janela deve fechar (a pessoa salvou depois de pedir para fechar).
    fechar: bool,
    /// A unidade foi trocada (nova ou aberta) desde a última consulta.
    trocou: bool,
}

impl Documento {
    /// A barra do documento: desenhe num `egui::Panel::top` antes dos outros painéis.
    pub fn barra(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        if s.unidade.is_none() {
            return;
        }
        let situacao = if s.eh_rascunho() || s.nao_salvo() {
            Situacao::NaoSalvo
        } else {
            Situacao::Salvo
        };
        match documento::barra(ui, s.automatico(), situacao) {
            Some(Clique::Salvar) => self.salvar(s),
            Some(Clique::SalvarComo) => self.salvar_como(s),
            Some(Clique::Automatico(ligado)) => {
                if s.eh_rascunho() {
                    // Sem pasta, ligar pede o nome e o lugar. Depois do primeiro Salvar a
                    // chave nasce ligada.
                    if ligado {
                        self.salvar_como(s);
                    }
                } else {
                    s.definir_automatico(ligado);
                }
            }
            None => {}
        }
    }

    /// Ctrl+S salva e Ctrl+Shift+S salva como, fora dos campos de texto também. Com
    /// `ativos` falso (a tela Modelo, que tem o próprio Ctrl+S), não faz nada.
    pub fn atalhos(&mut self, ctx: &egui::Context, s: &mut Sessao, ativos: bool) {
        if !ativos || s.unidade.is_none() || self.janela.is_some() {
            return;
        }
        use egui::{Key, Modifiers};
        let (como, salvar) = ctx.input_mut(|i| {
            let como = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
            (como, !como && i.consume_key(Modifiers::COMMAND, Key::S))
        });
        if como {
            self.salvar_como(s);
        } else if salvar {
            self.salvar(s);
        }
    }

    /// Salvar: com pasta, grava o que esperava; sem pasta, abre o "Salvar como".
    pub fn salvar(&mut self, s: &mut Sessao) {
        if s.eh_rascunho() {
            self.salvar_como(s);
        } else if s.salvar() {
            self.salvou(s);
        } else {
            self.guarda.cancelar();
        }
    }

    /// Abre a janela "Salvar como", com o nome da unidade e a pasta de agora (ou Documentos).
    pub fn salvar_como(&mut self, s: &Sessao) {
        let Some(u) = &s.unidade else {
            return;
        };
        let pasta = if u.eh_rascunho() {
            arquivos::documentos()
        } else {
            u.raiz().parent().map(std::path::Path::to_path_buf)
        };
        let nome = if u.eh_rascunho() {
            String::new()
        } else {
            u.manifesto.nome.clone()
        };
        self.janela = Some(JanelaSalvar {
            nome,
            pasta: pasta.map(|p| p.display().to_string()).unwrap_or_default(),
        });
    }

    /// Salva na pasta da janela e segue com o que esperava o salvar. Devolve se salvou.
    pub fn salvar_em(&mut self, s: &mut Sessao, j: &JanelaSalvar) -> bool {
        let Some(destino) = j.destino() else {
            return false;
        };
        if !s.salvar_como(&destino, j.nome.trim()) {
            return false;
        }
        self.janela = None;
        self.salvou(s);
        true
    }

    fn salvou(&mut self, s: &mut Sessao) {
        match self.guarda.salvou() {
            Some(Pedido::Fechar) => self.fechar = true,
            Some(p) => self.fazer(s, p),
            None => {}
        }
    }

    /// O app quer começar uma unidade nova ou abrir outra: com trabalho não salvo,
    /// pergunta antes; senão, faz já.
    pub fn pedir(&mut self, s: &mut Sessao, acao: Depois) {
        if let Some(a) = self.guarda.pedir(s.nao_salvo(), acao) {
            self.fazer(s, Pedido::Acao(a));
        }
    }

    /// A resposta a "Salvar / Não salvar / Cancelar" (o que a pergunta na tela faz).
    pub fn responder(&mut self, s: &mut Sessao, r: documento::Resposta) {
        match self.guarda.responder(r) {
            Some(Evento::Salvar) => self.salvar(s),
            Some(Evento::Seguir(p)) => self.fazer(s, p),
            None => {}
        }
    }

    fn fazer(&mut self, s: &mut Sessao, p: Pedido<Depois>) {
        match p {
            Pedido::Fechar => {}
            Pedido::Acao(Depois::Nova) => {
                // O que não foi salvo foi descartado (a pasta temporária vai junto).
                if s.rascunho() {
                    self.trocou = true;
                }
            }
            Pedido::Acao(Depois::Abrir(p)) => {
                let autor = s.autor_padrao();
                if s.abrir(&p, &autor) {
                    self.trocou = true;
                }
            }
        }
    }

    /// A unidade foi trocada desde a última consulta (o app leva a pessoa ao trabalho).
    pub fn trocou(&mut self) -> bool {
        std::mem::take(&mut self.trocou)
    }

    /// Uma vez por quadro, no fim do `draw`: a pergunta ao fechar ou trocar com trabalho não
    /// salvo, a janela "Salvar como" e o título da janela (`app` é o nome do app).
    pub fn quadro(&mut self, ctx: &egui::Context, s: &mut Sessao, app: &str) {
        if let Some(j) = self.janela.clone() {
            self.janela_salvar(ctx, s, j);
        }
        match self.guarda.quadro(ctx, s.nao_salvo()) {
            Some(Evento::Salvar) => self.salvar(s),
            Some(Evento::Seguir(p)) => self.fazer(s, p),
            None => {}
        }
        if std::mem::take(&mut self.fechar) {
            self.guarda.fechar_agora(ctx);
        }
        let nome = s.unidade.as_ref().map(|u| u.manifesto.nome.clone());
        let titulo =
            documento::titulo_da_janela(app, nome.as_deref(), s.eh_rascunho() || s.nao_salvo());
        if titulo != self.titulo {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(titulo.clone()));
            self.titulo = titulo;
        }
    }

    fn janela_salvar(&mut self, ctx: &egui::Context, s: &mut Sessao, mut j: JanelaSalvar) {
        let mut salvar = false;
        let mut cancelar = false;
        let destino = j.destino();
        let modal = egui::Modal::new(egui::Id::new("salvar_unidade")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading(utsim_ui_tema::texto::tx("documento-salvar_como"));
            ui.add_space(8.0);
            ui.label(tx("documento-nome_da_unidade"));
            ui.add(egui::TextEdit::singleline(&mut j.nome).desired_width(f32::INFINITY));
            ui.add_space(8.0);
            ui.label(tx("documento-onde_salvar"));
            arquivos::campo(
                ui,
                &mut j.pasta,
                Tipo::Pasta,
                tx("documento-onde_salvar"),
                &[],
                420.0,
            );
            ui.add_space(8.0);
            let fraco = ui.visuals().weak_text_color();
            match &destino {
                Some(d) => ui.colored_label(
                    fraco,
                    fx("documento-sera_criada", &[("pasta", vx(&d.display()))]),
                ),
                None => ui.colored_label(fraco, tx("documento-escreva_nome_e_pasta")),
            };
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                salvar = ui
                    .add_enabled(
                        destino.is_some(),
                        utsim_ui_tema::widgets::primary(utsim_ui_tema::texto::tx(
                            "documento-salvar",
                        )),
                    )
                    .clicked();
                cancelar = ui
                    .button(utsim_ui_tema::texto::tx("documento-cancelar"))
                    .clicked();
            });
            if s.erro && !s.mensagem.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &s.mensagem);
            }
        });
        if salvar {
            self.janela = Some(j.clone());
            self.salvar_em(s, &j);
        } else if cancelar || modal.should_close() {
            self.janela = None;
            self.guarda.cancelar();
        } else {
            self.janela = Some(j);
        }
    }
}
