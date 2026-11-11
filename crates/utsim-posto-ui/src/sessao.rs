//! A unidade aberta por uma pessoa, o autor lembrado em arquivo e a atualização periódica
//! (a pasta é compartilhada pelo Drive, outras pessoas gravam operações nela).

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
        std::fs::read_to_string(arquivo)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn gravar(&self, arquivo: &Path) -> Result<(), String> {
        if let Some(pai) = arquivo.parent() {
            std::fs::create_dir_all(pai).map_err(|e| e.to_string())?;
        }
        let t = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(arquivo, t).map_err(|e| e.to_string())
    }
}

/// Arquivo de configuração padrão dos dois apps (`.../UTSim/qualidade_seguranca.json`).
pub fn arquivo_padrao() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("UTSim").join("qualidade_seguranca.json"))
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

    /// Grava a configuração (avisa na barra se não der).
    pub fn gravar_config(&mut self) {
        if let Some(a) = self.arquivo.clone()
            && let Err(e) = self.config.gravar(&a)
        {
            self.falhar(format!("não consegui lembrar a configuração: {e}"));
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
                let msg = format!("Unidade '{}' aberta.", u.manifesto.nome);
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
                let msg = format!("Unidade '{}' criada.", u.manifesto.nome);
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
                    self.avisar(format!("{n} operações novas de outras pessoas."));
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

    /// Barra de estado com a última mensagem.
    pub fn barra(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            match &self.unidade {
                Some(u) => {
                    ui.label(format!(
                        "Unidade: {} · autor: {} · {} operações",
                        u.manifesto.nome,
                        u.autor(),
                        u.total_operacoes()
                    ));
                }
                None => {
                    ui.label("Nenhuma unidade aberta");
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
        return vec![u.manifesto.nome.clone(), format!("autor: {}", u.autor())];
    }
    match modelo.arquivo().and_then(|a| a.file_name()) {
        Some(n) => vec![
            "Sem unidade".to_string(),
            format!("modelo: {}", n.to_string_lossy()),
        ],
        None => vec!["Nenhuma unidade aberta".to_string()],
    }
}
