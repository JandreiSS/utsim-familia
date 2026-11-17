//! The drawn statement of a mission: a 3D picture of the process (rendered beforehand in
//! Blender with the elements of the simulator) and cards pinned to its objects, with the
//! numbers of the problem (`docs/plano_ensino_e_jogo.md`, "Enunciados desenhados").
//!
//! The mission declares the scene in its `cena` block: the picture, how the model becomes
//! the scene ([`Cena::planta`], read by `tools/blender/enunciado.py`) and the cards. The
//! picture comes with its anchors (`<picture>.ancoras.json`): the point of the picture over
//! each object (`nome`) and at its foot (`nome@pe`). The app draws the cards over the
//! anchors with live values ([`conteudo`]): the loads follow the tasks the student drags,
//! the measures show the result after the run.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_model::{Model, ObjectKind};

use crate::missao::{Decisoes, Missao, Objetivo, Termo};

fn um() -> f64 {
    1.0
}

fn segundos() -> String {
    "s".into()
}

/// The scene of a mission.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cena {
    /// The picture, relative to the mission file (`mix.cena.png`); its anchors are in
    /// `<picture without .png>.ancoras.json`.
    pub imagem: String,
    /// Factor on the distances of the model in the scene (0.5 brings a line of stations
    /// 6 m apart to 3 m), so the picture is not mostly floor.
    #[serde(default = "um")]
    pub escala: f64,
    /// Time unit of the station loads and of the takt on the cards.
    #[serde(default = "segundos")]
    pub unidade: String,
    /// The 3D element of an object, when not the default of its kind (`"cnc"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub elementos: BTreeMap<String, String>,
    /// Objects of the model left out of the picture.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ocultar: Vec<String>,
    /// Stations drawn without an operator.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sem_pessoa: Vec<String>,
    /// The person of an object, `"homem"` or `"mulher"`, when the mission names its people
    /// (Ana, Bruno); the others take turns.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pessoas: BTreeMap<String, String>,
    /// Products drawn on objects (waiting in a queue, on a pallet).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pecas: Vec<PecaCena>,
    #[serde(default)]
    pub cartoes: Vec<Cartao>,
}

/// A product drawn on an object of the scene.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PecaCena {
    /// The object it stands on.
    pub no: String,
    /// The 3D element (`"ventilador_mesa"`, `"caixa"`).
    pub elemento: String,
    /// Offset from the object, in metres of the scene.
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
    /// Size factor (products are small next to the stations).
    #[serde(default = "um")]
    pub escala: f64,
    /// Turn around the vertical axis, in degrees.
    #[serde(default)]
    pub giro: f64,
    /// Colour of the whole product, `"#rrggbb"` (urgent orders in red, one model per
    /// colour); absent: its own materials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cor: Option<String>,
}

/// A card pinned to an object of the picture.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cartao {
    /// The anchor: an object (`"est1"`, the point over it) or its foot (`"fila4@pe"`).
    pub no: String,
    pub tipo: TipoCartao,
}

impl Cartao {
    /// Whether the card hangs under its object (anchor at the foot, `nome@pe`).
    pub fn abaixo(&self) -> bool {
        self.no.ends_with("@pe")
    }
}

/// What a card shows.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TipoCartao {
    /// A station of the line balancing: its tasks and its load (one per model of a mix),
    /// red over the takt.
    Estacao(u32),
    /// The current value of a lever (the option of a choice).
    Alavanca(String),
    /// A measure: its goal before the run, the simulated value after.
    Metrica(String),
    /// The question of a prediction, where it applies.
    Previsao(String),
    /// A number computed from the decisions (pieces terms, `decisao.<id>`, `custo`,
    /// `const`), live as the student decides.
    Valor {
        rotulo: String,
        valor: Vec<Termo>,
        #[serde(default)]
        unidade: String,
        #[serde(default)]
        casas: u32,
    },
    /// A fixed text of the mission.
    Texto(String),
    /// The name of a machine of a layout; its card follows the machine to the cell the
    /// student puts it in (the anchor of the card is the machine id).
    Maquina(String),
}

/// The anchors of a picture.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Ancoras {
    pub largura: f64,
    pub altura: f64,
    pub nos: BTreeMap<String, [f64; 2]>,
}

impl Ancoras {
    pub fn parse(bytes: &[u8]) -> Result<Ancoras, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("âncoras: {e}"))
    }
}

/// The scene as Blender builds it: every object with its 3D element, and the products.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Planta {
    pub objetos: Vec<ObjetoPlanta>,
    pub pecas: Vec<PecaPlanta>,
    pub largura: u32,
    pub altura: u32,
    /// The grid of a layout mission: the cells are drawn on the floor (anchors `celula<c>`)
    /// and each machine is rendered apart, on a transparent picture
    /// (`<picture>.<machine>.png`), at the reference cell; the app moves it to the cell the
    /// student picks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<GradePlanta>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GradePlanta {
    /// Side of a cell, in metres of the scene.
    pub lado: f64,
    /// Every cell: its anchor name (`celula<c>`) and its centre.
    pub celulas: Vec<CelulaPlanta>,
    /// The cell where the machines are rendered.
    pub referencia: String,
    /// The machines, each rendered alone at the reference cell.
    pub maquinas: Vec<ObjetoPlanta>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CelulaPlanta {
    pub nome: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ObjetoPlanta {
    pub nome: String,
    pub elemento: String,
    pub x: f64,
    pub y: f64,
    pub giro: f64,
    /// Clip phases of the element (`{"ciclo": 0.5}`).
    pub fases: BTreeMap<String, f64>,
    /// `"homem"` or `"mulher"` at the operator point of the element, or none.
    pub pessoa: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PecaPlanta {
    pub elemento: String,
    pub x: f64,
    pub y: f64,
    pub giro: f64,
    pub escala: f64,
    pub cor: Option<String>,
}

/// The default 3D element of an object, by its kind (the mission picks another in
/// `elementos`). `None`: not drawn (logical objects, like a dispatcher).
pub fn elemento_padrao(o: &utsim_model::ObjectDef) -> Option<String> {
    let e = match &o.kind {
        ObjectKind::Source { .. } | ObjectKind::Queue { .. } => "espera_chao",
        ObjectKind::Processor { .. }
        | ObjectKind::Combiner { .. }
        | ObjectKind::Separator { .. } => "posto",
        ObjectKind::Sink => "palete",
        ObjectKind::Transporter { agv: true, .. } => "agv",
        ObjectKind::Transporter { .. } if e_pessoa(o) => "pessoa",
        ObjectKind::Transporter { .. } => "empilhadeira",
        _ => return None,
    };
    Some(e.to_string())
}

/// Whether a mobile resource is a person (its library asset says so).
fn e_pessoa(o: &utsim_model::ObjectDef) -> bool {
    matches!(
        o.visual.as_ref().and_then(|v| v.asset.as_deref()),
        Some("pessoa" | "operador" | "pessoa_homem" | "pessoa_mulher")
    )
}

/// The anchor of cell `c` of a layout grid.
pub fn ancora_celula(c: u32) -> String {
    format!("celula{c}")
}

/// The size of the pictures: under the 2048 pixels a side that every graphics card takes.
pub const LARGURA: u32 = 2040;
pub const ALTURA: u32 = 700;

/// Stations drawn for a processor with capacity n (at most), and the distance between
/// them, in metres of the scene.
const MAX_COPIAS: usize = 6;
const PASSO_COPIAS: f64 = 2.4;
/// Distance between mobile resources that start on the same spot.
const PASSO_MOVEIS: f64 = 1.6;

/// The elements that come with an operator at their `ponto_operador`.
const COM_OPERADOR: &[&str] = &["posto", "cnc", "prensa", "injetora", "celula"];

impl Cena {
    /// The scene of `modelo`: the objects not hidden, at their position times `escala`,
    /// each with its element; an operator (man and woman in turn) at each work element,
    /// unless the model has its own people; the products on their objects. A layout
    /// mission (`arranjo`) draws its grid instead of the objects of the model (they are the
    /// routes compiled from the layout), with the machines apart ([`GradePlanta`]).
    pub fn planta(&self, modelo: &Model, arranjo: Option<&crate::pecas::Arranjo>) -> Planta {
        let mut vez = 0;
        let mut sexo = || {
            vez += 1;
            if vez % 2 == 1 { "homem" } else { "mulher" }.to_string()
        };
        if let Some(a) = arranjo {
            let celulas = (0..a.celulas())
                .map(|c| {
                    let [x, y, _] = a.centro(c);
                    CelulaPlanta {
                        nome: ancora_celula(c),
                        x: x * self.escala,
                        y: y * self.escala,
                    }
                })
                .collect();
            let maquinas = a
                .maquinas
                .iter()
                .map(|m| {
                    let elemento = self
                        .elementos
                        .get(&m.id)
                        .cloned()
                        .unwrap_or_else(|| "cnc".into());
                    let trabalho = COM_OPERADOR.contains(&elemento.as_str());
                    ObjetoPlanta {
                        nome: m.id.clone(),
                        pessoa: (trabalho && !self.sem_pessoa.contains(&m.id)).then(&mut sexo),
                        fases: if trabalho {
                            BTreeMap::from([("ciclo".to_string(), 0.5)])
                        } else {
                            BTreeMap::new()
                        },
                        elemento,
                        x: 0.0,
                        y: 0.0,
                        giro: 0.0,
                    }
                })
                .collect();
            return Planta {
                objetos: Vec::new(),
                pecas: Vec::new(),
                largura: LARGURA,
                altura: ALTURA,
                grade: Some(GradePlanta {
                    lado: a.celula * self.escala,
                    celulas,
                    referencia: ancora_celula(0),
                    maquinas,
                }),
            };
        }
        let tem_pessoas = modelo
            .objects
            .iter()
            .any(|o| matches!(o.kind, ObjectKind::Transporter { .. }) && e_pessoa(o));
        let mut objetos: Vec<ObjetoPlanta> = Vec::new();
        for o in &modelo.objects {
            if self.ocultar.contains(&o.name) {
                continue;
            }
            let Some(mut elemento) = self
                .elementos
                .get(&o.name)
                .cloned()
                .or_else(|| elemento_padrao(o))
            else {
                continue;
            };
            if elemento == "pessoa" {
                let s = self.pessoas.get(&o.name).cloned().unwrap_or_else(&mut sexo);
                elemento = format!("pessoa_{s}");
            }
            let trabalho = COM_OPERADOR.contains(&elemento.as_str());
            let fases = if trabalho {
                BTreeMap::from([("ciclo".to_string(), 0.5)])
            } else {
                BTreeMap::new()
            };
            // A processor with capacity n is n stations side by side (across the flow);
            // the one in the middle keeps the name (the anchor of the cards), the others are
            // `nome#2`, `nome#3`...
            let copias = match &o.kind {
                ObjectKind::Processor { capacity, .. } => (*capacity).clamp(1, MAX_COPIAS),
                _ => 1,
            };
            let meio = (copias - 1) / 2;
            for k in 0..copias {
                let pessoa = (trabalho && !tem_pessoas && !self.sem_pessoa.contains(&o.name))
                    .then(|| self.pessoas.get(&o.name).cloned().unwrap_or_else(&mut sexo));
                let desvio = (k as f64 - (copias - 1) as f64 / 2.0) * PASSO_COPIAS;
                let (mut x, y) = (
                    o.position[0] * self.escala,
                    o.position[1] * self.escala + desvio,
                );
                // Mobile resources start on the same spot (four AGVs at the dock): side by
                // side, so all of them show.
                if matches!(o.kind, ObjectKind::Transporter { .. }) {
                    while objetos
                        .iter()
                        .any(|p| (p.x - x).abs() < 0.5 && (p.y - y).abs() < 0.5)
                    {
                        x += PASSO_MOVEIS;
                    }
                }
                objetos.push(ObjetoPlanta {
                    nome: if k == meio {
                        o.name.clone()
                    } else {
                        format!("{}#{}", o.name, k + 1)
                    },
                    elemento: elemento.clone(),
                    x,
                    y,
                    giro: o.visual.as_ref().and_then(|v| v.yaw).unwrap_or(0.0),
                    fases: fases.clone(),
                    pessoa,
                });
            }
        }
        let pecas = self
            .pecas
            .iter()
            .filter_map(|p| {
                let o = objetos.iter().find(|o| o.nome == p.no)?;
                Some(PecaPlanta {
                    elemento: p.elemento.clone(),
                    x: o.x + p.dx,
                    y: o.y + p.dy,
                    giro: p.giro,
                    escala: p.escala,
                    cor: p.cor.clone(),
                })
            })
            .collect();
        Planta {
            objetos,
            pecas,
            largura: LARGURA,
            altura: ALTURA,
            grade: None,
        }
    }

    /// The picture of a machine of a layout (`oficina.cena.png` → `oficina.cena.serra.png`).
    pub fn imagem_maquina(&self, id: &str) -> String {
        let base = self.imagem.strip_suffix(".png").unwrap_or(&self.imagem);
        format!("{base}.{id}.png")
    }

    /// The path of the anchors of the picture (`mix.cena.png` → `mix.cena.ancoras.json`).
    pub fn caminho_ancoras(&self) -> String {
        let base = self.imagem.strip_suffix(".png").unwrap_or(&self.imagem);
        format!("{base}.ancoras.json")
    }

    /// The anchor of card `c` for the decisions `d`: in a layout, a machine id (with or
    /// without `@pe`) becomes the cell the machine is in.
    pub fn ancora(missao: &Missao, c: &Cartao, d: &Decisoes) -> String {
        let (no, pe) = match c.no.strip_suffix("@pe") {
            Some(n) => (n, "@pe"),
            None => (c.no.as_str(), ""),
        };
        if let Some(a) = &missao.arranjo
            && a.maquinas.iter().any(|m| m.id == no)
        {
            let pos = a.posicoes(&missao.completar(d));
            if let Some(cel) = pos.get(no) {
                return format!("{}{pe}", ancora_celula(*cel));
            }
        }
        c.no.clone()
    }

    /// Problems of the scene against its mission and model (the picture and the anchors
    /// are checked where the package is read).
    pub fn validar(&self, missao: &Missao, modelo: &Model) -> Vec<String> {
        let mut e = Vec::new();
        if !self.imagem.ends_with(".png") {
            e.push(format!("cena: a imagem '{}' precisa ser .png", self.imagem));
        }
        if !(self.escala.is_finite() && self.escala > 0.0) {
            e.push("cena: escala precisa ser positiva".into());
        }
        // In a layout the scene shows the machines and the cells of the grid, not the
        // objects of the model.
        let existe = |n: &str| match &missao.arranjo {
            Some(a) => {
                a.maquinas.iter().any(|m| m.id == n)
                    || n.strip_prefix("celula")
                        .and_then(|c| c.parse::<u32>().ok())
                        .is_some_and(|c| c < a.celulas())
            }
            None => modelo.objects.iter().any(|o| o.name == n),
        };
        for n in self
            .elementos
            .keys()
            .chain(&self.ocultar)
            .chain(&self.sem_pessoa)
            .chain(self.pessoas.keys())
            .chain(self.pecas.iter().map(|p| &p.no))
        {
            if !existe(n) {
                e.push(format!("cena: o objeto '{n}' não está no modelo"));
            }
        }
        for (n, s) in &self.pessoas {
            if s != "homem" && s != "mulher" {
                e.push(format!(
                    "cena: a pessoa de '{n}' é \"homem\" ou \"mulher\", não \"{s}\""
                ));
            }
        }
        for p in &self.pecas {
            let ok = p.cor.as_deref().is_none_or(|c| {
                c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
            });
            if !ok {
                e.push(format!(
                    "cena: cor '{}' da peça: use #rrggbb",
                    p.cor.as_deref().unwrap_or("")
                ));
            }
        }
        for c in &self.cartoes {
            // `nome@pe` is the foot of the object, `nome#k` a copy of a processor.
            let no = c.no.strip_suffix("@pe").unwrap_or(&c.no);
            let no = no.split_once('#').map_or(no, |(n, _)| n);
            if !existe(no) || self.ocultar.iter().any(|x| x == no) {
                e.push(format!(
                    "cena: o cartão de '{}' não tem objeto na imagem",
                    c.no
                ));
            }
            match &c.tipo {
                TipoCartao::Estacao(n) => match &missao.balanceamento {
                    Some(b) if *n >= 1 && *n <= b.estacoes => {}
                    Some(_) => e.push(format!("cena: estação {n} fora do balanceamento")),
                    None => e.push("cena: cartão de estação sem balanceamento".into()),
                },
                TipoCartao::Alavanca(id) => {
                    if missao.alavanca(id).is_none() {
                        e.push(format!("cena: alavanca '{id}' não existe"));
                    }
                }
                TipoCartao::Metrica(nome) => {
                    if !missao.metricas.iter().any(|m| &m.nome == nome) {
                        e.push(format!("cena: métrica '{nome}' não existe"));
                    }
                }
                TipoCartao::Previsao(id) => {
                    if !missao.previsao.iter().any(|p| p.id() == id) {
                        e.push(format!("cena: previsão '{id}' não existe"));
                    }
                }
                TipoCartao::Valor { rotulo, valor, .. } => {
                    for t in valor {
                        let ok = match t.chave.strip_prefix("decisao.") {
                            Some(id) => missao.alavanca(id).is_some(),
                            None => {
                                t.chave == "custo"
                                    || t.chave == "const"
                                    || crate::pecas::e_termo(&t.chave)
                            }
                        };
                        if !ok {
                            e.push(format!(
                                "cena: o cartão \"{rotulo}\" usa '{}': só decisões, custo, const e termos das peças",
                                t.chave
                            ));
                        }
                    }
                }
                TipoCartao::Texto(t) => {
                    if t.trim().is_empty() {
                        e.push("cena: cartão de texto vazio".into());
                    }
                }
                TipoCartao::Maquina(id) => {
                    if !missao
                        .arranjo
                        .as_ref()
                        .is_some_and(|a| a.maquinas.iter().any(|m| &m.id == id))
                    {
                        e.push(format!("cena: máquina '{id}' fora do arranjo"));
                    }
                }
            }
        }
        e
    }
}

/// The load of a station for one model (or the single load without a mix).
#[derive(Clone, Debug, PartialEq)]
pub struct Carga {
    /// The model's position in the mix (its colour), `None` without a mix.
    pub modelo: Option<usize>,
    pub nome: String,
    pub valor: f64,
    /// Over the takt.
    pub passa: bool,
}

/// What a card shows now, for the app to draw (the fixed words come from its texts).
#[derive(Clone, Debug, PartialEq)]
pub enum Conteudo {
    Estacao {
        n: u32,
        tarefas: Vec<String>,
        cargas: Vec<Carga>,
    },
    Alavanca {
        rotulo: String,
        valor: String,
    },
    Metrica {
        rotulo: String,
        unidade: String,
        casas: u32,
        objetivo: Objetivo,
        meta: Option<f64>,
        /// The simulated mean, after the run.
        valor: Option<f64>,
        /// Whether the goal was reached, after the run.
        ok: Option<bool>,
    },
    Previsao {
        pergunta: String,
    },
    Valor {
        rotulo: String,
        valor: f64,
        unidade: String,
        casas: u32,
    },
    Texto(String),
    Maquina {
        nome: String,
        fixa: bool,
    },
}

impl Conteudo {
    /// Whether the card asks for attention (a load over the takt, a goal not reached).
    pub fn alerta(&self) -> bool {
        match self {
            Conteudo::Estacao { cargas, .. } => cargas.iter().any(|c| c.passa),
            Conteudo::Metrica { ok, .. } => *ok == Some(false),
            _ => false,
        }
    }
}

/// The content of card `c` for the decisions `d` and, after a run, the result.
pub fn conteudo(
    missao: &Missao,
    c: &Cartao,
    d: &Decisoes,
    resultado: Option<&crate::nota::Resultado>,
) -> Option<Conteudo> {
    let d = missao.completar(d);
    Some(match &c.tipo {
        TipoCartao::Estacao(n) => {
            let b = missao.balanceamento.as_ref()?;
            let a = b.atribuicao(&d);
            let tarefas = b
                .tarefas
                .iter()
                .filter(|t| a.get(&t.id) == Some(n))
                .map(|t| t.id.clone())
                .collect();
            let i = *n as usize - 1;
            let cargas = if b.mix.is_empty() {
                let v = *b.resumo(&a).cargas.get(i)?;
                vec![Carga {
                    modelo: None,
                    nome: String::new(),
                    valor: v,
                    passa: v > b.takt + 1e-9,
                }]
            } else {
                b.mix
                    .iter()
                    .zip(b.cargas_modelos(&a))
                    .enumerate()
                    .map(|(k, (m, c))| Carga {
                        modelo: Some(k),
                        nome: m.nome.clone(),
                        valor: c[i],
                        passa: c[i] > b.takt + 1e-9,
                    })
                    .collect()
            };
            Conteudo::Estacao {
                n: *n,
                tarefas,
                cargas,
            }
        }
        TipoCartao::Alavanca(id) => {
            let a = missao.alavanca(id)?;
            Conteudo::Alavanca {
                rotulo: a.rotulo().to_string(),
                valor: a.descrever(*d.get(id)?),
            }
        }
        TipoCartao::Metrica(nome) => {
            let m = missao.metricas.iter().find(|m| &m.nome == nome)?;
            let r = resultado.and_then(|r| r.metricas.iter().find(|x| &x.nome == nome));
            Conteudo::Metrica {
                rotulo: m.rotulo.clone(),
                unidade: m.unidade.clone(),
                casas: m.casas,
                objetivo: m.objetivo,
                meta: m.meta,
                valor: r.map(|r| r.aluno.media),
                ok: r.and_then(|r| match r.situacao {
                    crate::nota::Situacao::SemMeta => None,
                    crate::nota::Situacao::NaoAtingida => Some(false),
                    _ => Some(true),
                }),
            }
        }
        TipoCartao::Previsao(id) => Conteudo::Previsao {
            pergunta: missao
                .previsao
                .iter()
                .find(|p| p.id() == id)?
                .pergunta()
                .to_string(),
        },
        TipoCartao::Valor {
            rotulo,
            valor,
            unidade,
            casas,
        } => {
            let mut v = 0.0;
            for t in valor {
                let x = match t.chave.as_str() {
                    "const" => 1.0,
                    "custo" => missao.custo(&d),
                    k => match k.strip_prefix("decisao.") {
                        Some(id) => {
                            let a = missao.alavanca(id)?;
                            a.numero(*d.get(id)?)
                        }
                        None => crate::pecas::termo(missao, k, &d)?,
                    },
                };
                v += t.fator * x;
            }
            Conteudo::Valor {
                rotulo: rotulo.clone(),
                valor: v,
                unidade: unidade.clone(),
                casas: *casas,
            }
        }
        TipoCartao::Texto(t) => Conteudo::Texto(t.clone()),
        TipoCartao::Maquina(id) => {
            let m = missao
                .arranjo
                .as_ref()?
                .maquinas
                .iter()
                .find(|m| &m.id == id)?;
            Conteudo::Maquina {
                nome: m.nome.clone(),
                fixa: m.fixa,
            }
        }
    })
}
