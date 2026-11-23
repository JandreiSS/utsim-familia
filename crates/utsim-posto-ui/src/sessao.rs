//! A unidade aberta por uma pessoa, o autor lembrado em arquivo e a atualização periódica
//! (a pasta é compartilhada pelo Drive, outras pessoas gravam operações nela).

use crate::texto::{fx, tx, vx};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use utsim_posto::Unidade;

/// Intervalo da atualização automática.
pub const INTERVALO: Duration = Duration::from_secs(5);

/// Configuração do usuário, lembrada entre execuções.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub autor: String,
    #[serde(default)]
    pub ultima_unidade: String,
    /// As unidades abertas por último, a mais nova primeiro.
    #[serde(default)]
    pub recentes: Vec<String>,
    /// A língua da interface (`pt-BR`, `en`, `es`, `fr`, `it`); sem ela, a do sistema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idioma: Option<String>,
}

impl Config {
    /// As unidades recentes (com a última unidade das versões antigas, se faltar).
    pub fn recentes(&self) -> Vec<PathBuf> {
        let mut l: Vec<PathBuf> = self.recentes.iter().map(PathBuf::from).collect();
        let u = self.ultima_unidade.trim();
        if !u.is_empty() && !self.recentes.iter().any(|r| r == u) {
            l.push(PathBuf::from(u));
        }
        l
    }

    pub fn ler(arquivo: &Path) -> Config {
        utsim_pasta::config::ler(arquivo)
    }

    pub fn gravar(&self, arquivo: &Path) -> Result<(), String> {
        utsim_pasta::config::gravar(arquivo, self)
    }
}

/// Arquivo de configuração padrão dos dois apps (`.../UTSim/qualidade_seguranca.json`).
pub fn arquivo_padrao() -> Option<PathBuf> {
    utsim_pasta::config::arquivo("qualidade_seguranca.json")
}

pub struct Sessao {
    pub config: Config,
    arquivo: Option<PathBuf>,
    pub unidade: Option<Unidade>,
    /// Última mensagem para a barra de estado e se é um erro.
    pub mensagem: String,
    pub erro: bool,
    ultima: Instant,
    /// Quantas vezes a unidade mudou (por atualização ou gravação), para as telas
    /// descartarem rascunhos velhos.
    pub versao: u64,
}

impl Sessao {
    pub fn nova(arquivo: Option<PathBuf>) -> Sessao {
        let config = arquivo.as_deref().map(Config::ler).unwrap_or_default();
        Sessao {
            config,
            arquivo,
            unidade: None,
            mensagem: String::new(),
            erro: false,
            ultima: Instant::now(),
            versao: 0,
        }
    }

    /// O arquivo de configuração (a unidade de análises avulsas fica ao lado).
    pub fn arquivo_config(&self) -> Option<&Path> {
        self.arquivo.as_deref()
    }

    pub fn avisar(&mut self, texto: impl Into<String>) {
        self.mensagem = texto.into();
        self.erro = false;
    }

    pub fn falhar(&mut self, texto: impl Into<String>) {
        self.mensagem = texto.into();
        self.erro = true;
    }

    fn lembrar(&mut self, autor: &str, caminho: &Path) {
        self.config.autor = autor.to_string();
        self.config.ultima_unidade = caminho.display().to_string();
        utsim_ui_tema::abertura::lembrar(&mut self.config.recentes, caminho);
        self.gravar_config();
    }

    /// Põe a língua guardada na configuração; sem ela, fica a de agora (a do sistema,
    /// posta pelo `main`).
    pub fn usar_idioma_guardado(&self) {
        if let Some(i) = self
            .config
            .idioma
            .as_deref()
            .and_then(utsim_idiomas::Idioma::de_codigo)
        {
            utsim_idiomas::definir(i);
        }
    }

    /// O seletor de língua (no pé da barra lateral): a escolhida vale na hora e fica
    /// guardada na configuração.
    pub fn seletor_de_idioma(&mut self, ui: &mut egui::Ui) {
        let mut i = utsim_idiomas::atual();
        if utsim_idiomas::seletor(ui, "idioma", &mut i) {
            utsim_idiomas::definir(i);
            self.config.idioma = Some(i.codigo().to_string());
            self.gravar_config();
        }
    }

    /// Grava a configuração (avisa na barra se não der).
    pub fn gravar_config(&mut self) {
        if let Some(a) = self.arquivo.clone()
            && let Err(e) = self.config.gravar(&a)
        {
            self.falhar(fx(
                "sessao-nao_consegui_lembrar_a_configuracao",
                &[("e", vx(&e))],
            ));
        }
    }

    /// Guarda o nome do autor para a próxima vez.
    pub fn lembrar_autor(&mut self, autor: &str) {
        if self.erro {
            self.avisar("");
        }
        if self.config.autor != autor {
            self.config.autor = autor.to_string();
            self.gravar_config();
        }
    }

    /// Tira da lista de recentes a unidade de índice `i` (de [`Config::recentes`]).
    pub fn esquecer_recente(&mut self, i: usize) {
        let Some(p) = self
            .config
            .recentes()
            .get(i)
            .map(|p| p.display().to_string())
        else {
            return;
        };
        self.config.recentes.retain(|r| r != &p);
        if self.config.ultima_unidade == p {
            self.config.ultima_unidade.clear();
        }
        self.gravar_config();
    }

    pub fn abrir(&mut self, caminho: &Path, autor: &str) -> bool {
        match Unidade::abrir(caminho, autor.trim()) {
            Ok(u) => {
                let msg = fx("sessao-unidade_aberta", &[("nome", vx(&u.manifesto.nome))]);
                self.unidade = Some(u);
                self.versao += 1;
                // Um erro antigo (outra pasta, nome faltando) não vale mais.
                self.avisar(msg);
                self.lembrar(autor.trim(), caminho);
                true
            }
            Err(e) => {
                self.falhar(e.to_string());
                false
            }
        }
    }

    pub fn criar(&mut self, caminho: &Path, nome: &str, autor: &str) -> bool {
        match Unidade::criar(caminho, nome.trim(), autor.trim()) {
            Ok(u) => {
                let msg = fx("sessao-unidade_criada", &[("nome", vx(&u.manifesto.nome))]);
                self.unidade = Some(u);
                self.versao += 1;
                // Um erro antigo (outra pasta, nome faltando) não vale mais.
                self.avisar(msg);
                self.lembrar(autor.trim(), caminho);
                true
            }
            Err(e) => {
                self.falhar(e.to_string());
                false
            }
        }
    }

    /// A unidade aberta tem algum posto ligado a um modelo (a tela de entrada dos apps
    /// passa a ser a aba "Modelo").
    pub fn tem_modelo(&self) -> bool {
        self.unidade
            .as_ref()
            .is_some_and(|u| u.postos().iter().any(|p| p.modelo.is_some()))
    }

    pub fn fechar(&mut self) {
        self.unidade = None;
        self.versao += 1;
    }

    /// Lê as operações novas da pasta. Devolve quantas vieram.
    pub fn atualizar(&mut self) -> usize {
        self.ultima = Instant::now();
        let Some(u) = self.unidade.as_mut() else {
            return 0;
        };
        match u.atualizar() {
            Ok(n) => {
                if n > 0 {
                    self.versao += 1;
                    self.avisar(fx(
                        "sessao-operacoes_novas_de_outras_pessoas",
                        &[("n", vx(&n))],
                    ));
                }
                n
            }
            Err(e) => {
                self.falhar(e.to_string());
                0
            }
        }
    }

    /// Atualiza sozinho a cada [`INTERVALO`] e pede o próximo quadro.
    pub fn atualizar_se_preciso(&mut self, ctx: &egui::Context) {
        if self.unidade.is_none() {
            return;
        }
        if self.ultima.elapsed() >= INTERVALO {
            self.atualizar();
        }
        ctx.request_repaint_after(INTERVALO);
    }

    /// Anota que a unidade mudou por uma gravação desta pessoa.
    pub fn mudou(&mut self) {
        self.versao += 1;
    }

    /// Resultado de uma gravação: mostra o erro ou a mensagem de sucesso.
    pub fn resultado<T>(&mut self, r: Result<T, impl std::fmt::Display>, ok: &str) -> Option<T> {
        match r {
            Ok(v) => {
                self.mudou();
                self.avisar(ok);
                Some(v)
            }
            Err(e) => {
                self.falhar(e.to_string());
                None
            }
        }
    }

    /// Desfaz a última gravação desta pessoa na unidade (nesta sessão do app).
    pub fn desfazer(&mut self) {
        let r = match self.unidade.as_mut() {
            Some(u) => u.desfazer(),
            None => return,
        };
        self.depois_de_voltar(r, "sessao-desfeito");
    }

    /// Refaz o último passo desfeito.
    pub fn refazer(&mut self) {
        let r = match self.unidade.as_mut() {
            Some(u) => u.refazer(),
            None => return,
        };
        self.depois_de_voltar(r, "sessao-refeito");
    }

    fn depois_de_voltar(
        &mut self,
        r: Result<utsim_posto::Desfeito, utsim_posto::UnidadeErro>,
        ok: &'static str,
    ) {
        match r {
            Ok(utsim_posto::Desfeito::Nada) => {}
            Ok(utsim_posto::Desfeito::Feito(n)) => {
                self.mudou();
                self.avisar(fx(ok, &[("n", vx(&n))]));
            }
            Ok(utsim_posto::Desfeito::Mudou) => self.falhar(tx("sessao-desfazer_mudou")),
            Err(e) => {
                // O que chegou a ser gravado já está na unidade.
                self.mudou();
                self.falhar(e.to_string());
            }
        }
    }

    /// Botões Desfazer e Refazer das gravações na unidade. Com `atalhos`, também Ctrl+Z,
    /// Ctrl+Y e Ctrl+Shift+Z fora dos campos de texto (a tela Modelo tem os seus).
    pub fn desfazer_ui(&mut self, ui: &mut egui::Ui, atalhos: bool) {
        let Some(u) = &self.unidade else {
            return;
        };
        let (pd, pr) = (u.pode_desfazer(), u.pode_refazer());
        let mut desfazer = ui
            .add_enabled(pd, egui::Button::new(tx("sessao-desfazer")))
            .on_hover_text(tx("sessao-desfazer_dica"))
            .clicked();
        let mut refazer = ui
            .add_enabled(pr, egui::Button::new(tx("sessao-refazer")))
            .on_hover_text(tx("sessao-refazer_dica"))
            .clicked();
        if atalhos && !ui.ctx().egui_wants_keyboard_input() {
            use egui::{Key, Modifiers};
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::COMMAND, Key::Y)
                {
                    refazer = true;
                } else if i.consume_key(Modifiers::COMMAND, Key::Z) {
                    desfazer = true;
                }
            });
        }
        if desfazer {
            self.desfazer();
        } else if refazer {
            self.refazer();
        }
    }

    /// Barra de estado com a última mensagem.
    pub fn barra(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            match &self.unidade {
                Some(u) => {
                    ui.label(fx(
                        "sessao-unidade_autor_operacoes",
                        &[
                            ("nome", vx(&u.manifesto.nome)),
                            ("autor", vx(&u.autor())),
                            ("total_operacoes", vx(&u.total_operacoes())),
                        ],
                    ));
                }
                None => {
                    ui.label(tx("sessao-nenhuma_unidade_aberta"));
                }
            }
            if !self.mensagem.is_empty() {
                ui.separator();
                let cor = if self.erro {
                    egui::Color32::from_rgb(230, 90, 80)
                } else {
                    ui.visuals().text_color()
                };
                ui.colored_label(cor, &self.mensagem);
            }
        });
    }
}

/// As linhas curtas da barra lateral dos apps: a unidade aberta e o autor, ou o modelo
/// aberto sem unidade (o pedido do 3D).
pub fn linhas_da_barra(s: &Sessao, modelo: &crate::TelaModelo) -> Vec<String> {
    if let Some(u) = &s.unidade {
        return vec![
            u.manifesto.nome.clone(),
            fx("sessao-autor", &[("autor", vx(&u.autor()))]),
        ];
    }
    match modelo.arquivo().and_then(|a| a.file_name()) {
        Some(n) => vec![
            tx("sessao-sem_unidade").to_string(),
            fx("sessao-modelo", &[("nome", vx(&n.to_string_lossy()))]),
        ],
        None => vec![tx("sessao-nenhuma_unidade_aberta").to_string()],
    }
}
