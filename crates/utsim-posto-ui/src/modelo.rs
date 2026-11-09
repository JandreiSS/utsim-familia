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
        let Some(u) = s.unidade.as_ref() else {
            ui.label("Abra uma unidade na tela inicial.");
            return;
        };
        let pessoa = u.autor().to_string();
        let modelos: Vec<String> = {
            let mut v: Vec<String> = u.postos().into_iter().filter_map(|p| p.modelo).collect();
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

        ui.heading("Modelo (mão dupla com o UTSim)");
        ui.horizontal(|ui| {
            ui.label("Modelo (.ron):");
            ui.add(egui::TextEdit::singleline(&mut self.caminho).desired_width(420.0));
            if !modelos.is_empty() {
                egui::ComboBox::from_id_salt("modelos_da_aba_modelo")
                    .selected_text("modelos dos postos")
                    .show_ui(ui, |ui| {
                        for m in &modelos {
                            ui.selectable_value(&mut self.caminho, m.clone(), m);
                        }
                    });
            }
            if ui
                .add_enabled(!self.caminho.trim().is_empty(), egui::Button::new("Abrir"))
                .clicked()
            {
                let c = s
                    .unidade
                    .as_ref()
                    .map(|u| u.caminho_modelo(self.caminho.trim()));
                if let Some(c) = c
                    && let Err(e) = self.abrir(&c, &pessoa)
                {
                    self.falhar(e);
                }
            }
            if self.aberto() && ui.button("Fechar modelo").clicked() {
                self.fechar();
            }
        });
        if !self.mensagem.is_empty() {
            let cor = if self.erro {
                egui::Color32::from_rgb(230, 90, 80)
            } else {
                ui.visuals().text_color()
            };
            ui.colored_label(cor, &self.mensagem);
        }
        if !self.aberto() {
            return;
        }
        self.barra_estado(ui);
        ui.separator();
        ui.columns(2, |cols| {
            self.lista(&mut cols[0]);
            self.painel_campos(&mut cols[1]);
        });
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

    fn barra_estado(&mut self, ui: &mut egui::Ui) {
        let laranja = egui::Color32::from_rgb(230, 160, 40);
        if !self.pode_editar() {
            ui.horizontal(|ui| {
                let texto = match self.aviso_trava() {
                    Some(av) => format!("{av}. Só leitura."),
                    None => "Só leitura.".to_string(),
                };
                ui.colored_label(laranja, texto);
                if ui.button("Tentar editar").clicked() {
                    self.tentar_editar();
                }
            });
        }
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
        let editavel = self.pode_editar();
        ui.horizontal(|ui| {
            let (desf, refaz) = self
                .doc
                .as_ref()
                .map_or((false, false), |d| (d.can_undo(), d.can_redo()));
            if ui
                .add_enabled(editavel && desf, egui::Button::new("Desfazer"))
                .clicked()
            {
                self.desfazer();
            }
            if ui
                .add_enabled(editavel && refaz, egui::Button::new("Refazer"))
                .clicked()
            {
                self.refazer();
            }
            if ui
                .add_enabled(editavel, egui::Button::new("Gravar no modelo"))
                .clicked()
            {
                self.gravar_com_aviso();
            }
            if editavel && ui.button("Soltar edição").clicked() {
                self.soltar_edicao();
            }
            if ui.button("Recarregar do disco").clicked() {
                if self.pendente() {
                    self.mudou_no_disco = true;
                } else {
                    self.recarregar();
                }
            }
            if self.pendente() {
                ui.colored_label(laranja, "mudança sem gravar");
            }
        });
    }

    fn lista(&mut self, ui: &mut egui::Ui) {
        ui.strong("Objetos do modelo");
        ui.add(
            egui::TextEdit::singleline(&mut self.filtro)
                .hint_text("filtrar por nome")
                .desired_width(f32::INFINITY),
        );
        let nomes = self.objetos();
        let mut escolhido: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("lista_objetos_modelo")
            .max_height(420.0)
            .show(ui, |ui| {
                for n in &nomes {
                    let tags = self.etiquetas(n);
                    let destaque = self.destaque(n);
                    let mut texto = n.clone();
                    if !tags.is_empty() {
                        texto.push_str(&format!("  [{}]", tags.join(", ")));
                    }
                    if let Some(d) = destaque {
                        texto.push_str(&format!("  {d}"));
                    }
                    if ui.selectable_label(self.selecionado == *n, texto).clicked() {
                        escolhido = Some(n.clone());
                    }
                }
            });
        if let Some(n) = escolhido {
            self.selecionar(&n);
        }
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

    /// Painel do nó: primeiro os resultados simulados, depois os campos editáveis.
    fn painel_campos(&mut self, ui: &mut egui::Ui) {
        self.painel_resumo(ui);
        ui.separator();
        if self.selecionado.is_empty() {
            ui.label("Escolha um objeto na lista.");
            return;
        }
        ui.strong(format!("Nó: {}", self.selecionado));
        let abas = [self.principal, Aba::Manutencao];
        ui.horizontal(|ui| {
            for a in abas {
                ui.selectable_value(&mut self.aba, a, a.nome());
            }
        });
        if !abas.contains(&self.aba) {
            self.aba = self.principal;
        }
        let aba = self.aba;
        let editavel = self.pode_editar() && aba == self.principal;
        if aba == Aba::Manutencao {
            ui.weak("Manutenção: só leitura aqui. Edite no UTSim.");
        }
        let campos = self.campos(aba);
        if campos.is_empty() {
            ui.weak("Nada para mostrar neste nó.");
        }
        let mut acoes: Vec<Acao> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("campos_modelo")
            .show(ui, |ui| {
                let mut ocultar: Option<u8> = None;
                for c in &campos {
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

    fn campo_ui(&mut self, ui: &mut egui::Ui, c: &Campo, editavel: bool, acoes: &mut Vec<Acao>) {
        let recuo = f32::from(c.profundidade) * 16.0;
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
                    if ui.small_button(if fechado { "▸" } else { "▾" }).clicked() {
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

    fn painel_resumo(&mut self, ui: &mut egui::Ui) {
        ui.strong("Resultados da simulação");
        ui.colored_label(
            egui::Color32::from_rgb(230, 160, 40),
            "Origem: Simulado (resultado do modelo, não é medição da fábrica)",
        );
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
        let Some(r) = self.trabalho.ok() else {
            if let Some(e) = self.trabalho.erro() {
                ui.colored_label(egui::Color32::from_rgb(230, 90, 80), e);
            }
            return;
        };
        if !self.selecionado.is_empty() {
            ui.label(format!("Nó {} (Simulado)", self.selecionado));
            match r.get(&self.selecionado) {
                Some(l) => linhas(ui, "resumo_no", l),
                None => {
                    ui.weak("Sem resultado simulado para este nó.");
                }
            }
        }
        if let Some(l) = r.get("") {
            ui.label("Modelo inteiro (Simulado)");
            linhas(ui, "resumo_geral", l);
        }
    }
}

fn linhas(ui: &mut egui::Ui, id: &str, l: &[(String, String)]) {
    egui::Grid::new(id).striped(true).show(ui, |ui| {
        for (k, v) in l {
            ui.label(k);
            ui.label(v);
            ui.end_row();
        }
    });
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
