//! Time and method study of man-machine work (`utsim tempos`): the micro-analysis of the
//! work cycle of an operator and the machines it tends, from the trace of one replication.
//!
//! 1. **Elements.** The states of the trace become the elements of the time study: for the
//!    operator, "carga/descarga maq1" (working at a machine during its setup, i.e. loading
//!    and unloading), "operar maq1" (working at it while it processes), "andar até maq2",
//!    "pegar peça"/"entregar peça" (a carrier loading and unloading items), "ocioso"; for a
//!    machine, "carga/descarga (com operador)", "automático" (processing on its own),
//!    "esperando operador", "bloqueada", "ociosa". Which machine the operator is at comes
//!    from the trace: the operator starts working at the instant the machine starts the
//!    phase that needed it.
//! 2. **Cycles.** The cycle of an operator is anchored on the start of its most frequent
//!    work at its most visited machine ([`ciclos`]): one cycle per pass of the rotation.
//!    Irregular cycles (another order of the machines, a breakdown, a pause) are listed
//!    and left out of the statistics. The representative cycle is the median one (or the
//!    one asked for), drawn as a multiple-activity chart with every actor on a common time
//!    axis.
//! 3. **Statistics** per element (mean, sd, min, max, n over the regular cycles) and per
//!    actor (utilisation, idle time and interference per cycle).
//! 4. **Man-machine model** ([`formulas`]): `a`, `b`, `t` measured from the cycles,
//!    `n' = (a + t)/(a + b)`, the arrangements with `⌊n'⌋` and `⌊n'⌋ + 1` machines and, with
//!    hourly costs, the cheapest one.
//! 5. **Standard time**: normal time (observed × rating) plus allowances, per element, per
//!    cycle and per unit. If the operator's profile carries a fatigue `Allowance(p)`, the
//!    simulated operation times already include it (the kernel multiplies them by
//!    `1 + p`): they are divided by `1 + p` first, and `p` is the default allowance.
//! 6. **Comparison** of two studies (before and after a method change), element by element.

pub mod ciclos;
pub mod formulas;
mod saida;

use std::collections::BTreeMap;

use serde::Serialize;
use utsim_core::{Fatigue, State};
use utsim_model::{Model, ObjectKind};

use crate::rastro::{Rastro, dist3};
use crate::text::fold;
use ciclos::{Ciclo, Seg};
pub use formulas::{Abt, Arranjo};
pub use saida::{csv_comparacao, csv_elementos, html, svg, texto};

/// What the study does.
#[derive(Clone, Debug)]
pub struct Opcoes {
    /// Operator or machine to study (absent: every operator; without operators, every
    /// machine).
    pub ator: Option<String>,
    /// Time window (absent: from the warm-up to the end of the run).
    pub janela: Option<(f64, f64)>,
    /// Cycle drawn, numbered from 1 among the cycles found (absent: the median one).
    pub ciclo: Option<usize>,
    /// Performance rating (`1.0` = 100 %).
    pub ritmo: f64,
    /// Allowances, PF&D (`0.15` = 15 %; absent: the person's `Allowance`, else 0).
    pub tolerancia: Option<f64>,
    /// Hourly costs of the operator and of one machine.
    pub custos: Option<(f64, f64)>,
    /// Time unit of the model: `s`, `min` or `h` (for the hourly costs and the labels).
    pub unidade: String,
    /// Machines whose name contains one of these do inspection.
    pub palavras_inspecao: Vec<String>,
}

impl Default for Opcoes {
    fn default() -> Self {
        Opcoes {
            ator: None,
            janela: None,
            ciclo: None,
            ritmo: 1.0,
            tolerancia: None,
            custos: None,
            unidade: "s".into(),
            palavras_inspecao: crate::Options::default().inspection_keywords,
        }
    }
}

/// Kind of element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tipo {
    /// Operator and machine together, the machine stopped (setup: load, unload).
    CargaDescarga,
    /// Operator and machine together, the machine running.
    Operacao,
    /// Inspection at a station.
    Inspecao,
    /// Machine on its own.
    Automatico,
    Andar,
    Transportar,
    Pegar,
    Entregar,
    /// Machine waiting for the operator (interference).
    EsperaOperador,
    /// Blocked, waiting for transport, or the operator waiting for something else.
    Bloqueio,
    Ocioso,
    /// Breakdown, maintenance, pause, off shift.
    Parado,
}

impl Tipo {
    pub const ALL: [Tipo; 12] = [
        Tipo::CargaDescarga,
        Tipo::Operacao,
        Tipo::Inspecao,
        Tipo::Automatico,
        Tipo::Andar,
        Tipo::Transportar,
        Tipo::Pegar,
        Tipo::Entregar,
        Tipo::EsperaOperador,
        Tipo::Bloqueio,
        Tipo::Ocioso,
        Tipo::Parado,
    ];

    /// Identifier for CSV and JSON.
    pub fn id(self) -> &'static str {
        match self {
            Tipo::CargaDescarga => "carga_descarga",
            Tipo::Operacao => "operacao",
            Tipo::Inspecao => "inspecao",
            Tipo::Automatico => "automatico",
            Tipo::Andar => "andar",
            Tipo::Transportar => "transportar",
            Tipo::Pegar => "pegar",
            Tipo::Entregar => "entregar",
            Tipo::EsperaOperador => "espera_operador",
            Tipo::Bloqueio => "bloqueio",
            Tipo::Ocioso => "ocioso",
            Tipo::Parado => "parado",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Tipo::CargaDescarga => "carga/descarga (operador e máquina juntos)",
            Tipo::Operacao => "operação assistida (máquina rodando com o operador)",
            Tipo::Inspecao => "inspeção",
            Tipo::Automatico => "automático (máquina sozinha)",
            Tipo::Andar => "andar",
            Tipo::Transportar => "transportar peça",
            Tipo::Pegar => "pegar peça",
            Tipo::Entregar => "entregar peça",
            Tipo::EsperaOperador => "máquina esperando o operador",
            Tipo::Bloqueio => "bloqueio ou espera",
            Tipo::Ocioso => "ocioso",
            Tipo::Parado => "parado (quebra, pausa, fora do turno)",
        }
    }

    pub fn cor(self) -> &'static str {
        match self {
            Tipo::CargaDescarga => "#ca8a04",
            Tipo::Operacao => "#16a34a",
            Tipo::Inspecao => "#9333ea",
            Tipo::Automatico => "#0891b2",
            Tipo::Andar => "#2563eb",
            Tipo::Transportar => "#1d4ed8",
            Tipo::Pegar | Tipo::Entregar => "#db2777",
            Tipo::EsperaOperador => "#c2410c",
            Tipo::Bloqueio => "#ea580c",
            Tipo::Ocioso => "#9ca3af",
            Tipo::Parado => "#7f1d1d",
        }
    }

    /// Work of the operator (or of a machine under it): rated, given allowances.
    pub fn manual(self) -> bool {
        matches!(
            self,
            Tipo::CargaDescarga
                | Tipo::Operacao
                | Tipo::Inspecao
                | Tipo::Andar
                | Tipo::Transportar
                | Tipo::Pegar
                | Tipo::Entregar
        )
    }

    /// Operator time concurrent with a machine (`a`).
    pub fn concorrente(self) -> bool {
        matches!(self, Tipo::CargaDescarga | Tipo::Operacao | Tipo::Inspecao)
    }

    /// Work (not idle, waiting or stopped).
    pub fn trabalho(self) -> bool {
        self.manual() || self == Tipo::Automatico
    }

    /// Times the kernel lengthens by the person's `Allowance` (operation, loading,
    /// unloading; not walking).
    fn com_tolerancia_do_modelo(self) -> bool {
        matches!(
            self,
            Tipo::CargaDescarga | Tipo::Operacao | Tipo::Inspecao | Tipo::Pegar | Tipo::Entregar
        )
    }
}

/// Mean, standard deviation (sample), minimum, maximum and count.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Resumo {
    pub media: f64,
    pub dp: f64,
    pub min: f64,
    pub max: f64,
    pub n: usize,
}

impl Resumo {
    pub fn de(xs: &[f64]) -> Option<Resumo> {
        if xs.is_empty() {
            return None;
        }
        let n = xs.len();
        let media = xs.iter().sum::<f64>() / n as f64;
        let dp = if n > 1 {
            (xs.iter().map(|x| (x - media).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
        } else {
            0.0
        };
        Some(Resumo {
            media,
            dp,
            min: xs.iter().copied().fold(f64::INFINITY, f64::min),
            max: xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            n,
        })
    }
}

/// A cycle found, for the reports.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CicloInfo {
    pub inicio: f64,
    pub fim: f64,
    pub duracao: f64,
    pub regular: bool,
    pub assinatura: String,
}

/// One element of one actor over the regular cycles.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LinhaElemento {
    pub ator: String,
    pub elemento: String,
    pub tipo: Tipo,
    /// Time per cycle (summed when the element happens more than once in a cycle), over the
    /// cycles where it happens (`n` of them).
    pub resumo: Resumo,
    /// Normal and standard time per cycle (work elements only).
    pub normal: Option<f64>,
    pub padrao: Option<f64>,
}

/// One actor over the regular cycles (means per cycle).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LinhaAtor {
    pub ator: String,
    /// "operador" or "máquina".
    pub papel: String,
    pub ocupado: f64,
    pub utilizacao: f64,
    /// Idle, waiting and blocked time.
    pub ocioso: f64,
    /// Machine: waiting for the operator. Operator: idle while one of its machines runs on
    /// its own (waiting for the machine).
    pub interferencia: f64,
}

/// A stretch of the representative cycle, relative to its start.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Barra {
    pub t0: f64,
    pub t1: f64,
    pub elemento: String,
    pub tipo: Tipo,
}

/// One actor of the multiple-activity chart.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Coluna {
    pub ator: String,
    pub barras: Vec<Barra>,
}

/// Standard time of the cycle and of one unit.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TempoPadrao {
    pub ritmo: f64,
    pub tolerancia: f64,
    /// Where the allowance came from.
    pub origem_tolerancia: String,
    /// The person's `Allowance` already inside the simulated times (removed first).
    pub tolerancia_no_modelo: f64,
    pub ciclo_observado: f64,
    pub ciclo_normal: f64,
    pub ciclo_padrao: f64,
    pub unidades_por_ciclo: f64,
    pub por_unidade_observado: Option<f64>,
    pub por_unidade_padrao: Option<f64>,
}

/// Man-machine model of an operator group.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HomemMaquina {
    pub abt: Abt,
    pub n_linha: f64,
    pub maquinas_atuais: usize,
    /// Observed time each machine waits for the operator per cycle (interference).
    pub espera_maquina: f64,
    /// `max(a + t, n·(a + b))` with the current number of machines, to set beside the
    /// observed cycle (the difference is interference and variability).
    pub ciclo_teorico: f64,
    pub arranjos: Vec<Arranjo>,
    pub otimo: Option<Arranjo>,
}

/// The study of one operator and its machines (or one machine alone).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Grupo {
    pub nome: String,
    pub operador: Option<String>,
    pub maquinas: Vec<String>,
    /// Element whose start opens each cycle.
    pub ancora: String,
    pub ciclos: Vec<CicloInfo>,
    /// Index in `ciclos` of the cycle drawn.
    pub representativo: Option<usize>,
    pub tempo_ciclo: Option<Resumo>,
    pub elementos: Vec<LinhaElemento>,
    pub atores: Vec<LinhaAtor>,
    pub homem_maquina: Option<HomemMaquina>,
    pub padrao: Option<TempoPadrao>,
    pub grafico: Vec<Coluna>,
}

impl Grupo {
    pub fn regulares(&self) -> usize {
        self.ciclos.iter().filter(|c| c.regular).count()
    }
}

/// The whole study of one model.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Estudo {
    pub modelo: String,
    pub semente: u64,
    pub replicacao: u32,
    pub janela: (f64, f64),
    pub unidade: String,
    pub grupos: Vec<Grupo>,
}

/// One stretch of an actor, with what the analysis needs beyond [`Seg`].
#[derive(Clone, Debug)]
struct Trecho {
    t0: f64,
    t1: f64,
    nome: String,
    tipo: Tipo,
    /// Machine the operator is at (operator work).
    maquina: Option<usize>,
    /// A machine starting to process (one unit).
    inicio_processo: bool,
}

fn seg_of(t: &Trecho) -> Seg {
    Seg {
        t0: t.t0,
        t1: t.t1,
        elemento: t.nome.clone(),
        trabalho: t.tipo.trabalho(),
        interrupcao: t.tipo == Tipo::Parado,
    }
}

/// Raw `(start, end, state)` stretches of a block over `[0, end]`.
fn brutos(estados: &[(f64, State)], fim: f64) -> Vec<(f64, f64, State)> {
    let mut out = Vec::new();
    let mut cur = State::Idle;
    let mut t = 0.0;
    for &(te, s) in estados {
        if te > fim {
            break;
        }
        if te > t {
            out.push((t, te, cur));
        }
        t = te;
        cur = s;
    }
    if fim > t {
        out.push((t, fim, cur));
    }
    out
}

/// Drops the zero-length stretches and joins consecutive stretches with the same name,
/// unless another state came in between, even for an instant: a machine that goes
/// processing → blocked → idle → processing at one instant starts a new unit.
fn juntar(v: Vec<Trecho>) -> Vec<Trecho> {
    let mut out: Vec<Trecho> = Vec::new();
    let mut ultimo: Option<String> = None;
    for t in v {
        let curto = t.t1 - t.t0 <= 1e-9;
        let mesmo = ultimo.as_deref() == Some(t.nome.as_str());
        if curto {
            ultimo = Some(t.nome);
            continue;
        }
        match out.last_mut() {
            Some(l) if mesmo && l.nome == t.nome && (t.t0 - l.t1).abs() <= 1e-9 => {
                l.t1 = t.t1;
            }
            _ => out.push(t.clone()),
        }
        ultimo = Some(t.nome);
    }
    out
}

fn parado(s: State) -> Option<&'static str> {
    Some(match s {
        State::Down | State::WaitingRepair => "quebra",
        State::Maintenance => "manutenção",
        State::Resting => "pausa (fadiga)",
        State::OffShift => "fora do turno",
        State::Stopped => "parado",
        _ => return None,
    })
}

struct Contexto<'a> {
    model: &'a Model,
    rastro: &'a Rastro,
    opcoes: &'a Opcoes,
    fim: f64,
}

impl Contexto<'_> {
    fn nome(&self, b: usize) -> &str {
        &self.rastro.nomes[b]
    }

    fn e_maquina(&self, b: usize) -> bool {
        matches!(
            self.model.objects[b].kind,
            ObjectKind::Processor { .. }
                | ObjectKind::Combiner { .. }
                | ObjectKind::Separator { .. }
        )
    }

    fn e_inspecao(&self, b: usize) -> bool {
        let n = fold(self.nome(b));
        self.opcoes
            .palavras_inspecao
            .iter()
            .any(|k| !k.is_empty() && n.contains(&fold(k)))
    }

    /// `(setup, process)` need operators.
    fn usa_operador(&self, b: usize) -> (bool, bool) {
        match &self.model.objects[b].kind {
            ObjectKind::Processor {
                operators: Some(op),
                ..
            } => (op.setup, op.process),
            _ => (false, false),
        }
    }

    /// Machines whose operators may be `op` (the team is `op` or a dispatcher with it).
    fn maquinas_de(&self, op: usize) -> Vec<usize> {
        let nome = self.nome(op);
        let membro = |team: &str| {
            team == nome
                || self.model.objects.iter().any(|o| {
                    o.name == team
                        && matches!(&o.kind, ObjectKind::Dispatcher { team, .. } if team.iter().any(|m| m == nome))
                })
        };
        self.model
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                matches!(&o.kind, ObjectKind::Processor { operators: Some(u), .. } if membro(&u.team))
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn trechos_maquina(&self, m: usize) -> Vec<Trecho> {
        let (op_setup, op_process) = self.usa_operador(m);
        let insp = self.e_inspecao(m);
        let v = brutos(&self.rastro.estados[m], self.fim)
            .into_iter()
            .map(|(t0, t1, s)| {
                let (nome, tipo) = match s {
                    State::Setup if op_setup => {
                        ("carga/descarga (com operador)", Tipo::CargaDescarga)
                    }
                    State::Setup => ("preparação (sem operador)", Tipo::Automatico),
                    State::Processing if insp && op_process => {
                        ("inspeção (com operador)", Tipo::Inspecao)
                    }
                    State::Processing if insp => ("inspeção automática", Tipo::Inspecao),
                    State::Processing if op_process => ("operação (com operador)", Tipo::Operacao),
                    State::Processing => ("automático", Tipo::Automatico),
                    State::WaitingOperator => ("esperando operador", Tipo::EsperaOperador),
                    State::Idle | State::Empty => ("ociosa", Tipo::Ocioso),
                    s => match parado(s) {
                        Some(p) => (p, Tipo::Parado),
                        None => ("bloqueada ou esperando transporte", Tipo::Bloqueio),
                    },
                };
                Trecho {
                    t0,
                    t1,
                    nome: nome.to_string(),
                    tipo,
                    maquina: None,
                    inicio_processo: s == State::Processing,
                }
            })
            .collect();
        // `inicio_processo` marks every processing stretch: one per unit, since `juntar`
        // keeps apart two stretches split by an instant in another state.
        juntar(v)
    }

    /// Nearest object (not a mobile resource) to `p`.
    fn mais_proximo(&self, p: [f64; 3]) -> Option<usize> {
        self.model
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                !matches!(
                    o.kind,
                    ObjectKind::Transporter { .. } | ObjectKind::Dispatcher { .. }
                )
            })
            .min_by(|a, b| dist3(a.1.position, p).total_cmp(&dist3(b.1.position, p)))
            .map(|(i, _)| i)
    }

    fn trechos_operador(&self, op: usize) -> Vec<Trecho> {
        let candidatas = self.maquinas_de(op);
        let pernas: Vec<(f64, [f64; 3])> = self
            .rastro
            .pernas
            .iter()
            .filter(|p| p.carrier == op)
            .filter_map(|p| p.path.last().map(|e| (p.t, *e)))
            .collect();
        // The legs and the state changes are in time order: binary searches.
        let onde = |t: f64| -> [f64; 3] {
            let k = pernas.partition_point(|(tp, _)| *tp <= t + 1e-9);
            if k == 0 {
                self.rastro.pos[op]
            } else {
                pernas[k - 1].1
            }
        };
        // Machine that starts a phase needing the operator at `t`.
        let maquina_em = |t: f64| -> Option<(usize, State)> {
            let mut achadas: Vec<(usize, State)> = candidatas
                .iter()
                .filter_map(|&m| {
                    let es = &self.rastro.estados[m];
                    let a = es.partition_point(|(te, _)| *te < t - 1e-9);
                    let b = es.partition_point(|(te, _)| *te <= t + 1e-9);
                    es[a..b]
                        .iter()
                        .rfind(|(_, s)| matches!(s, State::Setup | State::Processing))
                        .map(|(_, s)| (m, *s))
                })
                .collect();
            if achadas.len() > 1 {
                let p = onde(t);
                achadas.sort_by(|a, b| {
                    dist3(self.rastro.pos[a.0], p).total_cmp(&dist3(self.rastro.pos[b.0], p))
                });
            }
            achadas.first().copied()
        };
        let raw = brutos(&self.rastro.estados[op], self.fim);
        let mut v: Vec<Trecho> = Vec::with_capacity(raw.len());
        for (t0, t1, s) in &raw {
            let (t0, t1, s) = (*t0, *t1, *s);
            let (nome, tipo, maquina) = match s {
                State::Utilize => {
                    let (m, fase) = maquina_em(t0)
                        .or_else(|| {
                            self.mais_proximo(onde(t0))
                                .filter(|&m| self.e_maquina(m))
                                .map(|m| (m, State::Setup))
                        })
                        .unzip();
                    let lugar = m.map_or("?".to_string(), |m| self.nome(m).to_string());
                    let (verbo, tipo) = match (m.is_some_and(|m| self.e_inspecao(m)), fase) {
                        (true, _) => ("inspecionar", Tipo::Inspecao),
                        (false, Some(State::Processing)) => ("operar", Tipo::Operacao),
                        _ => ("carga/descarga", Tipo::CargaDescarga),
                    };
                    (format!("{verbo} {lugar}"), tipo, m)
                }
                State::TravelEmpty | State::TravelLoaded => {
                    let destino = self
                        .mais_proximo(onde(t0))
                        .map(|b| self.nome(b).to_string())
                        .unwrap_or_default();
                    if s == State::TravelEmpty {
                        (format!("andar até {destino}"), Tipo::Andar, None)
                    } else {
                        (
                            format!("transportar até {destino}"),
                            Tipo::Transportar,
                            None,
                        )
                    }
                }
                State::Loading | State::Unloading => {
                    let lugar = self
                        .mais_proximo(onde(t0))
                        .map(|b| self.nome(b).to_string())
                        .unwrap_or_default();
                    if s == State::Loading {
                        (format!("pegar peça em {lugar}"), Tipo::Pegar, None)
                    } else {
                        (format!("entregar peça em {lugar}"), Tipo::Entregar, None)
                    }
                }
                State::Idle | State::Empty => ("ocioso".to_string(), Tipo::Ocioso, None),
                s => match parado(s) {
                    Some(p) => (p.to_string(), Tipo::Parado, None),
                    None => ("esperando".to_string(), Tipo::Bloqueio, None),
                },
            };
            v.push(Trecho {
                t0,
                t1,
                nome,
                tipo,
                maquina,
                inicio_processo: false,
            });
        }
        juntar(v)
    }

    /// The person's `Allowance`, if the operator has one.
    fn tolerancia_do_modelo(&self, op: Option<usize>) -> f64 {
        op.and_then(|o| self.model.objects[o].person.as_ref())
            .and_then(|p| match p.fatigue {
                Some(Fatigue::Allowance(a)) => Some(a),
                _ => None,
            })
            .unwrap_or(0.0)
    }
}

/// Runs the study on the trace of one replication.
pub fn estudar(model: &Model, rastro: &Rastro, opcoes: &Opcoes) -> Result<Estudo, String> {
    let (w0, w1) = opcoes.janela.unwrap_or((rastro.aquecimento, rastro.parada));
    if w1.partial_cmp(&w0) != Some(std::cmp::Ordering::Greater) {
        return Err("o intervalo do estudo de tempos precisa ter fim maior que o início".into());
    }
    if !(opcoes.ritmo > 0.0 && opcoes.ritmo.is_finite()) {
        return Err("o ritmo precisa ser maior que zero (100 = ritmo normal)".into());
    }
    if let Some(t) = opcoes.tolerancia {
        if !(t >= 0.0 && t.is_finite()) {
            return Err("a tolerância precisa ser zero ou positiva".into());
        }
    }
    let horas = formulas::horas_por_unidade(&opcoes.unidade).ok_or_else(|| {
        format!(
            "unidade de tempo '{}' desconhecida (use s, min ou h)",
            opcoes.unidade
        )
    })?;
    let cx = Contexto {
        model,
        rastro,
        opcoes,
        fim: rastro.parada,
    };
    let n = model.objects.len().min(rastro.estados.len());
    let operadores: Vec<usize> = (0..n)
        .filter(|&b| matches!(model.objects[b].kind, ObjectKind::Transporter { .. }))
        .filter(|&b| !cx.maquinas_de(b).is_empty())
        .collect();
    // (operator, its stretches) of everyone who worked at a machine in the window.
    let mut por_operador: Vec<(usize, Vec<Trecho>)> = Vec::new();
    for &o in &operadores {
        let t = cx.trechos_operador(o);
        if t.iter()
            .any(|s| s.maquina.is_some() && s.t1 > w0 && s.t0 < w1)
        {
            por_operador.push((o, t));
        }
    }
    let maquinas_do = |trechos: &[Trecho]| -> Vec<usize> {
        let mut m: Vec<usize> = trechos
            .iter()
            .filter(|s| s.t1 > w0 && s.t0 < w1)
            .filter_map(|s| s.maquina)
            .collect();
        m.sort_unstable();
        m.dedup();
        m
    };
    let mut planos: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
    match &opcoes.ator {
        Some(nome) => {
            let b = (0..n)
                .find(|&b| model.objects[b].name == *nome)
                .ok_or_else(|| format!("não há objeto '{nome}' no modelo"))?;
            if let Some((o, t)) = por_operador.iter().find(|(o, _)| *o == b) {
                planos.push((Some(*o), maquinas_do(t)));
            } else if cx.e_maquina(b) {
                match por_operador
                    .iter()
                    .find(|(_, t)| maquinas_do(t).contains(&b))
                {
                    Some((o, t)) => planos.push((Some(*o), maquinas_do(t))),
                    None => planos.push((None, vec![b])),
                }
            } else {
                return Err(format!(
                    "'{nome}' não é um operador que trabalhou em máquinas nem uma máquina"
                ));
            }
        }
        None if !por_operador.is_empty() => {
            for (o, t) in &por_operador {
                planos.push((Some(*o), maquinas_do(t)));
            }
        }
        None => {
            for b in (0..n).filter(|&b| cx.e_maquina(b)) {
                planos.push((None, vec![b]));
            }
        }
    }
    let mut grupos = Vec::new();
    for (op, maquinas) in planos {
        let trechos_op = op.and_then(|o| {
            por_operador
                .iter()
                .find(|(x, _)| *x == o)
                .map(|(_, t)| t.clone())
        });
        if let Some(g) = grupo(&cx, op, trechos_op, &maquinas, (w0, w1), horas)? {
            grupos.push(g);
        }
    }
    if grupos.is_empty() {
        return Err(
            "nenhum ciclo de trabalho encontrado no intervalo (é preciso ao menos duas \
                    repetições do mesmo elemento)"
                .into(),
        );
    }
    Ok(Estudo {
        modelo: model.name.clone(),
        semente: rastro.semente,
        replicacao: rastro.replicacao,
        janela: (w0, w1),
        unidade: opcoes.unidade.clone(),
        grupos,
    })
}

/// The stretches (in time order, not overlapping) that meet `[a, b)`.
fn em(trechos: &[Trecho], a: f64, b: f64) -> &[Trecho] {
    let i = trechos.partition_point(|t| t.t1 <= a);
    let j = trechos.partition_point(|t| t.t0 < b);
    &trechos[i..j.max(i)]
}

/// Total time of the stretches matching `f` inside `[a, b)`.
fn soma(trechos: &[Trecho], a: f64, b: f64, f: impl Fn(&Trecho) -> bool) -> f64 {
    em(trechos, a, b)
        .iter()
        .filter(|t| f(t))
        .map(|t| (t.t1.min(b) - t.t0.max(a)).max(0.0))
        .sum()
}

/// Length of the intersection of the stretches `xs` with the union of `ys`, inside
/// `[a, b)`.
fn sobreposicao(xs: &[(f64, f64)], ys: &[(f64, f64)], a: f64, b: f64) -> f64 {
    let mut u: Vec<(f64, f64)> = ys
        .iter()
        .map(|&(p, q)| (p.max(a), q.min(b)))
        .filter(|(p, q)| q > p)
        .collect();
    u.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (p, q) in u {
        match merged.last_mut() {
            Some(l) if p <= l.1 => l.1 = l.1.max(q),
            _ => merged.push((p, q)),
        }
    }
    xs.iter()
        .map(|&(p, q)| {
            let (p, q) = (p.max(a), q.min(b));
            merged
                .iter()
                .map(|&(r, s)| (q.min(s) - p.max(r)).max(0.0))
                .sum::<f64>()
        })
        .sum()
}

fn grupo(
    cx: &Contexto,
    op: Option<usize>,
    trechos_op: Option<Vec<Trecho>>,
    maquinas: &[usize],
    (w0, w1): (f64, f64),
    horas: f64,
) -> Result<Option<Grupo>, String> {
    let mut atores: Vec<(usize, bool, Vec<Trecho>)> = Vec::new();
    if let (Some(o), Some(t)) = (op, trechos_op) {
        atores.push((o, true, t));
    }
    for &m in maquinas {
        atores.push((m, false, cx.trechos_maquina(m)));
    }
    let Some((_, _, base)) = atores.first() else {
        return Ok(None);
    };
    // Anchor: the most frequent element at the most visited machine (operator), or the
    // first work phase of the machine.
    let dentro = |t: &&Trecho| t.t0 >= w0 && t.t0 < w1;
    // Most frequent element among those matching `f` (ties: the one seen first).
    let ancora_de = |f: &dyn Fn(&Trecho) -> bool| -> Option<String> {
        let mut freq: Vec<(&str, usize)> = Vec::new();
        for t in base.iter().filter(dentro).filter(|t| f(t)) {
            match freq.iter_mut().find(|(n, _)| *n == t.nome) {
                Some(e) => e.1 += 1,
                None => freq.push((&t.nome, 1)),
            }
        }
        freq.iter()
            .fold(None::<(&str, usize)>, |best, &(s, n)| match best {
                Some((_, m)) if m >= n => best,
                _ => Some((s, n)),
            })
            .map(|(k, _)| k.to_string())
    };
    let ancora = if op.is_some() {
        let mut visitas: BTreeMap<usize, usize> = BTreeMap::new();
        for t in base.iter().filter(dentro) {
            if let Some(m) = t.maquina {
                *visitas.entry(m).or_default() += 1;
            }
        }
        let alvo = visitas
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            .map(|(m, _)| *m);
        ancora_de(&|t: &Trecho| t.maquina.is_some() && t.maquina == alvo)
    } else {
        ancora_de(&|t: &Trecho| t.tipo != Tipo::Automatico && t.tipo.trabalho())
            .or_else(|| ancora_de(&|t: &Trecho| t.tipo.trabalho()))
    };
    let Some(ancora) = ancora else {
        return Ok(None);
    };
    let segs: Vec<Seg> = base.iter().map(seg_of).collect();
    let mut cs: Vec<Ciclo> = ciclos::detectar(&segs, &ancora, w0, w1);
    // A breakdown or pause of any actor interrupts the cycle.
    for c in &mut cs {
        let parou = atores
            .iter()
            .any(|(_, _, t)| soma(t, c.inicio, c.fim, |s| s.tipo == Tipo::Parado) > 0.0);
        c.regular &= !parou;
    }
    if cs.is_empty() {
        return Ok(None);
    }
    let regulares: Vec<&Ciclo> = cs.iter().filter(|c| c.regular).collect();
    let representativo = match cx.opcoes.ciclo {
        Some(k) if k >= 1 && k <= cs.len() => Some(k - 1),
        Some(k) => {
            return Err(format!(
                "--ciclo {k}: há {} ciclo(s) no intervalo (numerados a partir de 1)",
                cs.len()
            ));
        }
        None => ciclos::mediano(&cs),
    };
    let nome_op = op.map(|o| cx.nome(o).to_string());
    let nomes_maq: Vec<String> = maquinas.iter().map(|&m| cx.nome(m).to_string()).collect();
    let nome = match &nome_op {
        Some(o) => format!("{o} com {}", nomes_maq.join(", ")),
        None => nomes_maq.join(", "),
    };
    let ciclos_info: Vec<CicloInfo> = cs
        .iter()
        .map(|c| CicloInfo {
            inicio: c.inicio,
            fim: c.fim,
            duracao: c.duracao(),
            regular: c.regular,
            assinatura: c.assinatura.clone(),
        })
        .collect();
    let tempo_ciclo = Resumo::de(&regulares.iter().map(|c| c.duracao()).collect::<Vec<_>>());
    // Allowance and rating.
    let no_modelo = cx.tolerancia_do_modelo(op);
    let (tolerancia, origem) = match cx.opcoes.tolerancia {
        Some(t) => (t, "informada (--tolerancia)".to_string()),
        None if no_modelo > 0.0 => (
            no_modelo,
            format!(
                "do perfil de {} (fadiga Allowance)",
                nome_op.clone().unwrap_or_default()
            ),
        ),
        None => (0.0, "nenhuma (informe --tolerancia)".to_string()),
    };
    let ritmo = cx.opcoes.ritmo;
    let fator_normal = |tipo: Tipo| -> f64 {
        if !tipo.manual() {
            return 1.0;
        }
        let tirar = if tipo.com_tolerancia_do_modelo() {
            1.0 / (1.0 + no_modelo)
        } else {
            1.0
        };
        tirar * ritmo
    };
    let fator_padrao = |tipo: Tipo| -> f64 {
        if tipo.manual() {
            fator_normal(tipo) * (1.0 + tolerancia)
        } else {
            1.0
        }
    };
    // Element statistics, in the order of the representative cycle.
    let mut elementos = Vec::new();
    let mut linhas_atores = Vec::new();
    for (b, e_op, trechos) in &atores {
        let mut ordem: Vec<(String, Tipo)> = Vec::new();
        let rep = representativo.map(|i| &cs[i]);
        let vistos = |a: f64, z: f64, ordem: &mut Vec<(String, Tipo)>| {
            for t in em(trechos, a, z) {
                if !ordem.iter().any(|(n, _)| *n == t.nome) {
                    ordem.push((t.nome.clone(), t.tipo));
                }
            }
        };
        if let Some(c) = rep {
            vistos(c.inicio, c.fim, &mut ordem);
        }
        for c in &regulares {
            vistos(c.inicio, c.fim, &mut ordem);
        }
        for (el, tipo) in ordem {
            let xs: Vec<f64> = regulares
                .iter()
                // Each occurrence counts whole in the cycle where it starts, as the
                // stopwatch reads it (a machine running across the start of the cycle
                // gives its full automatic time, not two pieces). One cut by the end of
                // the run is not a whole reading and is left out.
                .map(|c| {
                    em(trechos, c.inicio, c.fim)
                        .iter()
                        .filter(|t| t.nome == el && t.t0 >= c.inicio)
                        .filter(|t| t.t1 < cx.fim - 1e-9)
                        .map(|t| t.t1 - t.t0)
                        .sum::<f64>()
                })
                .filter(|d| *d > 1e-9)
                .collect();
            let Some(resumo) = Resumo::de(&xs) else {
                continue;
            };
            let (normal, padrao) = if tipo.trabalho() {
                (
                    Some(resumo.media * fator_normal(tipo)),
                    Some(resumo.media * fator_padrao(tipo)),
                )
            } else {
                (None, None)
            };
            elementos.push(LinhaElemento {
                ator: cx.nome(*b).to_string(),
                elemento: el,
                tipo,
                resumo,
                normal,
                padrao,
            });
        }
        // Actor summary.
        let k = regulares.len().max(1) as f64;
        let mut ocupado = 0.0;
        let mut ocioso = 0.0;
        let mut interf = 0.0;
        let mut total = 0.0;
        for c in &regulares {
            let (a, z) = (c.inicio, c.fim);
            total += z - a;
            if *e_op {
                ocupado += soma(trechos, a, z, |t| t.tipo.manual());
                ocioso += soma(trechos, a, z, |t| !t.tipo.trabalho());
                let livres: Vec<(f64, f64)> = em(trechos, a, z)
                    .iter()
                    .filter(|t| t.tipo == Tipo::Ocioso)
                    .map(|t| (t.t0, t.t1))
                    .collect();
                let rodando: Vec<(f64, f64)> = atores
                    .iter()
                    .filter(|(_, o, _)| !o)
                    .flat_map(|(_, _, ts)| {
                        em(ts, a, z)
                            .iter()
                            .filter(|t| t.tipo == Tipo::Automatico)
                            .map(|t| (t.t0, t.t1))
                    })
                    .collect();
                interf += sobreposicao(&livres, &rodando, a, z);
            } else {
                ocupado += soma(trechos, a, z, |t| t.tipo.trabalho());
                ocioso += soma(trechos, a, z, |t| !t.tipo.trabalho());
                interf += soma(trechos, a, z, |t| t.tipo == Tipo::EsperaOperador);
            }
        }
        linhas_atores.push(LinhaAtor {
            ator: cx.nome(*b).to_string(),
            papel: if *e_op { "operador" } else { "máquina" }.to_string(),
            ocupado: ocupado / k,
            utilizacao: if total > 0.0 { ocupado / total } else { 0.0 },
            ocioso: ocioso / k,
            interferencia: interf / k,
        });
    }
    // Per-cycle sums for the man-machine model and the standard time.
    let por_ciclo = |f: &dyn Fn(&Trecho) -> f64, de_op: Option<bool>| -> f64 {
        if regulares.is_empty() {
            return 0.0;
        }
        regulares
            .iter()
            .map(|c| {
                atores
                    .iter()
                    .filter(|(_, o, _)| de_op.is_none_or(|x| x == *o))
                    .map(|(_, _, ts)| {
                        em(ts, c.inicio, c.fim)
                            .iter()
                            .map(|t| (t.t1.min(c.fim) - t.t0.max(c.inicio)).max(0.0) * f(t))
                            .sum::<f64>()
                    })
                    .sum::<f64>()
            })
            .sum::<f64>()
            / regulares.len() as f64
    };
    let unidades = if regulares.is_empty() {
        0.0
    } else {
        regulares
            .iter()
            .map(|c| {
                atores
                    .iter()
                    .filter(|(_, o, _)| !o)
                    .flat_map(|(_, _, ts)| em(ts, c.inicio, c.fim).iter())
                    .filter(|t| t.inicio_processo && t.t0 >= c.inicio)
                    .count() as f64
            })
            .sum::<f64>()
            / regulares.len() as f64
    };
    let ciclo_obs = tempo_ciclo.map_or(0.0, |r| r.media);
    let nmaq = maquinas.len().max(1) as f64;
    let ind = |b: bool| if b { 1.0 } else { 0.0 };
    let (homem_maquina, ciclo_normal, ciclo_padrao) = if op.is_some() && !regulares.is_empty() {
        let a = por_ciclo(&|t| ind(t.tipo.concorrente()), Some(true)) / nmaq;
        let b = por_ciclo(
            &|t| ind(t.tipo.manual() && !t.tipo.concorrente()),
            Some(true),
        ) / nmaq;
        let tt = por_ciclo(&|t| ind(t.tipo == Tipo::Automatico), Some(false)) / nmaq;
        let abt = Abt { a, b, t: tt };
        let an = por_ciclo(
            &|t| ind(t.tipo.concorrente()) * fator_normal(t.tipo),
            Some(true),
        ) / nmaq;
        let bn = por_ciclo(
            &|t| ind(t.tipo.manual() && !t.tipo.concorrente()) * fator_normal(t.tipo),
            Some(true),
        ) / nmaq;
        let ap = por_ciclo(
            &|t| ind(t.tipo.concorrente()) * fator_padrao(t.tipo),
            Some(true),
        ) / nmaq;
        let bp = por_ciclo(
            &|t| ind(t.tipo.manual() && !t.tipo.concorrente()) * fator_padrao(t.tipo),
            Some(true),
        ) / nmaq;
        // Machine waiting for the operator, per machine (it is not in the textbook model,
        // where the operator walks while the machines run; here the operator may only
        // leave when called). It enters the normal and standard cycles as observed.
        let espera = por_ciclo(&|t| ind(t.tipo == Tipo::EsperaOperador), Some(false)) / nmaq;
        let ciclo_de = |a: f64, b: f64, i: f64| (a + tt + i).max(nmaq * (a + b));
        let atual = maquinas.len();
        let hm = HomemMaquina {
            n_linha: abt.n_linha(),
            maquinas_atuais: atual,
            espera_maquina: espera,
            ciclo_teorico: ciclo_de(a, b, 0.0),
            arranjos: abt.arranjos(atual, cx.opcoes.custos, horas),
            otimo: cx.opcoes.custos.and_then(|c| abt.otimo(c, horas)),
            abt,
        };
        (Some(hm), ciclo_de(an, bn, espera), ciclo_de(ap, bp, espera))
    } else {
        // A machine alone: the work content of its cycle.
        (
            None,
            por_ciclo(&|t| ind(t.tipo.trabalho()) * fator_normal(t.tipo), None),
            por_ciclo(&|t| ind(t.tipo.trabalho()) * fator_padrao(t.tipo), None),
        )
    };
    let padrao = (!regulares.is_empty()).then(|| TempoPadrao {
        ritmo,
        tolerancia,
        origem_tolerancia: origem,
        tolerancia_no_modelo: no_modelo,
        ciclo_observado: ciclo_obs,
        ciclo_normal,
        ciclo_padrao,
        unidades_por_ciclo: unidades,
        por_unidade_observado: (unidades > 0.0).then(|| ciclo_obs / unidades),
        por_unidade_padrao: (unidades > 0.0).then(|| ciclo_padrao / unidades),
    });
    let grafico = match representativo {
        Some(i) => {
            let c = &cs[i];
            atores
                .iter()
                .map(|(b, _, ts)| Coluna {
                    ator: cx.nome(*b).to_string(),
                    barras: ts
                        .iter()
                        .filter(|t| t.t1 > c.inicio && t.t0 < c.fim)
                        .map(|t| Barra {
                            t0: t.t0.max(c.inicio) - c.inicio,
                            t1: t.t1.min(c.fim) - c.inicio,
                            elemento: t.nome.clone(),
                            tipo: t.tipo,
                        })
                        .collect(),
                })
                .collect()
        }
        None => Vec::new(),
    };
    Ok(Some(Grupo {
        nome,
        operador: nome_op,
        maquinas: nomes_maq,
        ancora,
        ciclos: ciclos_info,
        representativo,
        tempo_ciclo,
        elementos,
        atores: linhas_atores,
        homem_maquina,
        padrao,
        grafico,
    }))
}

/// One element compared between two studies.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LinhaComparacao {
    pub ator: String,
    pub elemento: String,
    pub antes: Option<f64>,
    pub depois: Option<f64>,
}

impl LinhaComparacao {
    pub fn diferenca(&self) -> Option<f64> {
        Some(self.depois? - self.antes?)
    }

    /// Relative change (`-0.25` = 25 % less).
    pub fn variacao(&self) -> Option<f64> {
        let a = self.antes?;
        let d = self.depois?;
        (a.abs() > 1e-12).then(|| (d - a) / a)
    }
}

/// Before and after a method change.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Comparacao {
    pub antes: String,
    pub depois: String,
    pub grupo_antes: String,
    pub grupo_depois: String,
    /// Mean time per cycle of each element, paired by actor and element name.
    pub elementos: Vec<LinhaComparacao>,
    /// Cycle, standard time, utilisations and interference.
    pub indicadores: Vec<LinhaComparacao>,
}

/// Compares the group of `antes` with the matching group of `depois` (same operator, or
/// the same machines, or the first one).
pub fn comparar(antes: &Estudo, depois: &Estudo) -> Result<Comparacao, String> {
    let ga = antes
        .grupos
        .first()
        .ok_or("o estudo do primeiro modelo não tem ciclos")?;
    let gd = depois
        .grupos
        .iter()
        .find(|g| g.operador.is_some() && g.operador == ga.operador)
        .or_else(|| depois.grupos.iter().find(|g| g.maquinas == ga.maquinas))
        .or_else(|| depois.grupos.first())
        .ok_or("o estudo do segundo modelo não tem ciclos")?;
    Ok(comparar_grupos(&antes.modelo, ga, &depois.modelo, gd))
}

/// Element-by-element comparison of two groups.
pub fn comparar_grupos(nome_a: &str, a: &Grupo, nome_d: &str, d: &Grupo) -> Comparacao {
    let mut elementos: Vec<LinhaComparacao> = a
        .elementos
        .iter()
        .map(|e| LinhaComparacao {
            ator: e.ator.clone(),
            elemento: e.elemento.clone(),
            antes: Some(e.resumo.media),
            depois: d
                .elementos
                .iter()
                .find(|x| x.ator == e.ator && x.elemento == e.elemento)
                .map(|x| x.resumo.media),
        })
        .collect();
    for e in &d.elementos {
        if !a
            .elementos
            .iter()
            .any(|x| x.ator == e.ator && x.elemento == e.elemento)
        {
            elementos.push(LinhaComparacao {
                ator: e.ator.clone(),
                elemento: e.elemento.clone(),
                antes: None,
                depois: Some(e.resumo.media),
            });
        }
    }
    let ind = |nome: &str, f: &dyn Fn(&Grupo) -> Option<f64>| LinhaComparacao {
        ator: String::new(),
        elemento: nome.to_string(),
        antes: f(a),
        depois: f(d),
    };
    let mut indicadores = vec![
        ind("tempo de ciclo (média)", &|g| {
            g.tempo_ciclo.map(|r| r.media)
        }),
        ind("unidades por ciclo", &|g| {
            g.padrao.as_ref().map(|p| p.unidades_por_ciclo)
        }),
        ind("tempo padrão do ciclo", &|g| {
            g.padrao.as_ref().map(|p| p.ciclo_padrao)
        }),
        ind("tempo padrão por unidade", &|g| {
            g.padrao.as_ref().and_then(|p| p.por_unidade_padrao)
        }),
        ind("n' (máquinas por operador)", &|g| {
            g.homem_maquina
                .as_ref()
                .map(|h| h.n_linha)
                .filter(|x| x.is_finite())
        }),
    ];
    let mut nomes: Vec<&str> = a.atores.iter().map(|x| x.ator.as_str()).collect();
    for x in &d.atores {
        if !nomes.contains(&x.ator.as_str()) {
            nomes.push(&x.ator);
        }
    }
    for n in nomes {
        let de =
            |g: &Grupo, f: &dyn Fn(&LinhaAtor) -> f64| g.atores.iter().find(|x| x.ator == n).map(f);
        indicadores.push(LinhaComparacao {
            ator: n.to_string(),
            elemento: "utilização".into(),
            antes: de(a, &|x| x.utilizacao),
            depois: de(d, &|x| x.utilizacao),
        });
        indicadores.push(LinhaComparacao {
            ator: n.to_string(),
            elemento: "ocioso por ciclo".into(),
            antes: de(a, &|x| x.ocioso),
            depois: de(d, &|x| x.ocioso),
        });
        indicadores.push(LinhaComparacao {
            ator: n.to_string(),
            elemento: "interferência por ciclo".into(),
            antes: de(a, &|x| x.interferencia),
            depois: de(d, &|x| x.interferencia),
        });
    }
    Comparacao {
        antes: nome_a.to_string(),
        depois: nome_d.to_string(),
        grupo_antes: a.nome.clone(),
        grupo_depois: d.nome.clone(),
        elementos,
        indicadores,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grupo_com(elementos: &[(&str, &str, f64)], ciclo: f64) -> Grupo {
        Grupo {
            nome: "g".into(),
            operador: Some("ana".into()),
            maquinas: vec!["m1".into()],
            ancora: "carga/descarga m1".into(),
            ciclos: Vec::new(),
            representativo: None,
            tempo_ciclo: Resumo::de(&[ciclo]),
            elementos: elementos
                .iter()
                .map(|(a, e, m)| LinhaElemento {
                    ator: a.to_string(),
                    elemento: e.to_string(),
                    tipo: Tipo::CargaDescarga,
                    resumo: Resumo::de(&[*m]).unwrap(),
                    normal: None,
                    padrao: None,
                })
                .collect(),
            atores: vec![LinhaAtor {
                ator: "ana".into(),
                papel: "operador".into(),
                ocupado: ciclo / 2.0,
                utilizacao: 0.5,
                ocioso: ciclo / 2.0,
                interferencia: 0.0,
            }],
            homem_maquina: None,
            padrao: None,
            grafico: Vec::new(),
        }
    }

    #[test]
    fn comparison_pairs_elements_by_name() {
        let a = grupo_com(
            &[
                ("ana", "carga/descarga m1", 20.0),
                ("ana", "andar até m2", 6.0),
                ("m1", "automático", 70.0),
            ],
            96.0,
        );
        let d = grupo_com(
            &[
                ("ana", "carga/descarga m1", 14.0),
                ("m1", "automático", 70.0),
                ("ana", "inspecionar m1", 3.0),
            ],
            84.0,
        );
        let c = comparar_grupos("antes", &a, "depois", &d);
        let get = |e: &str| c.elementos.iter().find(|l| l.elemento == e).unwrap();
        let carga = get("carga/descarga m1");
        assert_eq!(carga.diferenca(), Some(-6.0));
        assert!((carga.variacao().unwrap() + 0.3).abs() < 1e-12);
        assert_eq!(get("andar até m2").depois, None);
        assert_eq!(get("andar até m2").diferenca(), None);
        assert_eq!(get("inspecionar m1").antes, None);
        assert_eq!(get("automático").diferenca(), Some(0.0));
        let ciclo = &c.indicadores[0];
        assert_eq!((ciclo.antes, ciclo.depois), (Some(96.0), Some(84.0)));
        assert!(
            c.indicadores
                .iter()
                .any(|l| l.ator == "ana" && l.elemento == "utilização")
        );
    }

    #[test]
    fn summary_statistics() {
        let r = Resumo::de(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        assert!((r.media - 5.0).abs() < 1e-12);
        // Sample sd: sqrt(32/7).
        assert!((r.dp - (32.0f64 / 7.0).sqrt()).abs() < 1e-12);
        assert_eq!((r.min, r.max, r.n), (2.0, 9.0, 8));
        assert_eq!(Resumo::de(&[3.0]).unwrap().dp, 0.0);
        assert!(Resumo::de(&[]).is_none());
    }

    #[test]
    fn overlap_of_idle_and_running() {
        // Idle 0-10 and 20-30; machines run 5-12 and 8-25: overlap 5 + 5 = 10.
        let x = sobreposicao(
            &[(0.0, 10.0), (20.0, 30.0)],
            &[(5.0, 12.0), (8.0, 25.0)],
            0.0,
            100.0,
        );
        assert!((x - 10.0).abs() < 1e-12);
        // Clipped to the cycle [6, 22].
        let y = sobreposicao(
            &[(0.0, 10.0), (20.0, 30.0)],
            &[(5.0, 12.0), (8.0, 25.0)],
            6.0,
            22.0,
        );
        assert!((y - 6.0).abs() < 1e-12);
    }
}
