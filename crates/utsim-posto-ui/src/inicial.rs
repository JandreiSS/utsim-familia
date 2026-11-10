//! A abertura dos apps de unidade (Qualidade, Segurança do Trabalho e Estatística): quem é
//! o autor, abrir ou criar a pasta de unidade, as unidades recentes e onde tudo fica salvo.
//! O desenho é a abertura comum da família ([`utsim_ui_tema::abertura`]).

use std::path::{Path, PathBuf};

use egui::{Color32, RichText};
use utsim_posto::Unidade;
use utsim_ui_tema::abertura::{Abertura, Acao, Escolha, Onde, Recente, Rotulos, Simbolo};
use utsim_ui_tema::arquivos::{self, Tipo};

use crate::sessao::Sessao;

/// O nome, o subtítulo, a cor e o ícone do app (os mesmos do lançador).
#[derive(Clone, Copy, Debug)]
pub struct Marca {
    pub nome: &'static str,
    pub subtitulo: &'static str,
    pub cor: Color32,
    pub icone: Option<&'static [u8]>,
    /// O que "Abrir modelo do UTSim" faz neste app, numa linha.
    pub modelo: &'static str,
}

/// O que a abertura pede ao app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Saida {
    /// Uma unidade acabou de ser aberta ou criada.
    Aberta,
    /// Seguir para o trabalho na unidade já aberta.
    Continuar,
    /// Abrir este modelo do UTSim (a unidade já está aberta).
    Modelo(PathBuf),
    /// Uma ação própria do app (de `extras` em [`TelaInicial::ui_com`]).
    Acao(&'static str),
}

/// O formulário de criar unidade (uma janela por cima da abertura).
#[derive(Default)]
struct Criar {
    nome: String,
    pasta: String,
}

#[derive(Default)]
pub struct TelaInicial {
    pub autor: String,
    editando_autor: bool,
    criar: Option<Criar>,
    iniciada: bool,
}

const EXPLICACAO: &str = "A unidade é uma pasta comum do computador (uma fábrica, um setor ou \
    um laboratório). Tudo o que você grava aqui fica dentro dela. Para trabalhar em equipe, \
    deixe a pasta numa pasta sincronizada do Google Drive: o Qualidade, o Segurança do Trabalho \
    e o Estatística abrem a mesma unidade, e cada um vê o que os outros gravaram.";

impl TelaInicial {
    /// Desenha a abertura. Devolve o que o app deve fazer em seguida, se algo.
    pub fn ui(&mut self, ui: &mut egui::Ui, s: &mut Sessao, marca: &Marca) -> Option<Saida> {
        self.ui_com(ui, s, marca, &[], |_| {})
    }

    /// A abertura com ações próprias do app (`extras`, mostradas logo depois de
    /// "Continuar" quando há unidade aberta) e algo do app no fim da tela (`rodape`).
    pub fn ui_com(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Sessao,
        marca: &Marca,
        extras: &[Acao<'_>],
        rodape: impl FnOnce(&mut egui::Ui),
    ) -> Option<Saida> {
        if !self.iniciada {
            self.iniciada = true;
            if self.autor.is_empty() {
                self.autor = s.config.autor.clone();
            }
            self.editando_autor = self.autor.trim().is_empty();
        }
        let sem_autor = self.autor.trim().is_empty();
        let precisa_nome = "Escreva seu nome acima primeiro: ele assina o que você grava.";
        let sem_unidade = "Abra ou crie uma unidade primeiro: o modelo fica ligado a ela.";
        let aberta = s.unidade.as_ref().map(|u| u.manifesto.nome.clone());
        let raiz = s.unidade.as_ref().map(|u| u.raiz().to_path_buf());
        let continuar = aberta
            .as_ref()
            .map(|n| format!("Continuar em \"{n}\""))
            .unwrap_or_default();
        let mut acoes = Vec::new();
        if aberta.is_some() {
            acoes.push(
                Acao::nova(
                    "continuar",
                    Simbolo::Continuar,
                    &continuar,
                    "Volte ao trabalho na unidade aberta.",
                )
                .principal(),
            );
            acoes.extend_from_slice(extras);
        }
        acoes.push(
            Acao::nova(
                "abrir",
                Simbolo::Abrir,
                if aberta.is_some() {
                    "Abrir outra unidade"
                } else {
                    "Abrir unidade"
                },
                "Escolha a pasta de uma unidade que já existe, por exemplo a da equipe no Drive.",
            )
            .bloqueada_se(sem_autor, precisa_nome),
        );
        acoes.push(
            Acao::nova(
                "criar",
                Simbolo::Novo,
                "Criar unidade",
                "Comece uma unidade nova: você dá o nome e escolhe onde fica a pasta.",
            )
            .bloqueada_se(sem_autor, precisa_nome),
        );
        acoes.push(
            Acao::nova(
                "modelo",
                Simbolo::Modelo,
                "Abrir modelo do UTSim",
                marca.modelo,
            )
            .bloqueada_se(aberta.is_none(), sem_unidade),
        );
        if aberta.is_none()
            && let Some(a) = acoes.first_mut()
        {
            *a = a.principal();
        }
        let recentes: Vec<Recente> = s.config.recentes().into_iter().map(Recente::de).collect();
        let abertura = Abertura {
            nome: marca.nome,
            subtitulo: marca.subtitulo,
            cor: marca.cor,
            icone: marca.icone,
            acoes: &acoes,
            titulo_recentes: "Unidades recentes",
            recentes: &recentes,
            onde: Onde {
                caminho: raiz.as_deref(),
                explicacao: EXPLICACAO,
            },
            rotulos: Rotulos::PT,
        };
        let erro = (s.erro && !s.mensagem.is_empty()).then(|| s.mensagem.clone());
        let escolha = abertura.mostrar(
            ui,
            |ui| self.autor_ui(ui, s, marca.cor),
            |ui| {
                if let Some(e) = erro {
                    ui.add_space(10.0);
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                rodape(ui);
            },
        );
        let mut saida = None;
        match escolha {
            Some(Escolha::Acao("continuar")) => saida = Some(Saida::Continuar),
            Some(Escolha::Acao("abrir")) => {
                let inicio = raiz
                    .as_ref()
                    .and_then(|r| r.parent())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                if let Some(p) =
                    arquivos::procurar(Tipo::Pasta, "Escolha a pasta da unidade", &[], &inicio)
                {
                    saida = self.abrir(s, &p);
                }
            }
            Some(Escolha::Acao("criar")) => {
                self.criar = Some(Criar {
                    nome: String::new(),
                    pasta: arquivos::documentos()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                });
            }
            Some(Escolha::Acao("modelo")) => {
                let inicio = raiz
                    .as_ref()
                    .map(|r| r.display().to_string())
                    .unwrap_or_default();
                if let Some(p) = arquivos::procurar(
                    Tipo::Abrir,
                    "Abrir modelo do UTSim",
                    &[arquivos::RON],
                    &inicio,
                ) {
                    saida = Some(Saida::Modelo(p));
                }
            }
            Some(Escolha::Recente(i)) => {
                if let Some(p) = s.config.recentes().get(i).cloned() {
                    if sem_autor {
                        s.falhar(precisa_nome);
                    } else {
                        saida = self.abrir(s, &p);
                    }
                }
            }
            Some(Escolha::Esquecer(i)) => s.esquecer_recente(i),
            Some(Escolha::MostrarPasta(p)) => {
                if let Err(e) = arquivos::mostrar_no_explorador(&p) {
                    s.falhar(format!("não consegui abrir o explorador: {e}"));
                }
            }
            Some(Escolha::Acao(id)) => saida = Some(Saida::Acao(id)),
            None => {}
        }
        if let Some(r) = self.janela_criar(ui.ctx(), s) {
            saida = Some(r);
        }
        saida
    }

    fn abrir(&mut self, s: &mut Sessao, p: &Path) -> Option<Saida> {
        if !Unidade::eh_unidade(p) {
            s.falhar(format!(
                "A pasta {} não é uma unidade. Escolha a pasta que tem o arquivo unidade.json, \
                 ou use \"Criar unidade\".",
                p.display()
            ));
            return None;
        }
        s.abrir(p, &self.autor).then_some(Saida::Aberta)
    }

    fn autor_ui(&mut self, ui: &mut egui::Ui, s: &mut Sessao, cor: Color32) {
        let fraco = ui.visuals().weak_text_color();
        if self.editando_autor {
            utsim_ui_tema::abertura::quadro_destaque(cor).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("Antes de começar, diga seu nome")
                        .strong()
                        .size(16.0),
                );
                ui.label(
                    RichText::new(
                        "Ele vai em tudo o que você gravar, para a equipe saber quem mudou o quê.",
                    )
                    .color(fraco),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.autor)
                            .hint_text("seu nome")
                            .desired_width(300.0),
                    );
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let ok = !self.autor.trim().is_empty();
                    if (ui
                        .add_enabled(ok, utsim_ui_tema::widgets::primary("Pronto"))
                        .clicked()
                        || enter)
                        && ok
                    {
                        self.editando_autor = false;
                        s.lembrar_autor(self.autor.trim());
                    }
                });
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Você está como").color(fraco));
                ui.label(RichText::new(self.autor.trim()).strong());
                if ui.small_button("Trocar").clicked() {
                    self.editando_autor = true;
                }
            });
        }
        ui.add_space(16.0);
    }

    fn janela_criar(&mut self, ctx: &egui::Context, s: &mut Sessao) -> Option<Saida> {
        let c = self.criar.as_mut()?;
        let mut fechar = false;
        let mut saida = None;
        let pasta_final = if c.nome.trim().is_empty() || c.pasta.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(c.pasta.trim()).join(nome_de_pasta(&c.nome)))
        };
        egui::Modal::new(egui::Id::new("criar_unidade")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading("Criar unidade");
            ui.add_space(8.0);
            ui.label("Nome da unidade (a fábrica, o setor ou o laboratório):");
            ui.add(
                egui::TextEdit::singleline(&mut c.nome)
                    .hint_text("por exemplo: Fábrica Norte")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(8.0);
            ui.label("Onde criar a pasta (para a equipe, escolha uma pasta do Google Drive):");
            arquivos::campo(
                ui,
                &mut c.pasta,
                Tipo::Pasta,
                "Onde criar a pasta da unidade",
                &[],
                420.0,
            );
            ui.add_space(8.0);
            match &pasta_final {
                Some(p) => {
                    ui.label(
                        RichText::new(format!("Será criada a pasta: {}", p.display()))
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                None => {
                    ui.label(
                        RichText::new("Escreva o nome e escolha onde fica.")
                            .color(ui.visuals().weak_text_color()),
                    );
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        pasta_final.is_some(),
                        utsim_ui_tema::widgets::primary("Criar unidade"),
                    )
                    .clicked()
                    && let Some(p) = &pasta_final
                    && s.criar(p, &c.nome, &self.autor)
                {
                    saida = Some(Saida::Aberta);
                    fechar = true;
                }
                if ui.button("Cancelar").clicked() {
                    fechar = true;
                }
            });
            if s.erro && !s.mensagem.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &s.mensagem);
            }
        });
        if fechar {
            self.criar = None;
        }
        saida
    }
}

/// O nome da unidade como nome de pasta (sem os caracteres que o Windows recusa).
pub fn nome_de_pasta(nome: &str) -> String {
    let n: String = nome
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let n = n.trim_end_matches(['.', ' ']).to_string();
    if n.is_empty() { "Unidade".into() } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_de_pasta() {
        assert_eq!(nome_de_pasta(" Fábrica Norte "), "Fábrica Norte");
        assert_eq!(nome_de_pasta("A/B: C?"), "A-B- C-");
        assert_eq!(nome_de_pasta("..."), "Unidade");
    }
}
