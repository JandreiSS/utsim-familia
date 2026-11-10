//! Tela "Modelo" (mão dupla com o UTSim): abre o MESMO `.ron` do UTSim, mostra e edita os
//! campos de Qualidade, Segurança e Manutenção de cada nó (`utsim_edit::abas`) e grava de
//! volta no modelo (`docs/plano_integracao_familia.md`, seção 3).
//!
//! - Um escritor por modelo: ao abrir, a tela toma a trava ao lado do arquivo
//!   (`utsim_pasta::trava`). Se outro app ou pessoa edita, a tela fica só de leitura e
//!   diz quem edita.
//! - Gravação atômica do arquivo inteiro, depois renova a trava.
//! - Recarrega sozinha quando o arquivo muda no disco e não há mudança local pendente.
//! - Ao lado dos campos, um resumo por nó vindo da simulação do mesmo modelo, calculado
//!   por uma função que o app entrega (esta crate não depende dos `-core` dos apps).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use utsim_edit::abas::{self, Aba, Campo, Tipo};
use utsim_edit::{Command, Document};
use utsim_model::Model;
use utsim_pasta::trava::{self, Estado, RENOVAR_A_CADA_MS, agora_ms};

use crate::sessao::Sessao;
use crate::trabalho::Trabalho;

/// Resumo da simulação por nó: nome do objeto, depois linhas (rótulo, valor). A chave
/// vazia guarda as linhas do modelo inteiro.
pub type Resumo = BTreeMap<String, Vec<(String, String)>>;
/// Rótulo da linha do resumo de um nó que vai ao lado do nome na lista.
pub const DESTAQUE: &str = "Em destaque";
/// Função de resumo entregue pelo app (roda num fio de trabalho).
pub type FnResumo = Arc<dyn Fn(&Model) -> Result<Resumo, String> + Send + Sync>;

/// Nome do computador, para distinguir duas janelas da mesma pessoa.
pub fn maquina() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "maquina".to_string())
}

fn modificado(caminho: &Path) -> Option<SystemTime> {
    std::fs::metadata(caminho).and_then(|m| m.modified()).ok()
}

/// Hora do dia (UTC) de um instante em milissegundos, `HH:MM`.
fn hora(ms: u64) -> String {
    let s = ms / 1000 % 86_400;
    format!("{:02}:{:02}", s / 3600, s / 60 % 60)
}

enum Acao {
    Editar(String, String),
    Adicionar(String),
    Remover(String, usize),
}

pub struct TelaModelo {
    app: String,
    principal: Aba,
    resumir: FnResumo,
    maquina: String,
    pessoa: String,
    /// Caminho digitado ou escolhido.
    pub caminho: String,
    doc: Option<Document>,
    arquivo: Option<PathBuf>,
    estado: Option<Estado>,
    mtime: Option<SystemTime>,
    revisao_gravada: u64,
    /// O arquivo mudou no disco com mudança local pendente: o usuário decide.
    pub mudou_no_disco: bool,
    pub filtro: String,
    pub selecionado: String,
    aba: Aba,
    erros: HashMap<String, String>,
    rascunhos: HashMap<String, String>,
    fechados: HashSet<String>,
    ultima_renovacao: Option<f64>,
    ultima_checagem: f64,
    pub trabalho: Trabalho<Resumo>,
    pub mensagem: String,
    pub erro: bool,
    /// Pedido de abrir o nó na tela Ferramentas: (modelo, objeto). O app o atende.
    pub pedido_ferramentas: Option<(String, String)>,
    /// Estudos do nó escolhido: (versão da sessão, nó, quantos).
    estudos_no: Option<(u64, String, usize)>,
    /// O caminho do modelo aberto como os estudos o guardam (relativo à unidade).
    modelo_rel: String,
}

impl TelaModelo {
    /// `app` é `"qualidade"` ou `"seguranca"` (vai na trava). `principal` é a aba que o
    /// app edita. A Manutenção aparece sempre, só para leitura.
    pub fn nova(app: &str, principal: Aba, resumir: FnResumo) -> TelaModelo {
        TelaModelo {
            app: app.to_string(),
            principal,
            resumir,
            maquina: maquina(),
            pessoa: String::new(),
            caminho: String::new(),
            doc: None,
            arquivo: None,
            estado: None,
            mtime: None,
            revisao_gravada: 0,
            mudou_no_disco: false,
            filtro: String::new(),
            selecionado: String::new(),
            aba: principal,
            erros: HashMap::new(),
            rascunhos: HashMap::new(),
            fechados: HashSet::new(),
            ultima_renovacao: None,
            ultima_checagem: 0.0,
            trabalho: Trabalho::default(),
            mensagem: String::new(),
            erro: false,
            pedido_ferramentas: None,
            estudos_no: None,
            modelo_rel: String::new(),
        }
    }

    fn avisar(&mut self, t: impl Into<String>) {
        self.mensagem = t.into();
        self.erro = false;
    }

    fn falhar(&mut self, t: impl Into<String>) {
        self.mensagem = t.into();
        self.erro = true;
    }

    // ---- estado ----------------------------------------------------------------

    pub fn aberto(&self) -> bool {
        self.doc.is_some()
    }

    pub fn documento(&self) -> Option<&Document> {
        self.doc.as_ref()
    }

    pub fn arquivo(&self) -> Option<&Path> {
        self.arquivo.as_deref()
    }

    /// Só a trava nossa permite editar e gravar.
    pub fn pode_editar(&self) -> bool {
        matches!(self.estado, Some(Estado::Minha))
    }

    /// Quem edita, quando não somos nós.
    pub fn editor(&self) -> Option<&trava::Trava> {
        match &self.estado {
            Some(Estado::DeOutro(t)) => Some(t),
            _ => None,
        }
    }

    /// Aviso "Em edição por ..." quando outra pessoa ou app tem a trava.
    pub fn aviso_trava(&self) -> Option<String> {
        self.editor().map(|t| {
            format!(
                "Em edição por {} no {} desde {}",
                t.pessoa,
                t.app,
                hora(t.desde_ms)
            )
        })
    }

    /// Mudança local ainda não gravada.
    pub fn pendente(&self) -> bool {
        self.doc
            .as_ref()
            .is_some_and(|d| d.revision() != self.revisao_gravada)
    }

    // ---- abrir, fechar, trava ---------------------------------------------------

    /// Abre o modelo, toma a trava (ou fica só de leitura) e começa o resumo.
    pub fn abrir(&mut self, caminho: &Path, pessoa: &str) -> Result<(), String> {
        self.fechar();
        let doc = Document::open(caminho).map_err(|e| format!("{}: {e}", caminho.display()))?;
        let estado = trava::travar(caminho, &self.app, pessoa, &self.maquina, agora_ms())
            .map_err(|e| e.to_string())?;
        self.pessoa = pessoa.to_string();
        self.mtime = modificado(caminho);
        self.arquivo = Some(caminho.to_path_buf());
        self.caminho = caminho.display().to_string();
        self.doc = Some(doc);
        self.estado = Some(estado);
        self.revisao_gravada = 0;
        self.mudou_no_disco = false;
        self.ultima_renovacao = None;
        self.limpar_edicao();
        self.selecionado.clear();
        if let Some(aviso) = self.aviso_trava() {
            self.avisar(aviso);
        } else {
            self.avisar("Modelo aberto para edição.");
        }
        self.calcular_resumo();
        Ok(())
    }

    /// Abre o modelo já com o nó escolhido (o pedido do 3D pela linha de comando). Nó que
    /// não está no modelo: abre assim mesmo e avisa.
    pub fn abrir_em_no(
        &mut self,
        caminho: &Path,
        no: Option<&str>,
        pessoa: &str,
    ) -> Result<(), String> {
        self.abrir(caminho, pessoa)?;
        let Some(no) = no.map(str::trim).filter(|n| !n.is_empty()) else {
            return Ok(());
        };
        let existe = self
            .doc
            .as_ref()
            .is_some_and(|d| d.model().objects.iter().any(|o| o.name == no));
        if existe {
            self.selecionar(no);
        } else {
            let antes = std::mem::take(&mut self.mensagem);
            self.falhar(format!("{antes} O nó {no} não está no modelo."));
        }
        Ok(())
    }

    /// Fecha o modelo e solta a trava.
    pub fn fechar(&mut self) {
        self.soltar_trava();
        self.doc = None;
        self.arquivo = None;
        self.estado = None;
        self.mudou_no_disco = false;
        self.limpar_edicao();
    }

    fn soltar_trava(&mut self) {
        if matches!(self.estado, Some(Estado::Minha))
            && let Some(a) = &self.arquivo
        {
            let _ = trava::soltar(a, &self.app, &self.pessoa, &self.maquina);
        }
    }

    /// Solta a trava e deixa o modelo aberto só para leitura.
    pub fn soltar_edicao(&mut self) {
        if !self.pode_editar() {
            return;
        }
        self.soltar_trava();
        self.estado = None;
        self.avisar("Edição solta. O modelo segue aberto só para leitura.");
    }

    /// Tenta tomar a trava de novo. Com sucesso, relê o arquivo para partir do que há
    /// no disco (se não houver mudança local).
    pub fn tentar_editar(&mut self) -> bool {
        let Some(a) = self.arquivo.clone() else {
            return false;
        };
        match trava::travar(&a, &self.app, &self.pessoa, &self.maquina, agora_ms()) {
            Ok(Estado::Minha) => {
                self.estado = Some(Estado::Minha);
                self.ultima_renovacao = None;
                if !self.pendente() {
                    self.recarregar();
                }
                self.avisar("Modelo aberto para edição.");
                true
            }
            Ok(e) => {
                self.estado = Some(e);
                if let Some(av) = self.aviso_trava() {
                    self.falhar(av);
                }
                false
            }
            Err(e) => {
                self.falhar(e.to_string());
                false
            }
        }
    }

    /// Renova a trava a cada [`RENOVAR_A_CADA_MS`], pelo relógio (em segundos) do quadro.
    pub fn renovar_se_preciso(&mut self, agora_s: f64) {
        if !self.pode_editar() {
            return;
        }
        let Some(antes) = self.ultima_renovacao else {
            self.ultima_renovacao = Some(agora_s);
            return;
        };
        if agora_s - antes < RENOVAR_A_CADA_MS as f64 / 1000.0 {
            return;
        }
        self.ultima_renovacao = Some(agora_s);
        self.renovar_agora();
    }

    fn renovar_agora(&mut self) {
        let Some(a) = self.arquivo.clone() else {
            return;
        };
        match trava::renovar(&a, &self.app, &self.pessoa, &self.maquina, agora_ms()) {
            Ok(Estado::Minha) => {}
            Ok(e) => {
                self.estado = Some(e);
                if let Some(av) = self.aviso_trava() {
                    self.falhar(format!("Perdi a edição. {av}"));
                }
            }
            Err(e) => self.falhar(e.to_string()),
        }
    }

    // ---- disco -----------------------------------------------------------------

    /// Relê o arquivo (descarta a mudança local).
    pub fn recarregar(&mut self) {
        let Some(a) = self.arquivo.clone() else {
            return;
        };
        match Document::open(&a) {
            Ok(d) => {
                self.doc = Some(d);
                self.revisao_gravada = 0;
                self.mtime = modificado(&a);
                self.mudou_no_disco = false;
                self.limpar_edicao();
                self.avisar("Modelo recarregado do disco.");
                self.calcular_resumo();
            }
            Err(e) => self.falhar(format!("{}: {e}", a.display())),
        }
    }

    /// Vê se o arquivo mudou no disco. Sem mudança local pendente (ou em só leitura),
    /// recarrega sozinha. Com mudança pendente, só avisa.
    pub fn verificar_disco(&mut self) {
        let Some(a) = self.arquivo.clone() else {
            return;
        };
        if modificado(&a) == self.mtime {
            return;
        }
        if self.pode_editar() && self.pendente() {
            self.mudou_no_disco = true;
        } else {
            self.recarregar();
        }
    }

    /// Grava o modelo inteiro de uma vez, só com a trava nossa.
    pub fn gravar(&mut self) -> Result<(), String> {
        if !self.pode_editar() {
            return Err("O modelo está só para leitura: outra pessoa edita.".into());
        }
        let (Some(doc), Some(a)) = (self.doc.as_ref(), self.arquivo.clone()) else {
            return Err("Nenhum modelo aberto.".into());
        };
        utsim_pasta::write_replace(&a, doc.to_text().as_bytes()).map_err(|e| e.to_string())?;
        self.revisao_gravada = doc.revision();
        self.mtime = modificado(&a);
        self.mudou_no_disco = false;
        self.renovar_agora();
        self.avisar("Gravado no modelo.");
        self.calcular_resumo();
        Ok(())
    }

    // ---- edição ----------------------------------------------------------------

    fn limpar_edicao(&mut self) {
        self.erros.clear();
        self.rascunhos.clear();
    }

    fn aplicar(
        &mut self,
        chave: &str,
        f: impl FnOnce(&Model, &str) -> Result<Command, String>,
    ) -> Result<(), String> {
        if !self.pode_editar() {
            return Err("O modelo está só para leitura.".into());
        }
        let objeto = self.selecionado.clone();
        let doc = self.doc.as_mut().ok_or("Nenhum modelo aberto.")?;
        let r = f(doc.model(), &objeto).and_then(|c| doc.apply(c));
        match &r {
            Ok(()) => {
                self.erros.remove(chave);
                self.rascunhos.remove(chave);
            }
            Err(e) => {
                self.erros.insert(chave.to_string(), e.clone());
            }
        }
        r
    }

    /// Edita um campo do objeto selecionado. O erro (faixa, formato) volta sem aplicar.
    pub fn editar(&mut self, caminho: &str, valor: &str) -> Result<(), String> {
        self.aplicar(caminho, |m, o| abas::editar(m, o, caminho, valor))
    }

    pub fn adicionar(&mut self, lista: &str) -> Result<(), String> {
        self.aplicar(lista, |m, o| abas::adicionar(m, o, lista))
    }

    pub fn remover(&mut self, lista: &str, indice: usize) -> Result<(), String> {
        self.aplicar(lista, |m, o| abas::remover(m, o, lista, indice))
    }

    /// Erro mostrado ao lado de um campo.
    pub fn erro_do_campo(&self, caminho: &str) -> Option<&str> {
        self.erros.get(caminho).map(String::as_str)
    }

    pub fn desfazer(&mut self) {
        if self.pode_editar()
            && let Some(d) = self.doc.as_mut()
        {
            d.undo();
            self.limpar_edicao();
        }
    }

    pub fn refazer(&mut self) {
        if self.pode_editar()
            && let Some(d) = self.doc.as_mut()
        {
            d.redo();
            self.limpar_edicao();
        }
    }

    /// Campos de uma aba do objeto selecionado.
    pub fn campos(&self, aba: Aba) -> Vec<Campo> {
        let Some(d) = &self.doc else {
            return Vec::new();
        };
        abas::campos(d.model(), &self.selecionado, aba).unwrap_or_default()
    }

    /// Nomes dos objetos do modelo que passam pelo filtro.
    pub fn objetos(&self) -> Vec<String> {
        let f = self.filtro.trim().to_lowercase();
        self.doc
            .as_ref()
            .map(|d| {
                d.model()
                    .objects
                    .iter()
                    .map(|o| o.name.clone())
                    .filter(|n| f.is_empty() || n.to_lowercase().contains(&f))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn selecionar(&mut self, nome: &str) {
        if self.selecionado != nome {
            self.selecionado = nome.to_string();
            self.limpar_edicao();
        }
    }

    // ---- resumo ----------------------------------------------------------------

    /// Roda a função de resumo num fio, sobre o modelo como está agora.
    pub fn calcular_resumo(&mut self) {
        let Some(d) = &self.doc else {
            return;
        };
        let modelo = d.model().clone();
        let f = Arc::clone(&self.resumir);
        self.trabalho.iniciar(move || f(&modelo));
    }

    // ---- tela ------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        // Sem unidade, só o modelo aberto pela linha de comando (o pedido do 3D).
        if s.unidade.is_none() && !self.aberto() {
            ui.label("Abra uma unidade na tela inicial.");
            return;
        }
        let pessoa = match s.unidade.as_ref() {
            Some(u) => u.autor().to_string(),
            None => self.pessoa.clone(),
        };
        let modelos: Vec<String> = {
            let mut v: Vec<String> = s
                .unidade
                .as_ref()
                .map(|u| u.postos())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| p.modelo)
                .collect();
            v.sort();
            v.dedup();
            v
        };
        let agora = ui.ctx().input(|i| i.time);
        self.renovar_se_preciso(agora);
        if agora - self.ultima_checagem >= 1.0 || self.ultima_checagem > agora {
            self.ultima_checagem = agora;
            self.verificar_disco();
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
        if self.trabalho.sondar(ui.ctx())
            && let Some(e) = self.trabalho.erro()
        {
            let e = e.to_string();
            self.falhar(format!("Resumo da simulação: {e}"));
        }
        self.atalhos(ui.ctx());

        self.faixa_arquivo(ui, s, &modelos, &pessoa);
        if !self.aberto() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Nenhum modelo aberto")
                        .size(18.0)
                        .strong(),
                );
                ui.weak("Escolha o .ron do simulador acima e clique em Abrir.");
            });
            return;
        }
        self.contar_estudos(s);
        ui.add_space(8.0);
        self.zonas(ui);
    }

    /// O caminho do modelo como os estudos o guardam: relativo à pasta da unidade quando
    /// o modelo está dentro dela, senão absoluto.
    pub fn modelo_para_estudo(&self, s: &Sessao) -> String {
        let Some(a) = &self.arquivo else {
            return String::new();
        };
        if let Some(u) = s.unidade.as_ref()
            && let Ok(r) = a.strip_prefix(u.raiz())
        {
            return r.to_string_lossy().replace('\\', "/");
        }
        a.display().to_string()
    }

    fn contar_estudos(&mut self, s: &Sessao) {
        self.modelo_rel = self.modelo_para_estudo(s);
        if self.selecionado.is_empty() {
            return;
        }
        let atual = self
            .estudos_no
            .as_ref()
            .is_some_and(|(v, o, _)| *v == s.versao && *o == self.selecionado);
        if !atual {
            let n = crate::ferramentas::estudos_do_no(s, &self.modelo_rel, &self.selecionado).len();
            self.estudos_no = Some((s.versao, self.selecionado.clone(), n));
        }
    }

    /// Quantos estudos o nó escolhido tem (contados no último quadro).
    pub fn estudos_do_selecionado(&self) -> usize {
        self.estudos_no
            .as_ref()
            .filter(|(_, o, _)| *o == self.selecionado)
            .map_or(0, |(_, _, n)| *n)
    }

    fn atalhos(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        if !self.aberto() {
            return;
        }
        let (s, z, y) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::S),
                i.consume_key(Modifiers::COMMAND, Key::Z),
                i.consume_key(Modifiers::COMMAND, Key::Y),
            )
        });
        if s {
            self.gravar_com_aviso();
        }
        if !ctx.egui_wants_keyboard_input() {
            if z {
                self.desfazer();
            }
            if y {
                self.refazer();
            }
        }
    }

    fn gravar_com_aviso(&mut self) {
        if let Err(e) = self.gravar() {
            self.falhar(e);
        }
    }

    /// A faixa fina do arquivo: o modelo, "Procurar…", Abrir e Fechar à esquerda; a trava,
    /// Desfazer, Refazer e Gravar à direita. Embaixo, o aviso do momento.
    fn faixa_arquivo(&mut self, ui: &mut egui::Ui, s: &Sessao, modelos: &[String], pessoa: &str) {
        let v = ui.visuals().clone();
        egui::Frame::new()
            .fill(v.faint_bg_color)
            .stroke(v.widgets.noninteractive.bg_stroke)
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("MODELO")
                            .size(11.0)
                            .strong()
                            .color(v.weak_text_color()),
                    );
                    let largura = (ui.available_width() - 640.0).clamp(200.0, 420.0);
                    crate::util::campo_modelo(ui, &mut self.caminho, s.unidade.as_ref(), largura);
                    if !modelos.is_empty() {
                        egui::ComboBox::from_id_salt("modelos_da_aba_modelo")
                            .selected_text("dos postos")
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                for m in modelos {
                                    ui.selectable_value(&mut self.caminho, m.clone(), m);
                                }
                            });
                    }
                    if ui
                        .add_enabled(!self.caminho.trim().is_empty(), egui::Button::new("Abrir"))
                        .clicked()
                    {
                        let c = match s.unidade.as_ref() {
                            Some(u) => u.caminho_modelo(self.caminho.trim()),
                            None => PathBuf::from(self.caminho.trim()),
                        };
                        if let Err(e) = self.abrir(&c, pessoa) {
                            self.falhar(e);
                        }
                    }
                    if self.aberto() && ui.button("Fechar").clicked() {
                        self.fechar();
                    }
                    if self.aberto() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.controles_edicao(ui);
                        });
                    }
                });
            });
        let laranja = egui::Color32::from_rgb(230, 160, 40);
        if self.mudou_no_disco {
            ui.horizontal(|ui| {
                ui.colored_label(
                    laranja,
                    "O arquivo mudou no disco e há mudança sua sem gravar.",
                );
                if ui.button("Recarregar (perde a mudança)").clicked() {
                    self.recarregar();
                }
                if ui.button("Manter a minha").clicked() {
                    self.mtime = self.arquivo.as_deref().and_then(modificado);
                    self.mudou_no_disco = false;
                }
            });
        }
        if !self.mensagem.is_empty() {
            let cor = if self.erro {
                egui::Color32::from_rgb(230, 90, 80)
            } else {
                v.weak_text_color()
            };
            ui.add_space(2.0);
            ui.colored_label(cor, &self.mensagem);
        }
    }

    /// Da direita para a esquerda: Gravar, Refazer, Desfazer, o estado da trava e o aviso
    /// de mudança sem gravar.
    fn controles_edicao(&mut self, ui: &mut egui::Ui) {
        let laranja = egui::Color32::from_rgb(230, 160, 40);
        let verde = egui::Color32::from_rgb(70, 180, 110);
        let editavel = self.pode_editar();
        let (desf, refaz) = self
            .doc
            .as_ref()
            .map_or((false, false), |d| (d.can_undo(), d.can_redo()));
        let gravar = egui::Button::new(egui::RichText::new("Gravar").strong()).fill(
            if editavel && self.pendente() {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().widgets.inactive.weak_bg_fill
            },
        );
        if ui
            .add_enabled(editavel, gravar)
            .on_hover_text("Grava no modelo (Ctrl+S)")
            .clicked()
        {
            self.gravar_com_aviso();
        }
        if ui
            .add_enabled(editavel && refaz, egui::Button::new("Refazer"))
            .on_hover_text("Ctrl+Y")
            .clicked()
        {
            self.refazer();
        }
        if ui
            .add_enabled(editavel && desf, egui::Button::new("Desfazer"))
            .on_hover_text("Ctrl+Z")
            .clicked()
        {
            self.desfazer();
        }
        ui.add_space(6.0);
        if editavel {
            let r = selo(ui, "EDITANDO", verde)
                .on_hover_text("Você tem a trava do arquivo. Clique para soltar e ficar só lendo.");
            if r.clicked() {
                self.soltar_edicao();
            }
        } else {
            let dica = self
                .aviso_trava()
                .unwrap_or_else(|| "Ninguém edita. Clique para editar.".to_string());
            if selo(ui, "SÓ LEITURA", laranja)
                .on_hover_text(format!("{dica} Clique para tentar editar."))
                .clicked()
            {
                self.tentar_editar();
            }
        }
        if self.pendente() {
            ui.colored_label(laranja, "mudança sem gravar");
        }
        if ui
            .small_button("⟳")
            .on_hover_text("Recarregar do disco")
            .clicked()
        {
            if self.pendente() {
                self.mudou_no_disco = true;
            } else {
                self.recarregar();
            }
        }
    }

    /// As três zonas: os nós à esquerda, o nó escolhido no centro e os resultados à
    /// direita. Cada uma rola sozinha.
    fn zonas(&mut self, ui: &mut egui::Ui) {
        let total = ui.available_rect_before_wrap();
        let estreita = total.width() < 1000.0;
        let (le, ld) = if estreita {
            (220.0, 240.0)
        } else {
            (270.0, 300.0)
        };
        let gap = 12.0;
        let esq = egui::Rect::from_min_size(total.min, egui::vec2(le, total.height()));
        let dir = egui::Rect::from_min_size(
            egui::pos2(total.right() - ld, total.top()),
            egui::vec2(ld, total.height()),
        );
        let centro = egui::Rect::from_min_max(
            egui::pos2(esq.right() + gap, total.top()),
            egui::pos2(dir.left() - gap, total.bottom()),
        );
        for (r, qual) in [(esq, 0), (centro, 1), (dir, 2)] {
            zona_fundo(ui, r);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(r.shrink(12.0))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
                |ui| match qual {
                    0 => self.lista(ui),
                    1 => self.painel_no(ui),
                    _ => self.painel_resumo(ui),
                },
            );
        }
        ui.allocate_rect(total, egui::Sense::hover());
    }

    fn lista(&mut self, ui: &mut egui::Ui) {
        let nomes_todos = self.doc.as_ref().map_or(0, |d| d.model().objects.len());
        titulo_zona(ui, &format!("Nós do modelo · {nomes_todos}"));
        ui.add(
            egui::TextEdit::singleline(&mut self.filtro)
                .hint_text("Filtrar por nome")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        let nomes = self.objetos();
        let mut escolhido: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("lista_objetos_modelo")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for n in &nomes {
                    let tags = self.etiquetas(n);
                    let destaque = self.destaque(n);
                    if linha_no(ui, n, &tags, destaque.as_deref(), self.selecionado == *n) {
                        escolhido = Some(n.clone());
                    }
                }
                if nomes.is_empty() {
                    ui.weak("Nenhum nó com esse nome.");
                }
            });
        if let Some(n) = escolhido {
            self.selecionar(&n);
        }
    }

    /// O centro: o nó escolhido, as abas (a do app e a Manutenção) e cada bloco de campos
    /// num quadro com título.
    fn painel_no(&mut self, ui: &mut egui::Ui) {
        if self.selecionado.is_empty() {
            titulo_zona(ui, "Nó");
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.weak("Escolha um nó na lista à esquerda.");
            });
            return;
        }
        let tipo = self
            .doc
            .as_ref()
            .and_then(|d| {
                d.model()
                    .objects
                    .iter()
                    .find(|o| o.name == self.selecionado)
            })
            .map(|o| tipo_do_no(&format!("{:?}", o.kind)));
        titulo_zona(ui, tipo.unwrap_or("Nó"));
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&self.selecionado)
                    .size(22.0)
                    .strong()
                    .color(ui.visuals().strong_text_color()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Abrir em Ferramentas").clicked() {
                    self.pedido_ferramentas =
                        Some((self.modelo_rel.clone(), self.selecionado.clone()));
                }
                let n = self.estudos_do_selecionado();
                ui.weak(match n {
                    0 => "nenhum estudo".to_string(),
                    1 => "1 estudo".to_string(),
                    n => format!("{n} estudos"),
                });
            });
        });
        ui.add_space(8.0);
        let abas = [self.principal, Aba::Manutencao];
        if !abas.contains(&self.aba) {
            self.aba = self.principal;
        }
        if let Some(a) = segmentos(
            ui,
            &abas.map(|a| a.nome()),
            abas.iter().position(|a| *a == self.aba).unwrap_or(0),
        ) {
            self.aba = abas[a];
        }
        ui.add_space(8.0);
        let aba = self.aba;
        let editavel = self.pode_editar() && aba == self.principal;
        if aba == Aba::Manutencao {
            ui.weak("Manutenção: só leitura aqui. Edite no UTSim.");
            ui.add_space(4.0);
        }
        let campos = self.campos(aba);
        if campos.is_empty() {
            ui.weak("Nada para mostrar neste nó.");
        }
        // Cada campo de profundidade 0 abre um quadro; os de dentro vão nele.
        let mut grupos: Vec<Vec<&Campo>> = Vec::new();
        for c in &campos {
            if c.profundidade == 0 || grupos.is_empty() {
                grupos.push(Vec::new());
            }
            if let Some(g) = grupos.last_mut() {
                g.push(c);
            }
        }
        let mut acoes: Vec<Acao> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("campos_modelo")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for g in &grupos {
                    let v = ui.visuals().clone();
                    egui::Frame::new()
                        .fill(v.extreme_bg_color)
                        .stroke(v.widgets.noninteractive.bg_stroke)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let mut ocultar: Option<u8> = None;
                            for (k, c) in g.iter().enumerate() {
                                if let Some(p) = ocultar {
                                    if c.profundidade > p {
                                        continue;
                                    }
                                    ocultar = None;
                                }
                                if c.tipo == Tipo::Item {
                                    let chave = format!("{}#{}", self.selecionado, c.caminho);
                                    if self.fechados.contains(&chave) {
                                        ocultar = Some(c.profundidade);
                                    }
                                }
                                self.campo_ui(ui, c, editavel, &mut acoes);
                                if k == 0 && g.len() > 1 {
                                    ui.add_space(4.0);
                                    ui.separator();
                                }
                            }
                        });
                    ui.add_space(10.0);
                }
            });
        for a in acoes {
            let r = match a {
                Acao::Editar(c, v) => self.editar(&c, &v),
                Acao::Adicionar(l) => self.adicionar(&l),
                Acao::Remover(l, i) => self.remover(&l, i),
            };
            if r.is_ok() {
                self.mensagem.clear();
            }
        }
    }

    /// A direita: os resultados simulados do nó e do modelo inteiro em blocos de números,
    /// com a origem "Simulado" bem marcada.
    fn painel_resumo(&mut self, ui: &mut egui::Ui) {
        let laranja = egui::Color32::from_rgb(230, 160, 40);
        ui.horizontal(|ui| {
            titulo_zona(ui, "Resultados");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                selo(ui, "SIMULADO", laranja)
                    .on_hover_text("Resultado do modelo, não é medição da fábrica.");
            });
        });
        ui.weak("Do modelo, não é medição da fábrica.");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.trabalho.ocupado(),
                    egui::Button::new("Simular de novo"),
                )
                .clicked()
            {
                self.calcular_resumo();
            }
            if self.trabalho.ocupado() {
                ui.spinner();
            }
        });
        ui.add_space(10.0);
        let Some(r) = self.trabalho.ok() else {
            if let Some(e) = self.trabalho.erro() {
                ui.colored_label(egui::Color32::from_rgb(230, 90, 80), e);
            }
            return;
        };
        let r = r.clone();
        egui::ScrollArea::vertical()
            .id_salt("resumo_modelo")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !self.selecionado.is_empty() {
                    subtitulo(ui, &format!("Nó {}", self.selecionado));
                    match r.get(&self.selecionado) {
                        Some(l) => blocos(ui, l),
                        None => {
                            ui.weak("Sem resultado simulado para este nó.");
                        }
                    }
                    ui.add_space(12.0);
                }
                if let Some(l) = r.get("") {
                    subtitulo(ui, "Modelo inteiro");
                    blocos(ui, l);
                }
            });
    }

    /// O que o nó tem: qualidade, inspeção, retrabalho, EPI, perigos, quebras.
    pub fn etiquetas(&self, objeto: &str) -> Vec<&'static str> {
        let Some(o) = self
            .doc
            .as_ref()
            .and_then(|d| d.model().objects.iter().find(|o| o.name == objeto))
        else {
            return Vec::new();
        };
        let mut v = Vec::new();
        if o.quality.is_some() {
            v.push("qualidade");
        }
        if o.inspection.is_some() {
            v.push("inspeção");
        }
        if o.rework.is_some() {
            v.push("retrabalho");
        }
        if !o.ppe.is_empty() {
            v.push("EPI");
        }
        if o.safety.as_ref().is_some_and(|s| !s.is_empty()) {
            v.push("perigos");
        }
        if o.downtime.is_some() {
            v.push("quebras");
        }
        v
    }

    /// Número-chave do resultado simulado do nó (linha [`DESTAQUE`] do resumo).
    pub fn destaque(&self, objeto: &str) -> Option<String> {
        self.trabalho
            .ok()?
            .get(objeto)?
            .iter()
            .find(|(k, _)| k == DESTAQUE)
            .map(|(_, v)| v.clone())
    }

    fn campo_ui(&mut self, ui: &mut egui::Ui, c: &Campo, editavel: bool, acoes: &mut Vec<Acao>) {
        let recuo = f32::from(c.profundidade.saturating_sub(1)) * 16.0;
        let vermelho = egui::Color32::from_rgb(230, 90, 80);
        let mut nova_acao: Option<Acao> = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_space(recuo);
            match c.tipo {
                Tipo::Info => {
                    ui.label(format!("{}: {}", c.rotulo, c.valor));
                }
                Tipo::Lista => {
                    ui.strong(&c.rotulo);
                    if ui
                        .add_enabled(editavel, egui::Button::new("Adicionar"))
                        .clicked()
                    {
                        nova_acao = Some(Acao::Adicionar(c.caminho.clone()));
                    }
                }
                Tipo::Item => {
                    let chave = format!("{}#{}", self.selecionado, c.caminho);
                    let fechado = self.fechados.contains(&chave);
                    if ui
                        .small_button(if fechado { "+" } else { "−" })
                        .on_hover_text(if fechado { "Mostrar" } else { "Esconder" })
                        .clicked()
                    {
                        if fechado {
                            self.fechados.remove(&chave);
                        } else {
                            self.fechados.insert(chave);
                        }
                    }
                    ui.strong(&c.rotulo);
                    if ui
                        .add_enabled(editavel, egui::Button::new("Remover"))
                        .clicked()
                        && let Some((lista, i)) = separar_item(&c.caminho)
                    {
                        nova_acao = Some(Acao::Remover(lista, i));
                    }
                }
                Tipo::Bloco | Tipo::Booleano => {
                    let mut v = c.valor == "sim";
                    if ui
                        .add_enabled(editavel, egui::Checkbox::new(&mut v, &c.rotulo))
                        .changed()
                    {
                        nova_acao = Some(Acao::Editar(
                            c.caminho.clone(),
                            if v { "sim" } else { "não" }.to_string(),
                        ));
                    }
                }
                Tipo::Escolha => {
                    ui.label(&c.rotulo);
                    let mut atual = c.valor.clone();
                    egui::ComboBox::from_id_salt(format!("esc_{}_{}", self.selecionado, c.caminho))
                        .selected_text(atual.clone())
                        .show_ui(ui, |ui| {
                            ui.add_enabled_ui(editavel, |ui| {
                                for e in &c.escolhas {
                                    ui.selectable_value(&mut atual, e.clone(), e);
                                }
                            });
                        });
                    if atual != c.valor {
                        nova_acao = Some(Acao::Editar(c.caminho.clone(), atual));
                    }
                }
                Tipo::Conjunto => {
                    ui.label(&c.rotulo);
                    let mut itens: Vec<String> = c
                        .valor
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect();
                    let antes = itens.clone();
                    egui::ComboBox::from_id_salt(format!(
                        "conj_{}_{}",
                        self.selecionado, c.caminho
                    ))
                    .selected_text(if c.valor.is_empty() {
                        "nenhum"
                    } else {
                        c.valor.as_str()
                    })
                    .show_ui(ui, |ui| {
                        ui.add_enabled_ui(editavel, |ui| {
                            for e in &c.escolhas {
                                let mut marcado = itens.contains(e);
                                if ui.checkbox(&mut marcado, e).changed() {
                                    if marcado {
                                        itens.push(e.clone());
                                    } else {
                                        itens.retain(|x| x != e);
                                    }
                                }
                            }
                        });
                    });
                    let chave = format!("{}#livre", c.caminho);
                    let mut livre = self.rascunhos.get(&chave).cloned().unwrap_or_default();
                    let r = ui.add_enabled(
                        editavel,
                        egui::TextEdit::singleline(&mut livre)
                            .hint_text("outro")
                            .desired_width(80.0),
                    );
                    if r.lost_focus() && !livre.trim().is_empty() {
                        let novo = livre.trim().to_string();
                        if !itens.contains(&novo) {
                            itens.push(novo);
                        }
                        livre.clear();
                    }
                    if livre.is_empty() {
                        self.rascunhos.remove(&chave);
                    } else {
                        self.rascunhos.insert(chave, livre);
                    }
                    if itens != antes {
                        nova_acao = Some(Acao::Editar(c.caminho.clone(), itens.join(", ")));
                    }
                }
                Tipo::Inteiro
                | Tipo::Real
                | Tipo::RealOpcional
                | Tipo::InteiroOpcional
                | Tipo::Texto
                | Tipo::Distribuicao => {
                    ui.label(&c.rotulo);
                    let mut t = self
                        .rascunhos
                        .get(&c.caminho)
                        .cloned()
                        .unwrap_or_else(|| c.valor.clone());
                    let r = ui.add_enabled(
                        editavel,
                        egui::TextEdit::singleline(&mut t).desired_width(160.0),
                    );
                    if r.changed() {
                        self.rascunhos.insert(c.caminho.clone(), t.clone());
                    }
                    if r.lost_focus() && t != c.valor {
                        nova_acao = Some(Acao::Editar(c.caminho.clone(), t));
                    } else if r.lost_focus() {
                        self.rascunhos.remove(&c.caminho);
                    }
                    if !c.unidade.is_empty() {
                        ui.weak(&c.unidade);
                    }
                }
            }
            if !c.ajuda.is_empty() && c.tipo != Tipo::Info {
                ui.weak("?").on_hover_text(&c.ajuda);
            }
            if let Some(e) = self.erros.get(&c.caminho) {
                ui.colored_label(vermelho, e);
            }
        });
        if let Some(a) = nova_acao {
            acoes.push(a);
        }
    }
}

/// O tipo do nó em palavras (do nome da variante do modelo).
fn tipo_do_no(depurado: &str) -> &'static str {
    let nome = depurado
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("");
    match nome {
        "Source" => "Chegada",
        "Processor" => "Processo",
        "Queue" => "Fila",
        "Sink" => "Saída",
        "Conveyor" => "Transportador",
        "Combiner" => "Montagem",
        "Separator" => "Separação",
        "Transporter" => "Transporte",
        "Rack" => "Estante",
        "Dispatcher" => "Despacho",
        _ => "Nó",
    }
}

/// O fundo de uma zona: um quadro arredondado do tamanho dela.
fn zona_fundo(ui: &egui::Ui, r: egui::Rect) {
    let v = ui.visuals();
    ui.painter().rect(
        r,
        egui::CornerRadius::same(10),
        v.faint_bg_color,
        v.widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
}

/// O título pequeno, em maiúsculas, no alto de cada zona.
fn titulo_zona(ui: &mut egui::Ui, t: &str) {
    ui.label(
        egui::RichText::new(t.to_uppercase())
            .size(11.0)
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(4.0);
}

fn subtitulo(ui: &mut egui::Ui, t: &str) {
    ui.label(
        egui::RichText::new(t)
            .size(13.0)
            .strong()
            .color(ui.visuals().strong_text_color()),
    );
    ui.add_space(4.0);
}

/// Um selo: texto curto em maiúsculas num contorno da cor. Clicável.
fn selo(ui: &mut egui::Ui, t: &str, cor: egui::Color32) -> egui::Response {
    let g = ui
        .painter()
        .layout_no_wrap(t.to_string(), egui::FontId::proportional(10.5), cor);
    let tam = g.size() + egui::vec2(14.0, 6.0);
    let (r, resp) = ui.allocate_exact_size(tam, egui::Sense::click());
    let p = ui.painter();
    p.rect(
        r,
        egui::CornerRadius::same(5),
        cor.gamma_multiply(if resp.hovered() { 0.25 } else { 0.12 }),
        egui::Stroke::new(1.0, cor.gamma_multiply(0.7)),
        egui::StrokeKind::Inside,
    );
    p.galley(r.center() - g.size() / 2.0, g, cor);
    resp
}

/// A cor do selo de cada etiqueta de nó.
fn cor_etiqueta(t: &str) -> egui::Color32 {
    match t {
        "qualidade" => egui::Color32::from_rgb(0x5B, 0x8D, 0xEF),
        "inspeção" => egui::Color32::from_rgb(0x2E, 0xB8, 0xA6),
        "retrabalho" => egui::Color32::from_rgb(0xA7, 0x7B, 0xE8),
        "EPI" => egui::Color32::from_rgb(0xF2, 0x8C, 0x1E),
        "perigos" => egui::Color32::from_rgb(0xE5, 0x5A, 0x50),
        _ => egui::Color32::from_rgb(0x9A, 0xA3, 0xAD),
    }
}

/// Uma linha da lista de nós: o nome, o número em destaque à direita e os selos embaixo.
/// Devolve `true` no clique.
fn linha_no(
    ui: &mut egui::Ui,
    nome: &str,
    tags: &[&str],
    destaque: Option<&str>,
    escolhido: bool,
) -> bool {
    let alto = if tags.is_empty() && destaque.is_none() {
        34.0
    } else {
        52.0
    };
    let (r, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), alto), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let resp = match destaque {
        Some(d) => resp.on_hover_text(d),
        None => resp,
    };
    if !ui.is_rect_visible(r) {
        return resp.clicked();
    }
    let v = ui.visuals().clone();
    let p = ui.painter();
    if escolhido {
        p.rect_filled(
            r,
            egui::CornerRadius::same(6),
            v.selection.bg_fill.gamma_multiply(0.35),
        );
        p.rect_filled(
            egui::Rect::from_min_size(r.min + egui::vec2(0.0, 6.0), egui::vec2(3.0, alto - 12.0)),
            egui::CornerRadius::same(2),
            v.selection.bg_fill,
        );
    } else if resp.hovered() {
        p.rect_filled(
            r,
            egui::CornerRadius::same(6),
            v.widgets.hovered.weak_bg_fill,
        );
    }
    let x = r.left() + 12.0;
    let y = r.top() + 9.0;
    let mut job = egui::text::LayoutJob::simple_singleline(
        nome.to_string(),
        egui::FontId::proportional(14.0),
        if escolhido {
            v.strong_text_color()
        } else {
            v.text_color()
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((r.width() - 24.0).max(20.0));
    let g = p.layout_job(job);
    p.galley(egui::pos2(x, y), g, v.text_color());
    // Na segunda linha: os selos e, no que sobra à direita, o número em destaque.
    let mut tx = x;
    for t in tags {
        let cor = cor_etiqueta(t);
        let g = p.layout_no_wrap(t.to_string(), egui::FontId::proportional(10.0), cor);
        let caixa =
            egui::Rect::from_min_size(egui::pos2(tx, y + 21.0), g.size() + egui::vec2(10.0, 4.0));
        if caixa.right() > r.right() - 8.0 {
            break;
        }
        p.rect_filled(caixa, egui::CornerRadius::same(4), cor.gamma_multiply(0.16));
        p.galley(caixa.min + egui::vec2(5.0, 2.0), g, cor);
        tx = caixa.right() + 4.0;
    }
    if let Some(d) = destaque {
        let livre = r.right() - 10.0 - (tx + 6.0);
        if livre > 30.0 {
            let mut job = egui::text::LayoutJob::simple_singleline(
                d.to_string(),
                egui::FontId::monospace(11.0),
                v.weak_text_color(),
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(livre);
            let g = p.layout_job(job);
            p.galley(
                egui::pos2(r.right() - 10.0 - g.size().x, y + 22.0),
                g,
                v.weak_text_color(),
            );
        }
    }
    resp.clicked()
}

/// Abas segmentadas: botões colados num contorno. Devolve o índice clicado.
fn segmentos(ui: &mut egui::Ui, nomes: &[&str], atual: usize) -> Option<usize> {
    let v = ui.visuals().clone();
    let alto = 30.0;
    let larguras: Vec<f32> = nomes
        .iter()
        .map(|n| {
            ui.painter()
                .layout_no_wrap(
                    n.to_string(),
                    egui::FontId::proportional(13.5),
                    v.text_color(),
                )
                .size()
                .x
                + 32.0
        })
        .collect();
    let total: f32 = larguras.iter().sum::<f32>() + 4.0;
    let (r, _) = ui.allocate_exact_size(egui::vec2(total, alto), egui::Sense::hover());
    ui.painter().rect(
        r,
        egui::CornerRadius::same(8),
        v.extreme_bg_color,
        v.widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let mut x = r.left() + 2.0;
    let mut clicado = None;
    for (k, (n, w)) in nomes.iter().zip(larguras).enumerate() {
        let c = egui::Rect::from_min_size(egui::pos2(x, r.top() + 2.0), egui::vec2(w, alto - 4.0));
        let resp = ui
            .interact(c, ui.id().with(("segmento", k)), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let sel = k == atual;
        if sel {
            ui.painter()
                .rect_filled(c, egui::CornerRadius::same(6), v.selection.bg_fill);
        } else if resp.hovered() {
            ui.painter().rect_filled(
                c,
                egui::CornerRadius::same(6),
                v.widgets.hovered.weak_bg_fill,
            );
        }
        ui.painter().text(
            c.center(),
            egui::Align2::CENTER_CENTER,
            *n,
            egui::FontId::proportional(13.5),
            if sel {
                egui::Color32::WHITE
            } else {
                v.text_color()
            },
        );
        if resp.clicked() && !sel {
            clicado = Some(k);
        }
        x += w;
    }
    clicado
}

/// Os números do resumo em blocos, dois por linha: o rótulo pequeno em cima e o valor
/// grande embaixo. "(%)" no rótulo vira " %" no valor; a origem e o destaque ficam de
/// fora (a origem já está no selo).
fn blocos(ui: &mut egui::Ui, l: &[(String, String)]) {
    let itens: Vec<(String, String)> = l
        .iter()
        .filter(|(k, _)| k != DESTAQUE && k != "Origem")
        .map(|(k, v)| match k.strip_suffix(" (%)") {
            Some(base) => (base.to_string(), format!("{v} %")),
            None => (k.clone(), v.clone()),
        })
        .collect();
    let vis = ui.visuals().clone();
    let gap = 8.0;
    let w = ((ui.available_width() - gap) / 2.0).floor();
    let alto = 70.0;
    for par in itens.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (k, v) in par {
                let (r, _) = ui.allocate_exact_size(egui::vec2(w, alto), egui::Sense::hover());
                let p = ui.painter();
                p.rect(
                    r,
                    egui::CornerRadius::same(8),
                    vis.extreme_bg_color,
                    vis.widgets.noninteractive.bg_stroke,
                    egui::StrokeKind::Inside,
                );
                let rot = p.layout(
                    k.clone(),
                    egui::FontId::proportional(11.0),
                    vis.weak_text_color(),
                    w - 20.0,
                );
                p.galley(r.min + egui::vec2(10.0, 8.0), rot, vis.weak_text_color());
                // O valor grande; se não cabe, menor e em até duas linhas.
                let grande = p.layout_no_wrap(
                    v.clone(),
                    egui::FontId::proportional(20.0),
                    vis.strong_text_color(),
                );
                let g = if grande.size().x <= w - 20.0 {
                    grande
                } else {
                    let mut job = egui::text::LayoutJob::simple(
                        v.clone(),
                        egui::FontId::proportional(13.0),
                        vis.strong_text_color(),
                        w - 20.0,
                    );
                    job.wrap.max_rows = 2;
                    p.layout_job(job)
                };
                p.galley(
                    egui::pos2(r.left() + 10.0, r.bottom() - 8.0 - g.size().y),
                    g,
                    vis.strong_text_color(),
                );
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
}

impl Drop for TelaModelo {
    fn drop(&mut self) {
        self.soltar_trava();
    }
}

/// `safety.hazards[2]` vira (`safety.hazards`, 2).
fn separar_item(caminho: &str) -> Option<(String, usize)> {
    let i = caminho.rfind('[')?;
    let n = caminho[i + 1..].strip_suffix(']')?.parse().ok()?;
    Some((caminho[..i].to_string(), n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item() {
        assert_eq!(
            separar_item("safety.hazards[2]"),
            Some(("safety.hazards".to_string(), 2))
        );
        assert_eq!(separar_item("quality"), None);
    }

    #[test]
    fn horas() {
        assert_eq!(hora(3_600_000 * 13 + 60_000 * 5), "13:05");
    }
}
