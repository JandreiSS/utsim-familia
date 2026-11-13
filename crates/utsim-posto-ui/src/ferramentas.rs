//! Tela "Ferramentas": os estudos da unidade (seção 5.1 de
//! `docs/plano_integracao_familia.md`), comum aos apps Qualidade e Segurança do Trabalho.
//!
//! - à esquerda, a lista dos estudos de todos os apps (os do app primeiro), o filtro e o
//!   formulário de estudo novo.
//! - à direita, o estudo: título, problema, assunto, conclusão, as ferramentas aplicadas e,
//!   da ferramenta escolhida, os dados (tabela, parâmetros, importação de CSV ou xlsx, simulação) e
//!   o resultado calculado por `utsim_estudo::calculo`, o mesmo que vai ao relatório.
//! - cada mudança grava o estudo inteiro na pasta de unidade, no máximo uma vez por
//!   segundo enquanto a pessoa digita e na hora em que o campo perde o foco.

use std::path::PathBuf;
use std::sync::Arc;

use egui::{Align, Color32, Layout, RichText, Stroke, vec2};
use egui_plot::{
    AxisHints, Bar, BarChart, HLine, HPlacement, Line, LineStyle, Plot, PlotPoints, Points, VLine,
};
use utsim_estudo::calculo::{self, Resultado};
use utsim_estudo::relatorio::{self, Contexto, Identidade};
use utsim_estudo::{
    Aplicacao, App, Assunto, Cadeia, Categoria, ESTUDOS, Estudo, Ferramenta, Origem, Parametros,
    Tabela,
};
use utsim_ferramentas::relatorios::{Chart, ChartKind, Tone};
use utsim_model::Model;

use crate::modelo::TelaModelo;
use crate::sessao::Sessao;
use crate::trabalho::Trabalho;
use crate::util;

/// Pedido de gravar os limites de especificação (LIE, LSE) de uma ferramenta na
/// característica do nó de onde vieram os dados simulados (mão dupla com o modelo).
#[derive(Clone, Debug, PartialEq)]
pub struct PedidoLimites {
    /// Modelo como está na origem dos dados (relativo à pasta de unidade ou absoluto).
    pub modelo: String,
    pub objeto: String,
    pub caracteristica: String,
    pub lie: Option<f64>,
    pub lse: Option<f64>,
}

/// O pedido de limites de uma aplicação: só histograma e capacidade com dados simulados
/// de uma característica de um nó.
pub fn limites_da_aplicacao(a: &Aplicacao) -> Option<PedidoLimites> {
    if !matches!(
        a.ferramenta,
        Ferramenta::Histograma | Ferramenta::Capacidade
    ) {
        return None;
    }
    let Origem::Simulado {
        modelo,
        objeto,
        leitura,
        ..
    } = &a.origem
    else {
        return None;
    };
    let caracteristica = leitura.strip_prefix(CARACTERISTICA)?.trim();
    if modelo.is_empty() || caracteristica.is_empty() {
        return None;
    }
    Some(PedidoLimites {
        modelo: modelo.clone(),
        objeto: objeto.clone(),
        caracteristica: caracteristica.to_string(),
        lie: a.parametros.lie,
        lse: a.parametros.lse,
    })
}

/// Grava os limites do pedido na característica do modelo, com a trava do app
/// ([`TelaModelo::editar_arquivo`]). A característica é procurada primeiro no nó do pedido,
/// depois nos outros nós. Devolve o nó onde gravou.
pub fn gravar_limites(
    p: &PedidoLimites,
    s: &Sessao,
    modelo: &mut TelaModelo,
) -> Result<String, String> {
    let caminho = s
        .unidade
        .as_ref()
        .map_or_else(|| PathBuf::from(&p.modelo), |u| u.caminho_modelo(&p.modelo));
    let autor = s.config.autor.clone();
    modelo.editar_arquivo(&caminho, &autor, |doc| {
        let achar = |o: &utsim_model::ObjectDef| {
            o.quality
                .as_ref()
                .and_then(|q| {
                    q.characteristics
                        .iter()
                        .position(|c| c.name == p.caracteristica)
                })
                .map(|i| (o.name.clone(), i))
        };
        let objs = &doc.model().objects;
        let (no, i) = objs
            .iter()
            .filter(|o| o.name == p.objeto)
            .chain(objs.iter().filter(|o| o.name != p.objeto))
            .find_map(achar)
            .ok_or_else(|| format!("A característica {} não está no modelo.", p.caracteristica))?;
        for (campo, v) in [("lsl", p.lie), ("usl", p.lse)] {
            let valor = v.map(|x| x.to_string()).unwrap_or_default();
            let c = utsim_edit::abas::editar(
                doc.model(),
                &no,
                &format!("quality.characteristics[{i}].{campo}"),
                &valor,
            )?;
            doc.apply(c)?;
        }
        Ok(no)
    })
}

/// Leituras que a simulação oferece para a ferramenta neste nó (ex.: "defeitos por tipo",
/// "característica diâmetro").
pub type FnLeituras = Arc<dyn Fn(&Model, &str, Ferramenta) -> Vec<String> + Send + Sync>;
/// Roda a simulação e devolve a tabela com a origem Simulado. Argumentos: o modelo, o
/// arquivo do modelo, o objeto, a ferramenta, a leitura e as replicações.
pub type FnPuxar = Arc<
    dyn Fn(&Model, &str, &str, Ferramenta, &str, u32) -> Result<(Tabela, Origem), String>
        + Send
        + Sync,
>;
/// Os limites de especificação (LIE, LSE) de uma característica do modelo.
pub type FnLimites = Arc<dyn Fn(&Model, &str) -> (Option<f64>, Option<f64>) + Send + Sync>;

/// O que o app oferece para puxar dados da simulação (só o Qualidade).
#[derive(Clone)]
pub struct FnSimular {
    pub leituras: FnLeituras,
    pub puxar: FnPuxar,
    /// Preenche LIE e LSE do histograma e da capacidade quando a leitura é uma
    /// característica (`característica <nome>`).
    pub limites: Option<FnLimites>,
}

/// O prefixo das leituras de característica.
const CARACTERISTICA: &str = "característica ";

const LARGURA_LISTA: f32 = 270.0;
const AZUL: Color32 = Color32::from_rgb(90, 150, 235);
const VERDE: Color32 = Color32::from_rgb(90, 200, 120);
const AMBAR: Color32 = Color32::from_rgb(235, 170, 60);
const VERMELHO: Color32 = Color32::from_rgb(235, 95, 85);
const ROXO: Color32 = Color32::from_rgb(180, 130, 235);
const CINZA: Color32 = Color32::from_rgb(160, 168, 184);
/// As cores das séries dos gráficos.
const PALETA: [Color32; 6] = [
    AZUL,
    AMBAR,
    VERDE,
    ROXO,
    Color32::from_rgb(70, 200, 200),
    Color32::from_rgb(230, 120, 170),
];

/// O formulário de estudo novo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NovoEstudo {
    pub titulo: String,
    pub problema: String,
    pub posto: String,
    pub modelo: String,
    pub objeto: String,
}

/// Resultado de uma simulação puxada: a tabela, a origem e os limites (LIE, LSE).
type Puxado = (Tabela, Origem, Option<(Option<f64>, Option<f64>)>);

/// O estado de "Puxar da simulação" da aplicação escolhida.
struct Puxar {
    /// (modelo, objeto, ferramenta) das leituras carregadas.
    chave: Option<(String, String, Ferramenta)>,
    modelo: Option<Arc<Model>>,
    leituras: Vec<String>,
    leitura: String,
    replicacoes: u32,
    erro: String,
    trabalho: Trabalho<Puxado>,
    /// (estudo, aplicação) que recebe o resultado.
    alvo: (String, String),
}

impl Default for Puxar {
    fn default() -> Self {
        Puxar {
            chave: None,
            modelo: None,
            leituras: Vec::new(),
            leitura: String::new(),
            replicacoes: 5,
            erro: String::new(),
            trabalho: Trabalho::default(),
            alvo: (String::new(), String::new()),
        }
    }
}

pub struct TelaFerramentas {
    app: App,
    identidade: Identidade,
    simular: Option<FnSimular>,
    /// A versão da sessão da última leitura da lista.
    versao: Option<u64>,
    /// Os estudos da unidade (os deste app primeiro).
    pub estudos: Vec<Estudo>,
    pub filtro: String,
    /// Só os estudos deste nó (modelo, objeto).
    pub filtro_no: Option<(String, String)>,
    /// Pedido de abrir um nó, resolvido no próximo quadro (com a lista em dia).
    pedido_no: Option<(String, String)>,
    /// Limites que a pessoa pediu para gravar no nó, atendidos pelo app com a tela Modelo
    /// ([`TelaFerramentas::atender_limites`]).
    pub pedido_limites: Option<PedidoLimites>,
    /// Id do estudo aberto.
    pub selecionado: String,
    /// A cópia de trabalho do estudo aberto.
    pub estudo: Option<Estudo>,
    /// Id da aplicação escolhida.
    pub aplicacao: String,
    sujo: bool,
    sujo_desde: f64,
    pub novo: Option<NovoEstudo>,
    objetos_cache: Option<(String, Result<Vec<String>, String>)>,
    cache: Option<(Aplicacao, String, Resultado)>,
    csv: String,
    /// As abas da planilha do caminho digitado e a escolhida.
    abas: Option<(String, Result<Vec<String>, String>)>,
    aba: String,
    /// Aviso sobre a origem dos dados (editados à mão, importados).
    nota: String,
    puxar: Puxar,
    /// Arquivos do último relatório gravado.
    pub relatorios: Vec<PathBuf>,
    relatorio_estudo: bool,
    /// Aplicações fora do relatório do estudo (o padrão é todas).
    desmarcadas: Vec<String>,
    /// Remoção à espera de confirmação (`estudo` ou o id da aplicação).
    confirmar: Option<String>,
    /// Arquivos abertos pela linha de comando sem ferramenta: (nome, tabela, origem) e os
    /// parâmetros pedidos. A pessoa escolhe a ferramenta.
    pub dados_pendentes: Option<(Vec<crate::abrir::Arquivo>, Parametros)>,
}

impl TelaFerramentas {
    pub fn nova(app: App, identidade: Identidade, simular: Option<FnSimular>) -> TelaFerramentas {
        TelaFerramentas {
            app,
            identidade,
            simular,
            versao: None,
            estudos: Vec::new(),
            filtro: String::new(),
            filtro_no: None,
            pedido_no: None,
            pedido_limites: None,
            selecionado: String::new(),
            estudo: None,
            aplicacao: String::new(),
            sujo: false,
            sujo_desde: 0.0,
            novo: None,
            objetos_cache: None,
            cache: None,
            csv: String::new(),
            abas: None,
            aba: String::new(),
            nota: String::new(),
            puxar: Puxar::default(),
            relatorios: Vec::new(),
            relatorio_estudo: false,
            desmarcadas: Vec::new(),
            confirmar: None,
            dados_pendentes: None,
        }
    }

    /// Cria um estudo com uma ferramenta `f` por arquivo, com os dados de origem
    /// Importado e os parâmetros pedidos (LIE, LSE, subgrupo). Devolve o id.
    pub fn importar_estudo(
        &mut self,
        s: &mut Sessao,
        arquivos: Vec<crate::abrir::Arquivo>,
        f: Ferramenta,
        parametros: &Parametros,
    ) -> Option<String> {
        if arquivos.is_empty() {
            return None;
        }
        let nomes: Vec<&str> = arquivos.iter().map(|(n, _, _)| n.as_str()).collect();
        let n = NovoEstudo {
            titulo: format!("Análise de {}", nomes.join(", ")),
            ..NovoEstudo::default()
        };
        let id = self.criar(s, &n)?;
        let e = self.estudo.as_mut()?;
        for (nome, t, o) in arquivos {
            let mut a = Aplicacao::nova(&e.novo_id_aplicacao(), f);
            a.titulo = std::path::Path::new(&nome)
                .file_stem()
                .map_or(nome.clone(), |x| x.to_string_lossy().into_owned());
            a.parametros = parametros.clone();
            utsim_estudo::importar::aplicar(&mut a, t, o);
            e.aplicacoes.push(a);
        }
        self.aplicacao = e
            .aplicacoes
            .first()
            .map(|a| a.id.clone())
            .unwrap_or_default();
        self.trocou_aplicacao();
        self.sujo = true;
        self.gravar_pendente(s);
        Some(id)
    }

    /// A faixa "escolha a ferramenta" dos arquivos abertos sem ferramenta.
    fn faixa_pendentes(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        let Some((arquivos, _)) = self.dados_pendentes.as_ref() else {
            return;
        };
        let nomes: Vec<String> = arquivos.iter().map(|(n, _, _)| n.clone()).collect();
        let mut escolhida = None;
        let mut descartar = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "Dados de {}: escolha a ferramenta.",
                    nomes.join(", ")
                ))
                .strong(),
            );
            ui.horizontal_wrapped(|ui| {
                for f in Ferramenta::do_app(self.app).iter().copied() {
                    if f.usa_tabela() && ui.button(f.nome()).on_hover_text(f.ajuda()).clicked() {
                        escolhida = Some(f);
                    }
                }
                if ui.button("Descartar").clicked() {
                    descartar = true;
                }
            });
        });
        if let Some(f) = escolhida
            && let Some((arquivos, p)) = self.dados_pendentes.take()
        {
            self.importar_estudo(s, arquivos, f, &p);
        } else if descartar {
            self.dados_pendentes = None;
        }
    }

    // ---- dados -----------------------------------------------------------------

    /// Relê a lista quando a unidade mudou. A cópia de trabalho com mudança pendente fica.
    pub fn sincronizar(&mut self, s: &Sessao) {
        if self.versao == Some(s.versao) {
            return;
        }
        self.versao = Some(s.versao);
        let Some(u) = s.unidade.as_ref() else {
            self.estudos.clear();
            self.estudo = None;
            return;
        };
        let mut v: Vec<Estudo> = u.estado().lista(ESTUDOS);
        let app = self.app;
        v.sort_by(|a, b| {
            (a.app != app)
                .cmp(&(b.app != app))
                .then_with(|| a.titulo.to_lowercase().cmp(&b.titulo.to_lowercase()))
        });
        self.estudos = v;
        if !self.sujo {
            match self.estudos.iter().find(|e| e.id == self.selecionado) {
                Some(e) => {
                    if self.estudo.as_ref() != Some(e) {
                        self.estudo = Some(e.clone());
                    }
                }
                None => self.estudo = None,
            }
        }
    }

    /// Grava a mudança pendente do estudo aberto.
    pub fn gravar_pendente(&mut self, s: &mut Sessao) {
        if !self.sujo {
            return;
        }
        let (Some(e), Some(u)) = (self.estudo.as_ref(), s.unidade.as_mut()) else {
            return;
        };
        match u.gravar(ESTUDOS, &e.id, e) {
            Ok(_) => {
                self.sujo = false;
                if let Some(x) = self.estudos.iter_mut().find(|x| x.id == e.id) {
                    *x = e.clone();
                }
                s.mudou();
            }
            Err(err) => s.falhar(format!("não consegui gravar o estudo: {err}")),
        }
    }

    fn marcar_sujo(&mut self, agora: f64) {
        if !self.sujo {
            self.sujo = true;
            self.sujo_desde = agora;
        }
    }

    /// Cria e grava um estudo com o formulário. Devolve o id.
    pub fn criar(&mut self, s: &mut Sessao, n: &NovoEstudo) -> Option<String> {
        self.gravar_pendente(s);
        let u = s.unidade.as_mut()?;
        let e = Estudo {
            id: utsim_posto::pasta::novo_id(),
            titulo: if n.titulo.trim().is_empty() {
                "Estudo sem título".into()
            } else {
                n.titulo.trim().into()
            },
            app: self.app,
            assunto: Assunto {
                problema: n.problema.trim().into(),
                posto: n.posto.clone(),
                modelo: n.modelo.trim().into(),
                objeto: n.objeto.trim().into(),
            },
            autor: u.autor().to_string(),
            criado: utsim_estudo::hoje(),
            ..Estudo::default()
        };
        match u.gravar(ESTUDOS, &e.id, &e) {
            Ok(id) => {
                s.mudou();
                s.avisar(format!("Estudo '{}' criado.", e.titulo));
                self.sincronizar(s);
                self.abrir_estudo(s, &id);
                Some(id)
            }
            Err(err) => {
                s.falhar(err.to_string());
                None
            }
        }
    }

    /// Abre um estudo da lista (grava antes o que estava aberto).
    pub fn abrir_estudo(&mut self, s: &mut Sessao, id: &str) {
        self.gravar_pendente(s);
        self.selecionado = id.to_string();
        self.estudo = self.estudos.iter().find(|e| e.id == id).cloned();
        self.aplicacao = self
            .estudo
            .as_ref()
            .and_then(|e| e.aplicacoes.first())
            .map(|a| a.id.clone())
            .unwrap_or_default();
        self.trocou_aplicacao();
        self.relatorios.clear();
        self.relatorio_estudo = false;
        self.desmarcadas.clear();
        self.confirmar = None;
    }

    /// Escolhe a aplicação do estudo aberto.
    pub fn escolher_aplicacao(&mut self, id: &str) {
        if self.aplicacao != id {
            self.aplicacao = id.to_string();
            self.trocou_aplicacao();
        }
    }

    fn trocou_aplicacao(&mut self) {
        self.nota.clear();
        self.csv.clear();
        self.puxar.chave = None;
        self.puxar.erro.clear();
        self.confirmar = None;
    }

    /// Atende o pedido de gravar os limites no nó, se houver, e diz o resultado na barra.
    pub fn atender_limites(&mut self, s: &mut Sessao, modelo: &mut TelaModelo) {
        let Some(p) = self.pedido_limites.take() else {
            return;
        };
        match gravar_limites(&p, s, modelo) {
            Ok(no) => s.avisar(format!(
                "LIE e LSE gravados na característica {} do nó {no}.",
                p.caracteristica
            )),
            Err(e) => s.falhar(format!("Os limites não foram para o nó: {e}")),
        }
    }

    /// Mostra os estudos do nó e, se não há nenhum, prepara um estudo novo com o nó.
    pub fn abrir_no(&mut self, modelo: &str, objeto: &str) {
        self.filtro_no = Some((modelo.to_string(), objeto.to_string()));
        self.pedido_no = Some((modelo.to_string(), objeto.to_string()));
    }

    /// Atende o pedido de [`abrir_no`](Self::abrir_no) com a lista em dia.
    pub fn resolver_pedido(&mut self, s: &mut Sessao) {
        self.sincronizar(s);
        let Some((modelo, objeto)) = self.pedido_no.take() else {
            return;
        };
        let do_no: Vec<String> = self
            .estudos
            .iter()
            .filter(|e| e.assunto.eh_do_no(&modelo, &objeto))
            .map(|e| e.id.clone())
            .collect();
        if do_no.is_empty() {
            self.novo = Some(NovoEstudo {
                titulo: format!("Estudo do nó {objeto}"),
                modelo,
                objeto,
                ..NovoEstudo::default()
            });
        } else {
            self.novo = None;
            if !do_no.contains(&self.selecionado) {
                self.abrir_estudo(s, &do_no[0]);
            }
        }
    }

    /// Os estudos (id, título) de um nó do modelo, lidos da unidade.
    pub fn estudos_do_no(&self, s: &Sessao, modelo: &str, objeto: &str) -> Vec<(String, String)> {
        estudos_do_no(s, modelo, objeto)
    }

    fn nome_do_posto(s: &Sessao, id: &str) -> Option<String> {
        if id.is_empty() {
            return None;
        }
        s.unidade
            .as_ref()?
            .postos()
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.nome)
    }

    // ---- relatórios ------------------------------------------------------------

    /// Grava o PDF e o HTML de uma ferramenta (`aplicacao` dada) ou do estudo (`None`, com
    /// as aplicações marcadas).
    pub fn gravar_relatorio(
        &mut self,
        s: &mut Sessao,
        e: &Estudo,
        aplicacao: Option<&str>,
    ) -> Result<Vec<PathBuf>, String> {
        let posto = Self::nome_do_posto(s, &e.assunto.posto);
        let u = s.unidade.as_ref().ok_or("Nenhuma unidade aberta.")?;
        let data = utsim_estudo::hoje();
        let ctx = Contexto {
            identidade: &self.identidade,
            autor: u.autor(),
            data: &data,
            nome_do_posto: posto.as_deref(),
        };
        let (doc, nome) = match aplicacao {
            Some(id) => {
                let a = e.aplicacao(id).ok_or("Ferramenta não encontrada.")?;
                (
                    relatorio::documento_ferramenta(e, a, &ctx),
                    format!("{}_{}", slug(&e.titulo), slug(&a.titulo)),
                )
            }
            None => {
                let ids: Vec<String> = e
                    .aplicacoes
                    .iter()
                    .filter(|a| !self.desmarcadas.contains(&a.id))
                    .map(|a| a.id.clone())
                    .collect();
                let ids = if ids.len() == e.aplicacoes.len() {
                    Vec::new()
                } else {
                    ids
                };
                (
                    relatorio::documento_estudo(e, &ids, &ctx),
                    format!("{}_estudo", slug(&e.titulo)),
                )
            }
        };
        let t = relatorio::modelo_do_app(&self.identidade);
        let emitido = format!("{}, {data}", u.autor());
        let pdf = relatorio::pdf(&doc, &t, &emitido);
        let html = relatorio::html(&doc, &t, &emitido);
        let p1 = u
            .gravar_relatorio(&format!("{nome}.pdf"), &pdf)
            .map_err(|x| x.to_string())?;
        let p2 = u
            .gravar_relatorio(&format!("{nome}.html"), html.as_bytes())
            .map_err(|x| x.to_string())?;
        Ok(vec![p1, p2])
    }

    // ---- simulação ---------------------------------------------------------------

    fn sondar_puxar(&mut self, ctx: &egui::Context, s: &mut Sessao, agora: f64) {
        if !self.puxar.trabalho.sondar(ctx) {
            return;
        }
        let (eid, aid) = self.puxar.alvo.clone();
        match self.puxar.trabalho.resultado.take() {
            Some(Ok((t, o, lim))) => {
                let aplicar = |a: &mut Aplicacao| {
                    a.tabela = t.clone();
                    a.origem = o.clone();
                    a.parametros.colunas.clear();
                    if let Some((lie, lse)) = lim {
                        if a.parametros.lie.is_none() {
                            a.parametros.lie = lie;
                        }
                        if a.parametros.lse.is_none() {
                            a.parametros.lse = lse;
                        }
                    }
                };
                if self.selecionado == eid
                    && let Some(e) = self.estudo.as_mut()
                    && let Some(a) = e.aplicacao_mut(&aid)
                {
                    aplicar(a);
                    self.marcar_sujo(agora);
                    self.nota.clear();
                    s.avisar("Dados puxados da simulação.");
                } else if let Some(mut e) = self.estudos.iter().find(|e| e.id == eid).cloned()
                    && let Some(a) = e.aplicacao_mut(&aid)
                {
                    aplicar(a);
                    if let Some(u) = s.unidade.as_mut() {
                        let r = u.gravar(ESTUDOS, &e.id, &e);
                        s.resultado(r, "Dados puxados da simulação e gravados no estudo.");
                    }
                }
            }
            Some(Err(err)) => {
                self.puxar.erro = err.clone();
                s.falhar(format!("Simulação: {err}"));
            }
            None => {}
        }
    }

    // ---- tela ------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        if s.unidade.is_none() {
            ui.label("Abra uma unidade na tela inicial.");
            return;
        }
        let agora = ui.ctx().input(|i| i.time);
        self.resolver_pedido(s);
        self.sondar_puxar(ui.ctx(), s, agora);
        self.faixa_pendentes(ui, s);
        let h = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(LARGURA_LISTA, h),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(vec2(LARGURA_LISTA, h));
                    self.lista(ui, s);
                },
            );
            ui.separator();
            let w = ui.available_width() - 6.0;
            ui.allocate_ui_with_layout(vec2(w, h), Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(vec2(w, h));
                self.corpo(ui, s, agora);
            });
        });
        if self.sujo {
            let foco = ui.ctx().memory(|m| m.focused().is_some());
            if !foco || agora - self.sujo_desde >= 1.0 || agora < self.sujo_desde {
                self.gravar_pendente(s);
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
    }

    fn lista(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        ui.heading("Estudos");
        ui.add(
            egui::TextEdit::singleline(&mut self.filtro)
                .hint_text("filtrar por título ou assunto")
                .desired_width(f32::INFINITY),
        );
        if let Some((_, objeto)) = self.filtro_no.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Só do nó {objeto}")).color(AMBAR));
                if ui.small_button("Mostrar todos").clicked() {
                    self.filtro_no = None;
                }
            });
        }
        if self.novo.is_none() && ui.button("Novo estudo").clicked() {
            self.novo = Some(NovoEstudo::default());
        }
        if self.novo.is_some() {
            self.formulario_novo(ui, s);
        }
        ui.add_space(4.0);
        let f = self.filtro.trim().to_lowercase();
        let postos = s.unidade.as_ref().map(|u| u.postos()).unwrap_or_default();
        let visiveis: Vec<usize> = self
            .estudos
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                self.filtro_no
                    .as_ref()
                    .is_none_or(|(m, o)| e.assunto.eh_do_no(m, o))
            })
            .filter(|(_, e)| {
                f.is_empty()
                    || e.titulo.to_lowercase().contains(&f)
                    || e.assunto.resumo(None).to_lowercase().contains(&f)
                    || e.assunto.problema.to_lowercase().contains(&f)
            })
            .map(|(i, _)| i)
            .collect();
        if visiveis.is_empty() {
            ui.weak(if self.estudos.is_empty() {
                "A unidade ainda não tem estudos."
            } else {
                "Nenhum estudo passa pelo filtro."
            });
        }
        let mut abrir: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("lista_estudos")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for i in visiveis {
                    let e = &self.estudos[i];
                    let marcado = e.id == self.selecionado;
                    let nome_posto = postos
                        .iter()
                        .find(|p| p.id == e.assunto.posto)
                        .map(|p| p.nome.as_str());
                    let fundo = if marcado {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals().faint_bg_color
                    };
                    let r = egui::Frame::new()
                        .fill(fundo)
                        .corner_radius(4)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(&e.titulo).strong());
                            ui.label(RichText::new(e.assunto.resumo(nome_posto)).small());
                            let mut linha = format!(
                                "{} {}",
                                e.aplicacoes.len(),
                                if e.aplicacoes.len() == 1 {
                                    "ferramenta"
                                } else {
                                    "ferramentas"
                                }
                            );
                            if e.app != self.app {
                                linha.push_str(&format!(" · {}", e.app.nome()));
                            }
                            ui.label(RichText::new(linha).small().weak());
                        })
                        .response
                        .interact(egui::Sense::click());
                    if r.clicked() {
                        abrir = Some(e.id.clone());
                    }
                    ui.add_space(3.0);
                }
            });
        if let Some(id) = abrir {
            self.abrir_estudo(s, &id);
        }
    }

    fn formulario_novo(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        let Some(mut n) = self.novo.take() else {
            return;
        };
        let (postos, modelos) = match s.unidade.as_ref() {
            Some(u) => {
                let p = u.postos();
                let mut m: Vec<String> = p.iter().filter_map(|p| p.modelo.clone()).collect();
                m.sort();
                m.dedup();
                (p, m)
            }
            None => (Vec::new(), Vec::new()),
        };
        // Os objetos do modelo digitado, para escolher o nó.
        let modelo = n.modelo.trim().to_string();
        if !modelo.is_empty()
            && self
                .objetos_cache
                .as_ref()
                .is_none_or(|(m, _)| *m != modelo)
            && let Some(u) = s.unidade.as_ref()
        {
            let r = util::carregar_modelo(u, &modelo)
                .map(|m| m.objects.iter().map(|o| o.name.clone()).collect());
            self.objetos_cache = Some((modelo.clone(), r));
        }
        let mut criar = false;
        let mut cancelar = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong("Novo estudo");
            ui.label("Título");
            ui.add(egui::TextEdit::singleline(&mut n.titulo).desired_width(f32::INFINITY));
            ui.label("Problema em uma frase");
            ui.add(egui::TextEdit::singleline(&mut n.problema).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label("Posto");
                let atual = postos
                    .iter()
                    .find(|p| p.id == n.posto)
                    .map_or("nenhum".to_string(), |p| p.nome.clone());
                egui::ComboBox::from_id_salt("novo_posto")
                    .selected_text(atual)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut n.posto, String::new(), "nenhum");
                        for p in &postos {
                            if ui
                                .selectable_value(&mut n.posto, p.id.clone(), &p.nome)
                                .clicked()
                                && n.modelo.is_empty()
                                && let (Some(m), Some(o)) = (&p.modelo, &p.objeto)
                            {
                                n.modelo = m.clone();
                                n.objeto = o.clone();
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Modelo");
                crate::util::campo_modelo(ui, &mut n.modelo, s.unidade.as_ref(), 130.0);
                if !modelos.is_empty() {
                    egui::ComboBox::from_id_salt("novo_modelo")
                        .selected_text("dos postos")
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for m in &modelos {
                                ui.selectable_value(&mut n.modelo, m.clone(), m);
                            }
                        });
                }
            });
            ui.horizontal(|ui| {
                ui.label("Nó");
                ui.add(egui::TextEdit::singleline(&mut n.objeto).desired_width(130.0));
                if let Some((m, Ok(objs))) = &self.objetos_cache
                    && *m == n.modelo.trim()
                    && !objs.is_empty()
                {
                    egui::ComboBox::from_id_salt("novo_objeto")
                        .selected_text("do modelo")
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for o in objs {
                                ui.selectable_value(&mut n.objeto, o.clone(), o);
                            }
                        });
                }
            });
            if let Some((m, Err(e))) = &self.objetos_cache
                && *m == n.modelo.trim()
            {
                ui.label(RichText::new(e).small().color(VERMELHO));
            }
            ui.horizontal(|ui| {
                if ui.button("Criar estudo").clicked() {
                    criar = true;
                }
                if ui.button("Cancelar").clicked() {
                    cancelar = true;
                }
            });
        });
        if criar {
            self.criar(s, &n);
        } else if !cancelar {
            self.novo = Some(n);
        }
    }

    fn corpo(&mut self, ui: &mut egui::Ui, s: &mut Sessao, agora: f64) {
        let Some(mut e) = self.estudo.take() else {
            ui.add_space(24.0);
            ui.heading("Ferramentas da qualidade em estudos");
            ui.label(
                "Um estudo aplica as ferramentas a um assunto: um nó do modelo, um posto ou um \
                 problema. Os dados vêm da simulação, de um CSV ou são digitados.",
            );
            ui.label("Escolha um estudo na lista ou crie um novo.");
            return;
        };
        let antes = e.clone();
        let mut remover_estudo = false;
        self.cabecalho(ui, s, &mut e, &mut remover_estudo);
        ui.separator();
        self.chips(ui, &mut e);
        ui.separator();
        if let Some(i) = e.aplicacoes.iter().position(|a| a.id == self.aplicacao) {
            egui::ScrollArea::vertical()
                .id_salt(("aplicacao", e.id.clone(), self.aplicacao.clone()))
                .auto_shrink([false, false])
                .show(ui, |ui| self.aplicacao_ui(ui, s, &mut e, i));
        } else if e.aplicacoes.is_empty() {
            ui.weak("O estudo ainda não tem ferramentas. Use \"Adicionar ferramenta\".");
        } else {
            ui.weak("Escolha uma ferramenta do estudo.");
        }
        if remover_estudo {
            if let Some(u) = s.unidade.as_mut() {
                let r = u.remover(ESTUDOS, &e.id);
                s.resultado(r, "Estudo removido.");
            }
            self.sujo = false;
            self.selecionado.clear();
            self.estudos.retain(|x| x.id != e.id);
            return;
        }
        if e != antes {
            self.marcar_sujo(agora);
        }
        self.estudo = Some(e);
    }

    fn cabecalho(&mut self, ui: &mut egui::Ui, s: &mut Sessao, e: &mut Estudo, remover: &mut bool) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut e.titulo)
                    .font(egui::TextStyle::Heading)
                    .hint_text("título do estudo")
                    .desired_width((ui.available_width() - 330.0).max(200.0)),
            );
            if ui.button("Relatório do estudo").clicked() {
                self.relatorio_estudo = !self.relatorio_estudo;
            }
            if self.confirmar.as_deref() == Some("estudo") {
                if ui
                    .button(RichText::new("Confirmar remoção").color(VERMELHO))
                    .clicked()
                {
                    *remover = true;
                }
                if ui.button("Não").clicked() {
                    self.confirmar = None;
                }
            } else if ui.button("Remover estudo").clicked() {
                self.confirmar = Some("estudo".into());
            }
        });
        let posto = Self::nome_do_posto(s, &e.assunto.posto);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(e.assunto.resumo(posto.as_deref())).color(AZUL));
            let mut info = Vec::new();
            if !e.autor.is_empty() {
                info.push(format!("responsável {}", e.autor));
            }
            if !e.criado.is_empty() {
                info.push(format!("aberto em {}", e.criado));
            }
            if e.app != self.app {
                info.push(format!("criado no {}", e.app.nome()));
            }
            if !info.is_empty() {
                ui.weak(info.join(" · "));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Problema");
            ui.add(
                egui::TextEdit::singleline(&mut e.assunto.problema)
                    .hint_text("o problema em uma frase")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.horizontal_top(|ui| {
            egui::CollapsingHeader::new("Assunto")
                .id_salt(("assunto", e.id.clone()))
                .show(ui, |ui| {
                    let postos = s.unidade.as_ref().map(|u| u.postos()).unwrap_or_default();
                    egui::Grid::new(("grade_assunto", e.id.clone()))
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Posto");
                            let atual = postos
                                .iter()
                                .find(|p| p.id == e.assunto.posto)
                                .map_or("nenhum".to_string(), |p| p.nome.clone());
                            egui::ComboBox::from_id_salt(("assunto_posto", e.id.clone()))
                                .selected_text(atual)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut e.assunto.posto,
                                        String::new(),
                                        "nenhum",
                                    );
                                    for p in &postos {
                                        ui.selectable_value(
                                            &mut e.assunto.posto,
                                            p.id.clone(),
                                            &p.nome,
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label("Modelo");
                            crate::util::campo_modelo(
                                ui,
                                &mut e.assunto.modelo,
                                s.unidade.as_ref(),
                                260.0,
                            );
                            ui.end_row();
                            ui.label("Nó");
                            ui.add(
                                egui::TextEdit::singleline(&mut e.assunto.objeto)
                                    .desired_width(160.0),
                            );
                            ui.end_row();
                        });
                });
            egui::CollapsingHeader::new("Conclusão do estudo")
                .id_salt(("conclusao_estudo", e.id.clone()))
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut e.conclusao)
                            .hint_text("o que o estudo concluiu e o que será feito")
                            .desired_rows(3)
                            .desired_width(420.0),
                    );
                });
        });
        if self.relatorio_estudo {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong("Ferramentas no relatório do estudo");
                ui.horizontal_wrapped(|ui| {
                    for a in &e.aplicacoes {
                        let mut marcado = !self.desmarcadas.contains(&a.id);
                        if ui.checkbox(&mut marcado, &a.titulo).changed() {
                            if marcado {
                                self.desmarcadas.retain(|x| x != &a.id);
                            } else {
                                self.desmarcadas.push(a.id.clone());
                            }
                        }
                    }
                });
                let algum = e
                    .aplicacoes
                    .iter()
                    .any(|a| !self.desmarcadas.contains(&a.id));
                if ui
                    .add_enabled(algum, egui::Button::new("Gravar PDF e HTML do estudo"))
                    .clicked()
                {
                    match self.gravar_relatorio(s, e, None) {
                        Ok(p) => {
                            s.avisar("Relatório do estudo gravado.");
                            self.relatorios = p;
                        }
                        Err(err) => s.falhar(err),
                    }
                }
            });
        }
        self.arquivos_gravados(ui, s);
    }

    fn arquivos_gravados(&mut self, ui: &mut egui::Ui, s: &mut Sessao) {
        if self.relatorios.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Gravado:");
            for p in &self.relatorios {
                ui.label(RichText::new(p.display().to_string()).small().weak());
            }
            if let Some(h) = self
                .relatorios
                .iter()
                .find(|p| p.extension().is_some_and(|x| x == "html"))
                && ui.button("Abrir").clicked()
                && let Err(err) = util::abrir_com_sistema(h)
            {
                s.falhar(format!("não consegui abrir {}: {err}", h.display()));
            }
        });
    }

    fn chips(&mut self, ui: &mut egui::Ui, e: &mut Estudo) {
        let mut escolher: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.strong("Ferramentas do estudo:");
            for a in &e.aplicacoes {
                if ui
                    .selectable_label(a.id == self.aplicacao, &a.titulo)
                    .on_hover_text(a.ferramenta.nome())
                    .clicked()
                {
                    escolher = Some(a.id.clone());
                }
            }
            ui.menu_button("+ Adicionar ferramenta", |ui| {
                ui.set_min_width(360.0);
                for f in Ferramenta::do_app(self.app) {
                    if ui.button(RichText::new(f.nome()).strong()).clicked() {
                        let id = e.novo_id_aplicacao();
                        e.aplicacoes.push(Aplicacao::nova(&id, *f));
                        escolher = Some(id);
                        ui.close();
                    }
                    ui.label(RichText::new(f.ajuda()).small().weak());
                    ui.add_space(2.0);
                }
            });
        });
        if let Some(id) = escolher {
            self.escolher_aplicacao(&id);
        }
    }

    fn aplicacao_ui(&mut self, ui: &mut egui::Ui, s: &mut Sessao, e: &mut Estudo, i: usize) {
        let n = e.aplicacoes.len();
        let mut mover: Option<usize> = None;
        let mut remover = false;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut e.aplicacoes[i].titulo)
                    .font(egui::TextStyle::Heading)
                    .desired_width((ui.available_width() - 330.0).max(200.0)),
            );
            if ui
                .add_enabled(i > 0, egui::Button::new("Subir"))
                .on_hover_text("Antes na ordem do estudo e do relatório")
                .clicked()
            {
                mover = Some(i - 1);
            }
            if ui
                .add_enabled(i + 1 < n, egui::Button::new("Descer"))
                .clicked()
            {
                mover = Some(i + 1);
            }
            let id = e.aplicacoes[i].id.clone();
            if self.confirmar.as_deref() == Some(id.as_str()) {
                if ui
                    .button(RichText::new("Confirmar remoção").color(VERMELHO))
                    .clicked()
                {
                    remover = true;
                }
                if ui.button("Não").clicked() {
                    self.confirmar = None;
                }
            } else if ui.button("Remover").clicked() {
                self.confirmar = Some(id);
            }
        });
        {
            let a = &e.aplicacoes[i];
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(a.ferramenta.nome()).strong());
                ui.weak(a.ferramenta.ajuda());
            });
            if a.ferramenta.usa_tabela() {
                ui.horizontal_wrapped(|ui| {
                    let cor = match a.origem {
                        Origem::Simulado { .. } => AMBAR,
                        Origem::Importado { .. } => ROXO,
                        Origem::Manual => CINZA,
                    };
                    selo(ui, a.origem.rotulo(), cor);
                    ui.label(RichText::new(a.origem.descricao()).small());
                });
            }
            if !self.nota.is_empty() {
                ui.label(RichText::new(&self.nota).color(AMBAR));
            }
        }
        ui.horizontal(|ui| {
            if ui
                .button("Relatório desta ferramenta (PDF e HTML)")
                .clicked()
            {
                let id = e.aplicacoes[i].id.clone();
                match self.gravar_relatorio(s, e, Some(&id)) {
                    Ok(p) => {
                        s.avisar("Relatório da ferramenta gravado.");
                        self.relatorios = p;
                    }
                    Err(err) => s.falhar(err),
                }
            }
        });
        ui.add_space(4.0);
        let problema = e.assunto.problema.clone();
        let assunto = e.assunto.clone();
        let eid = e.id.clone();
        let a = &mut e.aplicacoes[i];
        ui.columns(2, |cols| {
            cols[0].strong("Dados");
            self.dados_ui(&mut cols[0], s, a, &assunto, &eid);
            cols[0].add_space(6.0);
            cols[0].strong("Leitura da equipe");
            cols[0].add(
                egui::TextEdit::multiline(&mut a.conclusao)
                    .hint_text("o que o resultado mostra e o que fazer")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            cols[1].strong("Resultado");
            self.resultado_ui(&mut cols[1], a, &problema);
        });
        if let Some(j) = mover {
            e.aplicacoes.swap(i, j);
        }
        if remover {
            e.aplicacoes.remove(i);
            self.confirmar = None;
            let prox = e
                .aplicacoes
                .get(i.min(e.aplicacoes.len().saturating_sub(1)))
                .map(|a| a.id.clone())
                .unwrap_or_default();
            self.escolher_aplicacao(&prox);
        }
    }

    // ---- dados ---------------------------------------------------------------------

    fn dados_ui(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Sessao,
        a: &mut Aplicacao,
        assunto: &Assunto,
        eid: &str,
    ) {
        match a.ferramenta {
            Ferramenta::Ishikawa => ishikawa_ui(ui, a, &assunto.problema),
            Ferramenta::CincoPorques => porques_ui(ui, a, &assunto.problema),
            Ferramenta::A3 => a3_ui(ui, a),
            _ => {
                parametros_ui(ui, a);
                if let Some(p) = limites_da_aplicacao(a)
                    && ui
                        .button("Gravar LIE e LSE no nó")
                        .on_hover_text(format!(
                            "Grava estes limites na característica {} do modelo (aba \
                             Qualidade do nó {}), com a trava do modelo.",
                            p.caracteristica, p.objeto
                        ))
                        .clicked()
                {
                    self.pedido_limites = Some(p);
                }
                if let Some(dica) = dica_da_tabela(a.ferramenta) {
                    ui.label(RichText::new(dica).small().weak());
                }
                ui.add_space(4.0);
                a.completar_auxiliares();
                let antes = (a.tabela.clone(), a.auxiliares.clone());
                tabela_ui(ui, &mut a.tabela, &a.id);
                for (i, (titulo, _)) in a.ferramenta.auxiliares().iter().enumerate() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(*titulo).strong());
                    if let Some(dica) = dica_da_auxiliar(a.ferramenta, i) {
                        ui.label(RichText::new(dica).small().weak());
                    }
                    tabela_ui(ui, &mut a.auxiliares[i], &format!("{}-aux{i}", a.id));
                }
                if (&a.tabela, &a.auxiliares) != (&antes.0, &antes.1) && a.origem != Origem::Manual
                {
                    a.origem = Origem::Manual;
                    self.nota = "Dados editados à mão: a origem passou a Manual.".into();
                }
                ui.add_space(4.0);
                self.origem_ui(ui, s, a, assunto, eid);
            }
        }
    }

    fn origem_ui(
        &mut self,
        ui: &mut egui::Ui,
        s: &mut Sessao,
        a: &mut Aplicacao,
        assunto: &Assunto,
        eid: &str,
    ) {
        egui::CollapsingHeader::new("Importar e exportar dados")
            .id_salt(("csv", a.id.clone()))
            .show(ui, |ui| {
                let largura = (ui.available_width() - 110.0).max(160.0);
                utsim_ui_tema::arquivos::campo(
                    ui,
                    &mut self.csv,
                    utsim_ui_tema::arquivos::Tipo::Abrir,
                    "Importar dados",
                    &[utsim_ui_tema::arquivos::Filtro {
                        nome: "Planilha (.csv, .xlsx)",
                        extensoes: &["csv", "xlsx"],
                    }],
                    largura,
                );
                let caminho = self.csv.trim().to_string();
                let xlsx = caminho.to_lowercase().ends_with(".xlsx");
                // As abas da planilha, lidas uma vez por caminho.
                if xlsx && self.abas.as_ref().is_none_or(|(c, _)| *c != caminho) {
                    let r = std::fs::read(&caminho)
                        .map_err(|e| e.to_string())
                        .and_then(|b| Tabela::abas_xlsx(&b));
                    self.aba = r
                        .as_ref()
                        .ok()
                        .and_then(|v| v.first().cloned())
                        .unwrap_or_default();
                    self.abas = Some((caminho.clone(), r));
                }
                ui.horizontal(|ui| {
                    if xlsx && let Some((_, r)) = &self.abas {
                        match r {
                            Ok(abas) => {
                                ui.label("Aba");
                                egui::ComboBox::from_id_salt(("aba", a.id.clone()))
                                    .selected_text(self.aba.clone())
                                    .width(140.0)
                                    .show_ui(ui, |ui| {
                                        for x in abas {
                                            ui.selectable_value(&mut self.aba, x.clone(), x);
                                        }
                                    });
                            }
                            Err(e) => {
                                ui.label(RichText::new(e).small().color(VERMELHO));
                            }
                        }
                    }
                    if ui
                        .add_enabled(
                            !caminho.is_empty(),
                            egui::Button::new("Importar CSV ou xlsx"),
                        )
                        .clicked()
                    {
                        let p = PathBuf::from(&caminho);
                        let nome = p.file_name().map_or_else(
                            || p.display().to_string(),
                            |f| f.to_string_lossy().into_owned(),
                        );
                        let aba = (xlsx && !self.aba.is_empty()).then_some(self.aba.as_str());
                        match std::fs::read(&p)
                            .map_err(|e| format!("{}: {e}", p.display()))
                            .and_then(|b| Tabela::importar(&nome, &b, aba))
                        {
                            Ok((t, o)) => {
                                a.tabela = t;
                                a.origem = o;
                                a.parametros.colunas.clear();
                                self.nota.clear();
                                s.avisar(format!("{nome} importado."));
                            }
                            Err(err) => s.falhar(err),
                        }
                    }
                });
                if ui.button("Exportar CSV").clicked()
                    && let Some(u) = s.unidade.as_ref()
                {
                    let nome = format!("{}.csv", slug(&a.titulo));
                    match u.gravar_relatorio(&nome, a.tabela.csv().as_bytes()) {
                        Ok(p) => s.avisar(format!("Tabela gravada em {}", p.display())),
                        Err(err) => s.falhar(err.to_string()),
                    }
                }
            });
        let Some(sim) = self.simular.clone() else {
            return;
        };
        if !a.ferramenta.simulavel() {
            return;
        }
        egui::CollapsingHeader::new("Puxar da simulação")
            .id_salt(("puxar", a.id.clone()))
            .show(ui, |ui| {
                if assunto.objeto.is_empty() || assunto.modelo.is_empty() {
                    ui.weak("O estudo não tem nó do modelo. Escolha um em \"Assunto\".");
                    return;
                }
                let chave = (assunto.modelo.clone(), assunto.objeto.clone(), a.ferramenta);
                if self.puxar.chave.as_ref() != Some(&chave) {
                    self.puxar.chave = Some(chave);
                    self.puxar.erro.clear();
                    self.puxar.leituras.clear();
                    self.puxar.modelo = None;
                    if let Some(u) = s.unidade.as_ref() {
                        match util::carregar_modelo(u, &assunto.modelo) {
                            Ok(m) => {
                                self.puxar.leituras =
                                    (sim.leituras)(&m, &assunto.objeto, a.ferramenta);
                                self.puxar.modelo = Some(Arc::new(m));
                            }
                            Err(err) => self.puxar.erro = err,
                        }
                    }
                    if !self.puxar.leituras.contains(&self.puxar.leitura) {
                        self.puxar.leitura =
                            self.puxar.leituras.first().cloned().unwrap_or_default();
                    }
                }
                if !self.puxar.erro.is_empty() {
                    ui.label(RichText::new(&self.puxar.erro).color(VERMELHO));
                }
                if self.puxar.modelo.is_none() {
                    return;
                }
                if self.puxar.leituras.is_empty() {
                    ui.weak("A simulação não oferece leituras desta ferramenta para o nó.");
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label("Leitura");
                    egui::ComboBox::from_id_salt(("leitura", a.id.clone()))
                        .selected_text(self.puxar.leitura.clone())
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for l in &self.puxar.leituras {
                                ui.selectable_value(&mut self.puxar.leitura, l.clone(), l);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Replicações");
                    ui.add(egui::DragValue::new(&mut self.puxar.replicacoes).range(1..=200));
                    let ocupado = self.puxar.trabalho.ocupado();
                    if ui
                        .add_enabled(!ocupado, egui::Button::new("Rodar e preencher a tabela"))
                        .clicked()
                        && let Some(m) = self.puxar.modelo.clone()
                    {
                        let (arquivo, objeto) = (assunto.modelo.clone(), assunto.objeto.clone());
                        let (f, leitura, reps) = (
                            a.ferramenta,
                            self.puxar.leitura.clone(),
                            self.puxar.replicacoes,
                        );
                        self.puxar.alvo = (eid.to_string(), a.id.clone());
                        self.puxar.erro.clear();
                        let sim = sim.clone();
                        self.puxar.trabalho.iniciar(move || {
                            let (t, o) = (sim.puxar)(&m, &arquivo, &objeto, f, &leitura, reps)?;
                            let lim = match (&sim.limites, leitura.strip_prefix(CARACTERISTICA)) {
                                (Some(l), Some(nome))
                                    if matches!(
                                        f,
                                        Ferramenta::Histograma | Ferramenta::Capacidade
                                    ) =>
                                {
                                    Some(l(&m, nome))
                                }
                                _ => None,
                            };
                            Ok((t, o, lim))
                        });
                    }
                    if ocupado {
                        ui.spinner();
                        ui.weak("simulando...");
                    }
                });
                ui.weak("A tabela atual é trocada pelo resultado simulado.");
            });
    }

    // ---- resultado ---------------------------------------------------------------

    fn resultado_ui(&mut self, ui: &mut egui::Ui, a: &Aplicacao, problema: &str) {
        let atual = self
            .cache
            .as_ref()
            .is_some_and(|(x, p, _)| x == a && p == problema);
        if !atual {
            let r = calculo::calcular(a, problema);
            self.cache = Some((a.clone(), problema.to_string(), r));
        }
        let Some((_, _, r)) = &self.cache else {
            return;
        };
        mostrar_resultado(ui, r, &a.id);
    }
}

/// Os estudos (id, título) de um nó do modelo, lidos da unidade da sessão.
pub fn estudos_do_no(s: &Sessao, modelo: &str, objeto: &str) -> Vec<(String, String)> {
    let Some(u) = s.unidade.as_ref() else {
        return Vec::new();
    };
    u.estado()
        .lista::<Estudo>(ESTUDOS)
        .into_iter()
        .filter(|e| e.assunto.eh_do_no(modelo, objeto))
        .map(|e| (e.id, e.titulo))
        .collect()
}

/// Um nome de arquivo a partir de um título: minúsculas, sem acentos, `_` no lugar do resto.
pub fn slug(texto: &str) -> String {
    let mut s = String::new();
    for c in texto.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_ascii_alphanumeric() => c,
            _ => '_',
        };
        if c == '_' && (s.is_empty() || s.ends_with('_')) {
            continue;
        }
        s.push(c);
    }
    let s: String = s.trim_end_matches('_').chars().take(60).collect();
    if s.is_empty() { "estudo".into() } else { s }
}

fn selo(ui: &mut egui::Ui, texto: &str, cor: Color32) {
    egui::Frame::new()
        .fill(cor.gamma_multiply(0.3))
        .stroke(Stroke::new(1.0, cor))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(texto).strong().color(cor));
        });
}

// ---- editores ------------------------------------------------------------------

/// Os papéis das colunas lidas, na ordem de `parametros.colunas`.
/// Como preencher a tabela principal das ferramentas que não se explicam pelas colunas.
fn dica_da_tabela(f: Ferramenta) -> Option<&'static str> {
    match f {
        Ferramenta::Qfd => Some(
            "Uma linha por necessidade do cliente, importância de 1 a 5. Cada coluna depois de \"Ponto de venda\" é um requisito técnico (+ coluna), com a relação 0, 1, 3 ou 9. Atual, concorrente e meta vão de 1 a 5, o ponto de venda é 1, 1,2 ou 1,5.",
        ),
        Ferramenta::Tolerancias => Some(
            "Uma linha por cota. O coeficiente é 1 para a cota que soma e -1 para a que subtrai. Sem a tolerância superior, a cota é simétrica. Distribuição: normal (±3σ), normal 4 (±4σ), uniforme ou triangular. São 100 mil montagens simuladas, com a mesma semente.",
        ),
        Ferramenta::PokaYoke => Some(
            "Uma linha por dispositivo. Tipo: contato, valor fixo ou sequência. Função: controle (baixa a ocorrência) ou alerta (baixa a detecção). Etapa e modo de falha como estão no FMEA, notas O e D de 1 a 10 antes e depois.",
        ),
        _ => None,
    }
}

fn dica_da_auxiliar(f: Ferramenta, i: usize) -> Option<&'static str> {
    match (f, i) {
        (Ferramenta::Qfd, 0) => {
            Some("Pelo nome da coluna: unidade, sentido (maior, menor ou alvo) e valor-alvo.")
        }
        (Ferramenta::Qfd, 1) => Some(
            "Dois requisitos e a correlação: ++ e + quando um ajuda o outro, - e -- quando um atrapalha o outro.",
        ),
        (Ferramenta::PokaYoke, 0) => Some(
            "Cada teste do dispositivo: data (DD/MM/AAAA), se funcionou (sim ou não), quem testou e uma nota.",
        ),
        _ => None,
    }
}

fn papeis(a: &Aplicacao) -> &'static [&'static str] {
    match a.ferramenta {
        Ferramenta::FolhaVerificacao => &["Categoria"],
        Ferramenta::Estratificacao => &["Estrato", "Valor"],
        Ferramenta::Pareto => &["Categoria", "Quantidade", "Custo unitário"],
        Ferramenta::Histograma | Ferramenta::Capacidade => &["Valor"],
        Ferramenta::Dispersao => &["X", "Y"],
        Ferramenta::CartaControle => {
            if calculo::tipo_carta(a).is_attribute() {
                &["Amostra", "Contagem", "Tamanho"]
            } else {
                &["Subgrupo", "Valor"]
            }
        }
        Ferramenta::Msa => &["Peça", "Operador", "Valor"],
        _ => &[],
    }
}

const CARTAS: [(&str, &str); 8] = [
    ("", "automática pelo subgrupo"),
    ("imr", "I-MR (individuais)"),
    ("xbarr", "X̄-R"),
    ("xbars", "X̄-S"),
    ("p", "p (proporção)"),
    ("np", "np (defeituosos)"),
    ("c", "c (defeitos)"),
    ("u", "u (defeitos por unidade)"),
];

const REGRAS: [(&str, &str); 3] = [
    ("", "básicas"),
    ("western", "Western Electric"),
    ("nelson", "Nelson"),
];

fn opcional(ui: &mut egui::Ui, rotulo: &str, v: &mut Option<f64>) {
    let mut ligado = v.is_some();
    ui.checkbox(&mut ligado, rotulo);
    match (ligado, v.is_some()) {
        (true, false) => *v = Some(0.0),
        (false, true) => *v = None,
        _ => {}
    }
    if let Some(x) = v {
        ui.add(egui::DragValue::new(x).speed(0.01).max_decimals(4));
    }
}

fn parametros_ui(ui: &mut egui::Ui, a: &mut Aplicacao) {
    let papeis = papeis(a);
    let colunas = a.tabela.colunas.clone();
    let f = a.ferramenta;
    let p = &mut a.parametros;
    egui::Grid::new(("parametros", a.id.clone()))
        .num_columns(2)
        .spacing(vec2(8.0, 4.0))
        .show(ui, |ui| {
            for (i, papel) in papeis.iter().enumerate() {
                if colunas.is_empty() {
                    break;
                }
                ui.label(format!("Coluna {}", papel.to_lowercase()));
                let mut sel = p.colunas.get(i).copied().unwrap_or(i);
                let texto = colunas
                    .get(sel)
                    .cloned()
                    .unwrap_or_else(|| "nenhuma".into());
                egui::ComboBox::from_id_salt(("papel", a.id.clone(), i))
                    .selected_text(texto)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (k, c) in colunas.iter().enumerate() {
                            ui.selectable_value(&mut sel, k, c);
                        }
                        // O custo unitário do Pareto é opcional.
                        if f == Ferramenta::Pareto && i == 2 {
                            ui.selectable_value(&mut sel, usize::MAX, "nenhuma");
                        }
                    });
                if sel != p.colunas.get(i).copied().unwrap_or(i) {
                    while p.colunas.len() <= i {
                        p.colunas.push(p.colunas.len());
                    }
                    p.colunas[i] = sel;
                }
                ui.end_row();
            }
            if matches!(f, Ferramenta::Histograma | Ferramenta::Capacidade) {
                ui.label("Especificação");
                ui.horizontal(|ui| {
                    opcional(ui, "LIE", &mut p.lie);
                    opcional(ui, "LSE", &mut p.lse);
                    opcional(ui, "Alvo", &mut p.alvo);
                });
                ui.end_row();
            }
            if f == Ferramenta::Tolerancias {
                ui.label("Limites do resultado");
                ui.horizontal(|ui| {
                    opcional(ui, "LIE", &mut p.lie);
                    opcional(ui, "LSE", &mut p.lse);
                });
                ui.end_row();
            }
            if matches!(f, Ferramenta::CartaControle | Ferramenta::Capacidade) {
                ui.label("Tamanho do subgrupo");
                ui.add(egui::DragValue::new(&mut p.subgrupo).range(1..=25));
                ui.end_row();
            }
            if f == Ferramenta::CartaControle {
                ui.label("Tipo de carta");
                let atual = CARTAS
                    .iter()
                    .find(|(k, _)| *k == p.carta)
                    .map_or(p.carta.clone(), |(_, n)| n.to_string());
                egui::ComboBox::from_id_salt(("carta", a.id.clone()))
                    .selected_text(atual)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (k, n) in CARTAS {
                            ui.selectable_value(&mut p.carta, k.to_string(), n);
                        }
                    });
                ui.end_row();
                ui.label("Regras de sinal");
                let atual = REGRAS
                    .iter()
                    .find(|(k, _)| *k == p.regras || (k.is_empty() && p.regras == "basicas"))
                    .map_or(p.regras.clone(), |(_, n)| n.to_string());
                egui::ComboBox::from_id_salt(("regras", a.id.clone()))
                    .selected_text(atual)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (k, n) in REGRAS {
                            ui.selectable_value(&mut p.regras, k.to_string(), n);
                        }
                    });
                ui.end_row();
            }
            if f == Ferramenta::Msa {
                ui.label("Tolerância (LSE − LIE)");
                ui.horizontal(|ui| opcional(ui, "usar", &mut p.tolerancia));
                ui.end_row();
            }
            if f == Ferramenta::Pareto {
                ui.label("Outros abaixo de");
                ui.add(
                    egui::DragValue::new(&mut p.outros_abaixo)
                        .range(0.0..=50.0)
                        .speed(0.5)
                        .suffix(" %"),
                );
                ui.end_row();
            }
            if f == Ferramenta::Fmea {
                ui.label("Severidade crítica a partir de");
                ui.add(egui::DragValue::new(&mut p.severidade_critica).range(1..=10));
                ui.end_row();
            }
        });
}

/// A largura de uma coluna da grade: estreita quando é de números.
fn largura_coluna(t: &Tabela, c: usize) -> f32 {
    let numerica = t.linhas.iter().all(|l| {
        l.get(c)
            .is_none_or(|x| x.trim().is_empty() || utsim_estudo::tabela::numero(x).is_some())
    });
    if numerica { 92.0 } else { 130.0 }
}

/// A tabela como grade editável: renomear e remover colunas, editar células, linhas.
fn tabela_ui(ui: &mut egui::Ui, t: &mut Tabela, id: &str) {
    let larguras: Vec<f32> = (0..t.colunas.len()).map(|c| largura_coluna(t, c)).collect();
    let mut remover_coluna: Option<usize> = None;
    let mut remover_linha: Option<usize> = None;
    ui.horizontal(|ui| {
        ui.weak(format!(
            "{} {} · {} colunas",
            t.linhas.len(),
            if t.linhas.len() == 1 {
                "linha"
            } else {
                "linhas"
            },
            t.colunas.len()
        ));
        if ui.small_button("+ linha").clicked() {
            t.adicionar_linha();
        }
        if ui.small_button("+ coluna").clicked() {
            let n = t.colunas.len() + 1;
            t.adicionar_coluna(&format!("Coluna {n}"));
        }
    });
    let alto = (t.linhas.len() as f32 + 1.0) * 24.0 + 8.0;
    egui::ScrollArea::both()
        .id_salt(("grade", id.to_string()))
        .max_height(alto.min(300.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            ui.horizontal(|ui| {
                ui.add_sized([28.0, 20.0], egui::Label::new(""));
                for (c, nome) in t.colunas.iter_mut().enumerate() {
                    ui.add(
                        egui::TextEdit::singleline(nome)
                            .desired_width(larguras[c] - 18.0)
                            .font(egui::TextStyle::Body)
                            .text_color(AZUL),
                    );
                    if ui
                        .small_button("×")
                        .on_hover_text("Remover a coluna")
                        .clicked()
                    {
                        remover_coluna = Some(c);
                    }
                }
            });
            for (r, linha) in t.linhas.iter_mut().enumerate() {
                if linha.len() < larguras.len() {
                    linha.resize(larguras.len(), String::new());
                }
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [28.0, 20.0],
                        egui::Label::new(RichText::new((r + 1).to_string()).small().weak()),
                    );
                    for (c, cel) in linha.iter_mut().enumerate().take(larguras.len()) {
                        ui.add(egui::TextEdit::singleline(cel).desired_width(larguras[c] + 3.0));
                    }
                    if ui
                        .small_button("×")
                        .on_hover_text("Remover a linha")
                        .clicked()
                    {
                        remover_linha = Some(r);
                    }
                });
            }
        });
    if let Some(c) = remover_coluna {
        t.remover_coluna(c);
    }
    if let Some(r) = remover_linha {
        t.remover_linha(r);
    }
}

fn ishikawa_ui(ui: &mut egui::Ui, a: &mut Aplicacao, problema: &str) {
    ui.horizontal(|ui| {
        ui.label("Efeito");
        ui.add(
            egui::TextEdit::singleline(&mut a.efeito)
                .hint_text(if problema.is_empty() {
                    "o efeito"
                } else {
                    problema
                })
                .desired_width(f32::INFINITY),
        );
    });
    let mut remover_cat: Option<usize> = None;
    for (k, c) in a.categorias.iter_mut().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut c.nome)
                        .desired_width(180.0)
                        .text_color(AZUL),
                );
                if ui.small_button("+ causa").clicked() {
                    c.causas.push(String::new());
                }
                if ui.small_button("Remover categoria").clicked() {
                    remover_cat = Some(k);
                }
            });
            let mut remover: Option<usize> = None;
            for (j, causa) in c.causas.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.add(
                        egui::TextEdit::singleline(causa)
                            .hint_text("causa")
                            .desired_width((ui.available_width() - 30.0).max(80.0)),
                    );
                    if ui.small_button("×").clicked() {
                        remover = Some(j);
                    }
                });
            }
            if let Some(j) = remover {
                c.causas.remove(j);
            }
        });
    }
    if let Some(k) = remover_cat {
        a.categorias.remove(k);
    }
    if ui.button("+ categoria").clicked() {
        a.categorias.push(Categoria {
            nome: "Nova categoria".into(),
            causas: Vec::new(),
        });
    }
}

fn porques_ui(ui: &mut egui::Ui, a: &mut Aplicacao, problema: &str) {
    ui.horizontal(|ui| {
        ui.label("Problema");
        ui.add(
            egui::TextEdit::singleline(&mut a.efeito)
                .hint_text(if problema.is_empty() {
                    "o problema"
                } else {
                    problema
                })
                .desired_width(f32::INFINITY),
        );
    });
    let mut remover_cadeia: Option<usize> = None;
    for (k, c) in a.cadeias.iter_mut().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.strong(format!("Cadeia {}", k + 1));
                if ui.small_button("+ por quê").clicked() {
                    c.porques.push(String::new());
                }
                if ui.small_button("Remover cadeia").clicked() {
                    remover_cadeia = Some(k);
                }
            });
            let mut remover: Option<usize> = None;
            for (j, p) in c.porques.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Por quê {}", j + 1));
                    ui.add(
                        egui::TextEdit::singleline(p)
                            .desired_width((ui.available_width() - 30.0).max(80.0)),
                    );
                    if ui.small_button("×").clicked() {
                        remover = Some(j);
                    }
                });
            }
            if let Some(j) = remover {
                c.porques.remove(j);
            }
            ui.horizontal(|ui| {
                ui.label("Ação");
                ui.add(
                    egui::TextEdit::singleline(&mut c.acao)
                        .hint_text("a ação sobre a causa raiz")
                        .desired_width(f32::INFINITY),
                );
            });
        });
    }
    if let Some(k) = remover_cadeia {
        a.cadeias.remove(k);
    }
    if ui.button("+ cadeia").clicked() {
        a.cadeias.push(Cadeia::default());
    }
}

fn a3_ui(ui: &mut egui::Ui, a: &mut Aplicacao) {
    for s in &mut a.secoes {
        ui.label(RichText::new(&s.titulo).strong().color(AZUL));
        ui.add(
            egui::TextEdit::multiline(&mut s.texto)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
    }
}

// ---- resultado -------------------------------------------------------------------

fn cor_do_tom(t: Tone) -> Color32 {
    match t {
        Tone::Info => AZUL,
        Tone::Ok => VERDE,
        Tone::Warn => AMBAR,
        Tone::Danger => VERMELHO,
    }
}

fn caixa(ui: &mut egui::Ui, cor: Color32, titulo: &str, texto: &str) {
    egui::Frame::new()
        .fill(cor.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0, cor.gamma_multiply(0.8)))
        .corner_radius(4)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(titulo).strong().color(cor));
            ui.label(texto);
        });
}

/// O resultado de uma aplicação: indicadores, achados, gráficos, Ishikawa, textos,
/// tabelas e avisos, na ordem do relatório.
pub fn mostrar_resultado(ui: &mut egui::Ui, r: &Resultado, id: &str) {
    if !r.indicadores.is_empty() {
        // Cartões de largura igual, quantos couberem por linha (o quadro não sabe a
        // largura antes de desenhar, então a quebra é feita aqui).
        let espaco = ui.spacing().item_spacing.x;
        let total = ui.available_width();
        let por_linha =
            (((total + espaco) / (130.0 + espaco)).floor() as usize).clamp(1, r.indicadores.len());
        let largura = (total - espaco * (por_linha - 1) as f32) / por_linha as f32;
        for linha in r.indicadores.chunks(por_linha) {
            ui.horizontal_top(|ui| {
                for k in linha {
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_width(largura - 22.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&k.label).small().weak());
                                ui.add(
                                    egui::Label::new(RichText::new(&k.value).size(19.0).strong())
                                        .truncate(),
                                );
                                if !k.note.is_empty() {
                                    ui.add(
                                        egui::Label::new(RichText::new(&k.note).small().weak())
                                            .truncate(),
                                    );
                                }
                            });
                        });
                }
            });
        }
        ui.add_space(4.0);
    }
    for a in &r.achados {
        caixa(ui, cor_do_tom(a.tom), &a.titulo, &a.texto);
        ui.add_space(3.0);
    }
    for (i, g) in r.graficos.iter().enumerate() {
        ui.add_space(4.0);
        ui.label(RichText::new(&g.title).strong());
        grafico(ui, &format!("graf_{id}_{i}"), g);
    }
    if let Some((efeito, categorias)) = &r.ishikawa {
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Efeito: {efeito}")).strong());
        egui::Grid::new(("ishikawa", id.to_string()))
            .striped(true)
            .num_columns(2)
            .show(ui, |ui| {
                for (cat, causas) in categorias {
                    ui.label(RichText::new(cat).color(AZUL).strong());
                    if causas.is_empty() {
                        ui.weak("sem causas");
                    } else {
                        ui.vertical(|ui| {
                            for c in causas {
                                ui.label(format!("• {c}"));
                            }
                        });
                    }
                    ui.end_row();
                }
            });
    }
    for (titulo, linhas) in &r.textos {
        ui.add_space(4.0);
        ui.label(RichText::new(titulo).strong());
        for l in linhas {
            ui.label(format!("• {l}"));
        }
    }
    for (k, q) in r.quadros.iter().enumerate() {
        ui.add_space(6.0);
        ui.label(RichText::new(&q.nome).strong());
        let grade = |ui: &mut egui::Ui| {
            egui::Grid::new(("quadro", id.to_string(), k))
                .striped(true)
                .num_columns(q.cabecalho.len())
                .spacing(vec2(12.0, 3.0))
                .show(ui, |ui| {
                    for c in &q.cabecalho {
                        ui.label(RichText::new(c).strong().color(AZUL));
                    }
                    ui.end_row();
                    for l in &q.linhas {
                        for c in l {
                            ui.label(c);
                        }
                        ui.end_row();
                    }
                });
        };
        egui::ScrollArea::both()
            .id_salt(("quadro_rolagem", id.to_string(), k))
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, grade);
    }
    if !r.avisos.is_empty() {
        ui.add_space(4.0);
        caixa(ui, AMBAR, "Avisos", &r.avisos.join(" "));
    }
}

fn cor_da_linha(nome: &str) -> Color32 {
    match nome {
        "LSC" | "LIC" | "LIE" | "LSE" => VERMELHO,
        "LC" | "Alvo" => VERDE,
        _ => CINZA,
    }
}

/// Os rótulos do eixo x nas posições inteiras, pulando alguns quando são muitos.
fn rotulos_x(
    labels: Vec<String>,
) -> impl Fn(egui_plot::GridMark, &std::ops::RangeInclusive<f64>) -> String {
    let passo = (labels.len() / 12).max(1);
    move |m, _| {
        let k = m.value.round();
        if (m.value - k).abs() < 1e-6 && k >= 0.0 && (k as usize) < labels.len() {
            let k = k as usize;
            if k.is_multiple_of(passo) {
                let l = &labels[k];
                if l.chars().count() > 14 {
                    format!("{}…", l.chars().take(13).collect::<String>())
                } else {
                    l.clone()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }
}

fn plot_base(id: &str) -> Plot<'static> {
    Plot::new(id.to_string())
        .height(220.0)
        .allow_scroll(false)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_boxed_zoom(false)
        .show_crosshair(false)
}

/// As entradas da legenda: (nome, cor, tracejada), na ordem do desenho.
fn entradas_da_legenda(g: &Chart) -> Vec<(String, Color32, bool)> {
    let mut v: Vec<(String, Color32, bool)> = Vec::new();
    let linhas_ref = |v: &mut Vec<(String, Color32, bool)>| {
        for (n, _) in &g.hlines {
            v.push((n.clone(), cor_da_linha(n), true));
        }
    };
    match g.drawn_kind() {
        ChartKind::Bars | ChartKind::Auto => {
            if g.series.len() > 1 {
                for (j, (n, _)) in g.series.iter().enumerate() {
                    v.push((n.clone(), PALETA[j % PALETA.len()], false));
                }
            }
            linhas_ref(&mut v);
        }
        ChartKind::Lines => {
            for (j, (n, _)) in g.series.iter().enumerate() {
                let limite = matches!(n.as_str(), "LSC" | "LIC");
                let cor = if limite {
                    VERMELHO
                } else {
                    PALETA[j % PALETA.len()]
                };
                v.push((n.clone(), cor, limite));
            }
            linhas_ref(&mut v);
            if !g.marks.is_empty() {
                v.push(("sinais".into(), VERMELHO, false));
            }
        }
        ChartKind::Pareto => {
            if let Some((n, _)) = g.series.first() {
                v.push((n.clone(), AZUL, false));
            }
            v.push(("% acumulado (eixo da direita)".into(), AMBAR, false));
            v.push(("80 %".into(), CINZA, true));
        }
        ChartKind::Histogram => linhas_ref(&mut v),
        ChartKind::Scatter => {
            v.push(("pontos".into(), AZUL, false));
            v.push(("reta de mínimos quadrados".into(), AMBAR, false));
        }
    }
    v.retain(|(n, _, _)| !n.is_empty());
    v
}

/// A legenda acima do gráfico, para não cobrir os dados.
fn legenda(ui: &mut egui::Ui, entradas: &[(String, Color32, bool)]) {
    if entradas.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (n, cor, tracejada) in entradas {
            let (r, p) = ui.allocate_painter(vec2(18.0, 12.0), egui::Sense::hover());
            let y = r.rect.center().y;
            if *tracejada {
                for k in 0..3 {
                    let x = r.rect.left() + k as f32 * 6.5;
                    p.line_segment(
                        [egui::pos2(x, y), egui::pos2(x + 4.0, y)],
                        Stroke::new(2.0, *cor),
                    );
                }
            } else {
                p.rect_filled(r.rect.shrink2(vec2(2.0, 2.0)), 2.0, *cor);
            }
            ui.label(RichText::new(n).small());
            ui.add_space(8.0);
        }
    });
}

/// Um gráfico do resultado com o egui_plot.
fn grafico(ui: &mut egui::Ui, id: &str, g: &Chart) {
    legenda(ui, &entradas_da_legenda(g));
    match g.drawn_kind() {
        ChartKind::Bars | ChartKind::Auto => barras(ui, id, g),
        ChartKind::Lines => linhas(ui, id, g),
        ChartKind::Pareto => pareto(ui, id, g),
        ChartKind::Histogram => histograma(ui, id, g),
        ChartKind::Scatter => dispersao(ui, id, g),
    }
}

fn barras(ui: &mut egui::Ui, id: &str, g: &Chart) {
    let m = g.series.len().max(1);
    let w = 0.8 / m as f64;
    let n = g.labels.len();
    let charts: Vec<BarChart> = g
        .series
        .iter()
        .enumerate()
        .map(|(j, (nome, vs))| {
            let cor = PALETA[j % PALETA.len()];
            let bars = vs
                .iter()
                .enumerate()
                .take(n.max(vs.len()))
                .map(|(i, v)| {
                    Bar::new(i as f64 - 0.4 + w * (j as f64 + 0.5), *v)
                        .width(w * 0.95)
                        .fill(cor)
                })
                .collect();
            BarChart::new(nome.clone(), bars).color(cor)
        })
        .collect();
    let hl = g.hlines.clone();
    plot_base(id)
        .include_y(0.0)
        .x_axis_formatter(rotulos_x(g.labels.clone()))
        .show(ui, move |p| {
            for c in charts {
                p.bar_chart(c);
            }
            for (nome, y) in hl {
                p.hline(
                    HLine::new(nome.clone(), y)
                        .color(cor_da_linha(&nome))
                        .style(LineStyle::dashed_dense()),
                );
            }
        });
}

fn linhas(ui: &mut egui::Ui, id: &str, g: &Chart) {
    let series: Vec<(String, Vec<[f64; 2]>)> = g
        .series
        .iter()
        .map(|(n, vs)| {
            (
                n.clone(),
                vs.iter()
                    .enumerate()
                    .filter(|(_, v)| v.is_finite())
                    .map(|(i, v)| [i as f64, *v])
                    .collect(),
            )
        })
        .collect();
    let marcas: Vec<[f64; 2]> = g
        .series
        .first()
        .map(|(_, vs)| {
            g.marks
                .iter()
                .filter_map(|&i| vs.get(i).filter(|v| v.is_finite()).map(|v| [i as f64, *v]))
                .collect()
        })
        .unwrap_or_default();
    let hl = g.hlines.clone();
    plot_base(id)
        .x_axis_formatter(rotulos_x(g.labels.clone()))
        .show(ui, move |p| {
            for (j, (nome, pts)) in series.into_iter().enumerate() {
                let limite = matches!(nome.as_str(), "LSC" | "LIC");
                let cor = if limite {
                    VERMELHO
                } else {
                    PALETA[j % PALETA.len()]
                };
                let mut l = Line::new(nome, PlotPoints::new(pts.clone())).color(cor);
                if limite {
                    l = l.style(LineStyle::dashed_dense());
                }
                p.line(l);
                if j == 0 && !limite {
                    p.points(Points::new("", PlotPoints::new(pts)).radius(2.0).color(cor));
                }
            }
            for (nome, y) in hl {
                p.hline(
                    HLine::new(nome.clone(), y)
                        .color(cor_da_linha(&nome))
                        .style(LineStyle::dashed_dense()),
                );
            }
            if !marcas.is_empty() {
                p.points(
                    Points::new("sinais", PlotPoints::new(marcas))
                        .radius(5.0)
                        .color(VERMELHO),
                );
            }
        });
}

fn pareto(ui: &mut egui::Ui, id: &str, g: &Chart) {
    let Some((nome, vs)) = g.series.first() else {
        return;
    };
    let total: f64 = vs.iter().sum();
    let maior = vs
        .iter()
        .cloned()
        .fold(0.0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let bars: Vec<Bar> = vs
        .iter()
        .enumerate()
        .map(|(i, v)| Bar::new(i as f64, *v).width(0.7).fill(AZUL))
        .collect();
    let mut acc = 0.0;
    let acumulado: Vec<[f64; 2]> = vs
        .iter()
        .enumerate()
        .map(|(i, v)| {
            acc += v;
            [i as f64, acc / total.max(f64::MIN_POSITIVE) * maior]
        })
        .collect();
    let nome = nome.clone();
    plot_base(id)
        .include_y(0.0)
        .include_y(maior * 1.05)
        .x_axis_formatter(rotulos_x(g.labels.clone()))
        .custom_y_axes(vec![
            AxisHints::new_y(),
            AxisHints::new_y()
                .placement(HPlacement::Right)
                .label("acumulado")
                .formatter(move |m, _| format!("{:.0} %", m.value / maior * 100.0)),
        ])
        .show(ui, move |p| {
            p.bar_chart(BarChart::new(nome, bars).color(AZUL));
            p.line(
                Line::new("% acumulado", PlotPoints::new(acumulado.clone()))
                    .color(AMBAR)
                    .width(2.0),
            );
            p.points(
                Points::new("", PlotPoints::new(acumulado))
                    .radius(3.0)
                    .color(AMBAR),
            );
            p.hline(
                HLine::new("80 %", 0.8 * maior)
                    .color(CINZA)
                    .style(LineStyle::dashed_dense()),
            );
        });
}

fn histograma(ui: &mut egui::Ui, id: &str, g: &Chart) {
    let xs: Vec<f64> = g
        .series
        .first()
        .map(|(_, v)| v.iter().copied().filter(|x| x.is_finite()).collect())
        .unwrap_or_default();
    if xs.is_empty() {
        return;
    }
    let (lo, hi) = xs
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| {
            (a.min(x), b.max(x))
        });
    let k = ((xs.len() as f64).log2().ceil() as usize + 1).max(1);
    let largura = if hi > lo { (hi - lo) / k as f64 } else { 1.0 };
    let mut contagem = vec![0.0; k];
    for x in &xs {
        let i = (((x - lo) / largura) as usize).min(k - 1);
        contagem[i] += 1.0;
    }
    let bars: Vec<Bar> = contagem
        .iter()
        .enumerate()
        .map(|(i, c)| {
            Bar::new(lo + largura * (i as f64 + 0.5), *c)
                .width(largura * 0.97)
                .fill(AZUL)
        })
        .collect();
    let nome = g.series.first().map(|(n, _)| n.clone()).unwrap_or_default();
    let hl = g.hlines.clone();
    plot_base(id).include_y(0.0).show(ui, move |p| {
        p.bar_chart(BarChart::new(nome, bars).color(AZUL));
        for (n, x) in hl {
            p.vline(
                VLine::new(n.clone(), x)
                    .color(cor_da_linha(&n))
                    .width(2.0)
                    .style(LineStyle::dashed_dense()),
            );
        }
    });
}

fn dispersao(ui: &mut egui::Ui, id: &str, g: &Chart) {
    let (Some((nx, xs)), Some((ny, ys))) = (g.series.first(), g.series.get(1)) else {
        return;
    };
    let pts: Vec<[f64; 2]> = xs
        .iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| [*x, *y])
        .collect();
    let n = pts.len() as f64;
    let reta = if n >= 2.0 {
        let mx = pts.iter().map(|p| p[0]).sum::<f64>() / n;
        let my = pts.iter().map(|p| p[1]).sum::<f64>() / n;
        let sxx: f64 = pts.iter().map(|p| (p[0] - mx).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|p| (p[0] - mx) * (p[1] - my)).sum();
        (sxx > 0.0).then(|| {
            let b = sxy / sxx;
            let (lo, hi) = pts
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), p| {
                    (a.min(p[0]), c.max(p[0]))
                });
            vec![[lo, my + b * (lo - mx)], [hi, my + b * (hi - mx)]]
        })
    } else {
        None
    };
    let rotulo = format!("{ny} × {nx}");
    let (nx, ny) = (nx.clone(), ny.clone());
    plot_base(id)
        .x_axis_label(nx)
        .y_axis_label(ny)
        .show(ui, move |p| {
            p.points(
                Points::new(rotulo, PlotPoints::new(pts))
                    .radius(3.5)
                    .color(AZUL),
            );
            if let Some(r) = reta {
                p.line(
                    Line::new("mínimos quadrados", PlotPoints::new(r))
                        .color(AMBAR)
                        .width(2.0),
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn sessao(dir: &Path) -> Sessao {
        let mut s = Sessao::nova(None);
        assert!(s.criar(&dir.join("unidade"), "Fábrica", "Ana"));
        s
    }

    fn tela() -> TelaFerramentas {
        TelaFerramentas::nova(App::Qualidade, Identidade::default(), None)
    }

    #[test]
    fn study_is_saved_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = sessao(dir.path());
        let mut t = tela();
        let id = t
            .criar(
                &mut s,
                &NovoEstudo {
                    titulo: "Rebarba".into(),
                    problema: "Rebarba na tampa".into(),
                    modelo: "modelos/linha.ron".into(),
                    objeto: "torno".into(),
                    ..NovoEstudo::default()
                },
            )
            .unwrap();
        assert_eq!(t.selecionado, id);
        // An edit is kept in the working copy and saved as a whole.
        let e = t.estudo.as_mut().unwrap();
        e.aplicacoes.push(Aplicacao::nova("a1", Ferramenta::Pareto));
        e.conclusao = "Feito.".into();
        t.marcar_sujo(0.0);
        t.gravar_pendente(&mut s);
        assert!(!t.sujo);
        let mut outra = tela();
        outra.sincronizar(&s);
        assert_eq!(outra.estudos.len(), 1);
        let lido = &outra.estudos[0];
        assert_eq!(lido, t.estudo.as_ref().unwrap());
        assert_eq!(lido.assunto.problema, "Rebarba na tampa");
        assert_eq!(lido.criado.len(), 10);
        // The unit read from scratch sees the same study.
        let mut s2 = Sessao::nova(None);
        assert!(s2.abrir(&dir.path().join("unidade"), "Bruno"));
        assert_eq!(
            estudos_do_no(&s2, "C:/x/modelos/linha.ron", "torno"),
            vec![(id.clone(), "Rebarba".to_string())]
        );
    }

    #[test]
    fn opening_a_node_selects_its_study_or_prepares_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = sessao(dir.path());
        let mut t = tela();
        let mut e = utsim_estudo::exemplo::estudo_demonstracao();
        e.id = "demo".into();
        s.unidade
            .as_mut()
            .unwrap()
            .gravar(ESTUDOS, &e.id, &e)
            .unwrap();
        s.mudou();
        t.abrir_no("modelos/inspecao_e_retrabalho.ron", "torno");
        t.resolver_pedido(&mut s);
        assert_eq!(t.selecionado, "demo");
        assert!(t.novo.is_none());
        assert_eq!(t.aplicacao, "a1");
        assert_eq!(
            t.estudos_do_no(&s, "modelos/inspecao_e_retrabalho.ron", "torno")
                .len(),
            1
        );
        t.abrir_no("modelos/inspecao_e_retrabalho.ron", "prensa");
        t.resolver_pedido(&mut s);
        let n = t.novo.clone().unwrap();
        assert_eq!(n.objeto, "prensa");
        assert_eq!(n.modelo, "modelos/inspecao_e_retrabalho.ron");
        assert_eq!(
            t.filtro_no,
            Some(("modelos/inspecao_e_retrabalho.ron".into(), "prensa".into()))
        );
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Furo fora de posição!"), "furo_fora_de_posicao");
        assert_eq!(slug("  "), "estudo");
    }

    #[test]
    fn reports_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = sessao(dir.path());
        let mut t = tela();
        let e = utsim_estudo::exemplo::estudo_demonstracao();
        let p = t.gravar_relatorio(&mut s, &e, Some("a3")).unwrap();
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|x| x.is_file()));
        t.desmarcadas = vec!["a1".into()];
        let p = t.gravar_relatorio(&mut s, &e, None).unwrap();
        assert!(p[1].to_string_lossy().ends_with("_estudo.html"));
    }
}
