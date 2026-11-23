//! Começar sem salvar e salvar depois: o que é comum a todos os apps da família.
//!
//! O app abre num documento novo, sem arquivo nem pasta. Enquanto houver mudanças que não
//! estão em lugar nenhum, o app mostra "Não salvo" ([`indicador`]) e nada se perde sem
//! perguntar: fechar a janela, abrir outro documento ou começar um novo passam pela
//! [`Guarda`], que mostra "Salvar / Não salvar / Cancelar".
//!
//! Como um app adota (três pontos de contato):
//!
//! 1. Guarde uma `Guarda<Acao>` no estado, com `Acao` sendo o que o app faz depois da
//!    pergunta (por exemplo `enum Acao { Novo, Abrir(PathBuf) }`).
//! 2. Onde hoje faz "novo" ou "abrir" direto, chame [`Guarda::pedir`]: sem mudanças por
//!    salvar, ela devolve a ação para fazer na hora; com mudanças, guarda a ação e pergunta.
//! 3. A cada quadro chame [`Guarda::quadro`] com `sujo` (há mudanças não salvas?). Ela
//!    segura o fechar da janela, desenha a pergunta e devolve um [`Evento`]:
//!    - [`Evento::Salvar`]: o app salva (o seu "Salvar", que pode abrir o "Salvar como").
//!      Deu certo, chame [`Guarda::salvou`] e faça o [`Pedido`] que ela devolver; a pessoa
//!      desistiu ou deu erro, chame [`Guarda::cancelar`].
//!    - [`Evento::Seguir`]: a pessoa escolheu não salvar; o app faz o [`Pedido`].
//!
//!    [`Pedido::Fechar`] não pede nada do app: a guarda já mandou a janela fechar.
//!
//! Os textos vêm de `idiomas/<código>/tema.ftl` (`documento-*`), nas cinco línguas.

use egui::{Context, RichText, Ui, ViewportCommand};

use crate::texto::tx;

/// O que estava para acontecer quando a guarda perguntou.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pedido<A> {
    /// Fechar a janela (a guarda fecha sozinha).
    Fechar,
    /// A ação do app passada em [`Guarda::pedir`].
    Acao(A),
}

/// O que a pessoa respondeu à pergunta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evento<A> {
    /// "Salvar": o app salva e depois chama [`Guarda::salvou`] ou [`Guarda::cancelar`].
    Salvar,
    /// "Não salvar": o app faz o pedido, descartando as mudanças.
    Seguir(Pedido<A>),
}

/// A resposta do diálogo, antes de virar [`Evento`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resposta {
    Salvar,
    NaoSalvar,
    Cancelar,
}

/// Segura o que perderia mudanças não salvas até a pessoa responder.
#[derive(Clone, Debug)]
pub struct Guarda<A> {
    pendente: Option<Pedido<A>>,
    /// A janela pode fechar sem perguntar de novo (a pessoa já respondeu).
    liberado: bool,
}

impl<A> Default for Guarda<A> {
    fn default() -> Self {
        Guarda {
            pendente: None,
            liberado: false,
        }
    }
}

impl<A> Guarda<A> {
    /// Há uma pergunta na tela.
    pub fn perguntando(&self) -> bool {
        self.pendente.is_some()
    }

    /// O app quer fazer `acao`, que perderia o documento de agora. Sem mudanças por salvar,
    /// devolve a ação para fazer já; com mudanças, guarda e pergunta no próximo quadro.
    pub fn pedir(&mut self, sujo: bool, acao: A) -> Option<A> {
        if sujo {
            self.pendente = Some(Pedido::Acao(acao));
            None
        } else {
            Some(acao)
        }
    }

    /// A janela pediu para fechar: com mudanças por salvar, segura e pergunta. Devolve se
    /// segurou. (Separado de [`Guarda::quadro`] para os testes, que não têm janela.)
    pub fn fechar_pedido(&mut self, sujo: bool) -> bool {
        if sujo && !self.liberado {
            self.pendente = Some(Pedido::Fechar);
            true
        } else {
            false
        }
    }

    /// A resposta da pessoa, vinda do diálogo (ou de um teste).
    pub fn responder(&mut self, r: Resposta) -> Option<Evento<A>> {
        match r {
            Resposta::Salvar => self.pendente.is_some().then_some(Evento::Salvar),
            Resposta::NaoSalvar => {
                let p = self.pendente.take()?;
                self.liberado |= matches!(p, Pedido::Fechar);
                Some(Evento::Seguir(p))
            }
            Resposta::Cancelar => {
                self.pendente = None;
                None
            }
        }
    }

    /// O app salvou depois de um [`Evento::Salvar`]: devolve o que estava esperando.
    pub fn salvou(&mut self) -> Option<Pedido<A>> {
        let p = self.pendente.take()?;
        self.liberado |= matches!(p, Pedido::Fechar);
        Some(p)
    }

    /// A pessoa desistiu de salvar (fechou o "Salvar como") ou o salvar falhou: nada acontece.
    pub fn cancelar(&mut self) {
        self.pendente = None;
    }

    /// Uma vez por quadro: segura o fechar da janela quando há mudanças por salvar, desenha
    /// a pergunta e devolve a resposta. Quando o pedido é fechar e a pessoa não quis salvar,
    /// a janela já fecha daqui; depois de salvar, chame [`Guarda::fechar_agora`] se
    /// [`Guarda::salvou`] devolver [`Pedido::Fechar`].
    pub fn quadro(&mut self, ctx: &Context, sujo: bool) -> Option<Evento<A>>
    where
        A: PartialEq,
    {
        if ctx.input(|i| i.viewport().close_requested()) && self.fechar_pedido(sujo) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        }
        self.pendente.as_ref()?;
        let evento = pergunta(ctx).and_then(|r| self.responder(r));
        if evento == Some(Evento::Seguir(Pedido::Fechar)) {
            self.fechar_agora(ctx);
        }
        evento
    }

    /// Fecha a janela sem perguntar de novo.
    pub fn fechar_agora(&mut self, ctx: &Context) {
        self.liberado = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

impl<A: PartialEq> PartialEq for Guarda<A> {
    fn eq(&self, outro: &Self) -> bool {
        self.pendente == outro.pendente && self.liberado == outro.liberado
    }
}

/// O diálogo "Salvar / Não salvar / Cancelar", no meio da janela. Esc cancela.
pub fn pergunta(ctx: &Context) -> Option<Resposta> {
    let mut resposta = None;
    egui::Window::new(tx("documento-pergunta_titulo"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(tx("documento-pergunta_texto"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(tx("documento-salvar")).strong())
                    .clicked()
                {
                    resposta = Some(Resposta::Salvar);
                }
                if ui.button(tx("documento-nao_salvar")).clicked() {
                    resposta = Some(Resposta::NaoSalvar);
                }
                if ui.button(tx("documento-cancelar")).clicked() {
                    resposta = Some(Resposta::Cancelar);
                }
            });
        });
    if resposta.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        resposta = Some(Resposta::Cancelar);
    }
    resposta
}

/// Como está o documento, para a barra de estado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Situacao {
    /// Nunca foi salvo, ou tem mudanças por salvar.
    NaoSalvo,
    /// Tudo o que está na tela está no disco.
    Salvo,
}

/// O indicador da barra de estado: "Não salvo" em destaque, ou "Salvo" discreto.
pub fn indicador(ui: &mut Ui, s: Situacao) {
    match s {
        Situacao::NaoSalvo => {
            ui.colored_label(ui.visuals().warn_fg_color, tx("documento-nao_salvo"))
                .on_hover_text(tx("documento-nao_salvo_ajuda"));
        }
        Situacao::Salvo => {
            ui.label(RichText::new(tx("documento-salvo")).weak());
        }
    }
}

/// O título da janela: `nome — app`, com `•` na frente quando há mudanças por salvar e
/// "Sem nome" quando o documento ainda não tem nome.
pub fn titulo_da_janela(app: &str, nome: Option<&str>, sujo: bool) -> String {
    let nome = nome
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(tx("documento-sem_nome"));
    let marca = if sujo { "• " } else { "" };
    format!("{marca}{nome} — {app}")
}

/// O que a pessoa clicou na [`barra`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clique {
    Salvar,
    SalvarComo,
    /// Ligou ou desligou o salvamento automático.
    Automatico(bool),
}

/// A chave liga/desliga, no desenho de sempre: trilho arredondado e bolinha que desliza.
fn chave(ui: &mut Ui, ligado: &mut bool) -> egui::Response {
    let tamanho = ui.spacing().interact_size.y * egui::vec2(1.8, 0.9);
    let (rect, mut resp) = ui.allocate_exact_size(tamanho, egui::Sense::click());
    if resp.clicked() {
        *ligado = !*ligado;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let quanto = ui.ctx().animate_bool_responsive(resp.id, *ligado);
        let v = ui.style().interact_selectable(&resp, *ligado);
        let raio = rect.height() / 2.0;
        ui.painter()
            .rect(rect, raio, v.bg_fill, v.bg_stroke, egui::StrokeKind::Inside);
        let x = egui::lerp((rect.left() + raio)..=(rect.right() - raio), quanto);
        ui.painter().circle(
            egui::pos2(x, rect.center().y),
            0.75 * raio,
            v.fg_stroke.color,
            egui::Stroke::NONE,
        );
    }
    resp
}

/// A barra do documento, para o canto de cima à esquerda da janela: a chave "Salvamento
/// automático", os botões Salvar e Salvar como e o indicador. O app decide o que cada clique
/// faz:
///
/// - ligar a chave num documento que nunca foi salvo abre o Salvar (para pedir nome e lugar);
/// - ligada, o app grava sozinho a cada mudança; desligada, só no Salvar, e a [`Guarda`]
///   volta a perguntar ao fechar com mudanças não gravadas;
/// - a escolha é guardada por documento (na configuração do app, pelo caminho).
pub fn barra(ui: &mut Ui, automatico: bool, situacao: Situacao) -> Option<Clique> {
    let mut clique = None;
    ui.horizontal(|ui| {
        let mut ligado = automatico;
        let r = chave(ui, &mut ligado).on_hover_text(tx("documento-automatico_ajuda"));
        let rotulo = ui
            .add(egui::Label::new(tx("documento-automatico")).sense(egui::Sense::click()))
            .on_hover_text(tx("documento-automatico_ajuda"));
        if rotulo.clicked() {
            ligado = !ligado;
        }
        if r.changed() || ligado != automatico {
            clique = Some(Clique::Automatico(ligado));
        }
        ui.separator();
        if ui.button(tx("documento-salvar")).clicked() {
            clique = Some(Clique::Salvar);
        }
        if ui.button(tx("documento-salvar_como")).clicked() {
            clique = Some(Clique::SalvarComo);
        }
        indicador(ui, situacao);
    });
    clique
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    enum Acao {
        Novo,
        Abrir(&'static str),
    }

    #[test]
    fn sem_mudancas_a_acao_sai_na_hora() {
        let mut g = Guarda::default();
        assert_eq!(g.pedir(false, Acao::Novo), Some(Acao::Novo));
        assert!(!g.perguntando());
        assert!(!g.fechar_pedido(false));
    }

    #[test]
    fn com_mudancas_pergunta_e_cada_resposta_faz_o_seu() {
        // Não salvar: a ação segue.
        let mut g = Guarda::default();
        assert_eq!(g.pedir(true, Acao::Abrir("a")), None);
        assert!(g.perguntando());
        assert_eq!(
            g.responder(Resposta::NaoSalvar),
            Some(Evento::Seguir(Pedido::Acao(Acao::Abrir("a"))))
        );
        assert!(!g.perguntando());

        // Cancelar: nada acontece.
        g.pedir(true, Acao::Novo);
        assert_eq!(g.responder(Resposta::Cancelar), None);
        assert!(!g.perguntando());

        // Salvar: a ação espera o app salvar.
        g.pedir(true, Acao::Novo);
        assert_eq!(g.responder(Resposta::Salvar), Some(Evento::Salvar));
        assert!(g.perguntando());
        assert_eq!(g.salvou(), Some(Pedido::Acao(Acao::Novo)));
        assert!(!g.perguntando());

        // Salvar, mas a pessoa desistiu no "Salvar como".
        g.pedir(true, Acao::Novo);
        g.responder(Resposta::Salvar);
        g.cancelar();
        assert!(!g.perguntando());
        assert_eq!(g.salvou(), None);
    }

    #[test]
    fn fechar_com_mudancas_segura_uma_vez() {
        let mut g: Guarda<Acao> = Guarda::default();
        assert!(g.fechar_pedido(true));
        assert_eq!(
            g.responder(Resposta::NaoSalvar),
            Some(Evento::Seguir(Pedido::Fechar))
        );
        // A janela fecha agora, mesmo ainda suja: não pergunta de novo.
        assert!(!g.fechar_pedido(true));

        let mut g: Guarda<Acao> = Guarda::default();
        assert!(g.fechar_pedido(true));
        g.responder(Resposta::Salvar);
        assert_eq!(g.salvou(), Some(Pedido::Fechar));
        assert!(!g.fechar_pedido(true));
    }

    #[test]
    fn titulo_e_textos_nas_cinco_linguas() {
        use utsim_idiomas::Idioma;
        utsim_idiomas::definir_nesta_thread(Some(Idioma::PtBr));
        assert_eq!(
            titulo_da_janela("UTSim Produto", Some("Bicicleta"), false),
            "Bicicleta — UTSim Produto"
        );
        assert_eq!(
            titulo_da_janela("UTSim Produto", None, true),
            "• Sem nome — UTSim Produto"
        );
        let mut vistos = Vec::new();
        for i in Idioma::TODOS {
            utsim_idiomas::definir_nesta_thread(Some(i));
            for c in [
                "documento-pergunta_titulo",
                "documento-pergunta_texto",
                "documento-salvar",
                "documento-salvar_como",
                "documento-nao_salvar",
                "documento-cancelar",
                "documento-nao_salvo",
                "documento-nao_salvo_ajuda",
                "documento-salvo",
                "documento-sem_nome",
                "documento-salve_primeiro",
                "documento-automatico",
                "documento-automatico_ajuda",
            ] {
                assert!(!tx(c).starts_with("documento-"), "{i:?} {c}");
            }
            vistos.push(tx("documento-nao_salvar"));
        }
        vistos.dedup();
        assert_eq!(vistos.len(), Idioma::TODOS.len(), "{vistos:?}");
        utsim_idiomas::definir_nesta_thread(None);
    }
}
