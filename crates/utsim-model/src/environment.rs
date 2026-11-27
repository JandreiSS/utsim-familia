//! The environmental block (UTSim Ambiental, `docs/formato_ambiental.md` of that repository):
//! `environment` on an object (power per state group, flows, water, effluent) and on the
//! model (flows, emission factors, destinations, tariff, targets).
//!
//! Every field is optional and the simulation reads none of them: the environmental
//! accounts are made after the run, on the report and the trace, so switching the block on
//! changes no time and no draw. A model without the block opens, runs and saves as before.
//!
//! Field names are English, like `quality` and `safety`; the variants also accept the
//! Portuguese name (`Peca`, `Tempo`, `Partida`). The comments below are in Portuguese, as in
//! the app that owns the block.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// O bloco ambiental de um objeto.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentDef {
    /// A etapa do processo a que o nó pertence ("pintura").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Potência em kW por grupo de estado.
    pub power: PowerDef,
    /// Energia por evento (kWh por peça, por setup, por partida).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub energy_per: Vec<EnergyPer>,
    /// Fluxos do catálogo: `quantidade = amount × direcionador`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<FlowUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<WaterDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effluent: Option<EffluentDef>,
    /// Desligamento em espera, aplicado no cálculo (aproximação declarada).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standby: Option<StandbyDef>,
}

/// Potência em kW por grupo de estado; `by_state` vale no lugar do grupo para o estado.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerDef {
    pub producing: f64,
    pub preparing: f64,
    pub waiting: f64,
    pub stopped: f64,
    pub off_shift: f64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_state: BTreeMap<String, f64>,
}

impl PowerDef {
    /// A potência do grupo.
    pub fn do_grupo(&self, g: Grupo) -> f64 {
        match g {
            Grupo::Produzindo => self.producing,
            Grupo::Preparando => self.preparing,
            Grupo::Esperando => self.waiting,
            Grupo::Parada => self.stopped,
            Grupo::ForaDeTurno => self.off_shift,
        }
    }

    pub fn do_grupo_mut(&mut self, g: Grupo) -> &mut f64 {
        match g {
            Grupo::Produzindo => &mut self.producing,
            Grupo::Preparando => &mut self.preparing,
            Grupo::Esperando => &mut self.waiting,
            Grupo::Parada => &mut self.stopped,
            Grupo::ForaDeTurno => &mut self.off_shift,
        }
    }

    /// A potência no estado do motor (`"blocked"`): a própria, senão a do grupo, senão 0.
    pub fn no_estado(&self, estado: &str) -> f64 {
        if let Some(p) = self.by_state.get(estado) {
            return *p;
        }
        Grupo::de_estado(estado).map_or(0.0, |g| self.do_grupo(g))
    }

    /// Nenhuma potência informada.
    pub fn vazia(&self) -> bool {
        Grupo::TODOS.iter().all(|g| self.do_grupo(*g) == 0.0)
            && self.by_state.values().all(|p| *p == 0.0)
    }
}

/// Os cinco grupos de estado da potência (plano 2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Grupo {
    /// Energia que agrega valor.
    Produzindo,
    /// Necessária, mas não agrega.
    Preparando,
    /// Ligada sem produzir.
    Esperando,
    /// Em quebra ou manutenção.
    Parada,
    /// Consumo de base fora do turno.
    ForaDeTurno,
}

impl Grupo {
    pub const TODOS: [Grupo; 5] = [
        Grupo::Produzindo,
        Grupo::Preparando,
        Grupo::Esperando,
        Grupo::Parada,
        Grupo::ForaDeTurno,
    ];

    /// O grupo de um estado do motor; `None` para estados de fora dos cinco grupos
    /// (`generating`, viagens de veículos): contam só com potência própria em `by_state`.
    pub fn de_estado(estado: &str) -> Option<Grupo> {
        Some(match estado {
            "processing" | "utilize" => Grupo::Produzindo,
            "setup" | "entering" | "leaving" | "collecting" | "releasing" => Grupo::Preparando,
            "idle" | "empty" | "blocked" | "waiting_operator" | "waiting_transport" => {
                Grupo::Esperando
            }
            "down" | "waiting_repair" | "maintenance" | "stopped" => Grupo::Parada,
            "off_shift" => Grupo::ForaDeTurno,
            _ => return None,
        })
    }

    /// O grupo agrega valor? Só "produzindo".
    pub fn agrega_valor(self) -> bool {
        self == Grupo::Produzindo
    }

    /// A energia deste grupo é "sem valor agregado"? Esperando, parada e fora de turno.
    pub fn sem_valor(self) -> bool {
        matches!(self, Grupo::Esperando | Grupo::Parada | Grupo::ForaDeTurno)
    }

    /// O nome curto, usado nas chaves das métricas e dos textos.
    pub fn chave(self) -> &'static str {
        match self {
            Grupo::Produzindo => "produzindo",
            Grupo::Preparando => "preparando",
            Grupo::Esperando => "esperando",
            Grupo::Parada => "parada",
            Grupo::ForaDeTurno => "fora_de_turno",
        }
    }

    pub fn indice(self) -> usize {
        self as usize
    }
}

/// Energia por evento.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnergyPer {
    pub driver: EventDriver,
    pub kwh: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventDriver {
    #[serde(alias = "Peca")]
    Item,
    Setup,
    #[serde(alias = "Partida")]
    Startup,
}

/// Um fluxo usado pelo nó.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowUse {
    /// O nome do fluxo no catálogo do modelo.
    pub flow: String,
    pub amount: f64,
    pub driver: Driver,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub adds_mass: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removes_mass: bool,
}

/// O direcionador: o que a simulação mediu (plano 2.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Driver {
    /// Peça que entrou no nó.
    #[serde(alias = "Peca")]
    Item,
    /// Peça que saiu do nó.
    #[serde(alias = "PecaBoa")]
    GoodItem,
    #[serde(alias = "Refugo")]
    Scrap,
    #[serde(alias = "Retrabalho")]
    Rework,
    Setup,
    #[serde(alias = "Partida")]
    Startup,
    /// Horas nos estados dados.
    #[serde(alias = "Tempo")]
    Time(Vec<Estado>),
    /// Metros carregado e vazio.
    #[serde(alias = "Distancia")]
    Distance {
        loaded: f64,
        empty: f64,
    },
    Volume,
    #[serde(alias = "Massa")]
    Mass,
}

/// Os estados do motor, como aparecem no formato (`Time([Processing, Setup])`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Estado {
    Idle,
    Empty,
    Generating,
    Releasing,
    Collecting,
    Setup,
    Processing,
    Blocked,
    WaitingOperator,
    WaitingTransport,
    Down,
    OffShift,
    TravelEmpty,
    TravelLoaded,
    Loading,
    Unloading,
    Utilize,
    Stopped,
    WaitingRepair,
    Maintenance,
    Resting,
    Entering,
    Leaving,
}

impl Estado {
    /// O nome do estado no relatório do motor (`"processing"`).
    pub fn nome(self) -> &'static str {
        match self {
            Estado::Idle => "idle",
            Estado::Empty => "empty",
            Estado::Generating => "generating",
            Estado::Releasing => "releasing",
            Estado::Collecting => "collecting",
            Estado::Setup => "setup",
            Estado::Processing => "processing",
            Estado::Blocked => "blocked",
            Estado::WaitingOperator => "waiting_operator",
            Estado::WaitingTransport => "waiting_transport",
            Estado::Down => "down",
            Estado::OffShift => "off_shift",
            Estado::TravelEmpty => "travel_empty",
            Estado::TravelLoaded => "travel_loaded",
            Estado::Loading => "loading",
            Estado::Unloading => "unloading",
            Estado::Utilize => "utilize",
            Estado::Stopped => "stopped",
            Estado::WaitingRepair => "waiting_repair",
            Estado::Maintenance => "maintenance",
            Estado::Resting => "resting",
            Estado::Entering => "entering",
            Estado::Leaving => "leaving",
        }
    }
}

/// O balanço de água do nó.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaterDef {
    pub incorporated: f64,
    pub evaporated: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reuse_to: Option<String>,
    pub reuse: f64,
}

/// O efluente do nó.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffluentDef {
    pub destination: String,
    /// mg/L por parâmetro do catálogo.
    pub concentration: BTreeMap<String, f64>,
}

/// Desliga depois de `after` unidades de tempo esperando.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StandbyDef {
    pub after: f64,
}

/// O bloco ambiental do modelo: catálogo, tarifa, regras e metas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentCatalog {
    pub clock: Clock,
    pub flows: Vec<FlowDef>,
    pub factors: Vec<Factor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gases: Vec<Gas>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub waste_destinations: Vec<WasteDestination>,
    #[serde(skip_serializing_if = "EffluentCatalog::vazio")]
    pub effluent: EffluentCatalog,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tariff: Option<Tariff>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub prices: BTreeMap<String, f64>,
    pub allocation: Allocation,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Target>,
    /// Massa (kg) de cada tipo de item, pelo nome do tipo; `"*"` vale para as peças sem tipo
    /// (fase AM5; vai para `item_types` quando o campo entrar na família).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub item_mass_kg: BTreeMap<String, f64>,
    /// O destino da sucata (as peças que chegam às saídas de refugo).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrap_destination: Option<String>,
    /// O orçamento de carbono do período simulado (kg CO₂e), para o painel de metas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carbon_budget_kg: Option<f64>,
    /// O professor desligou os selos de conquista.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hide_badges: bool,
}

impl EnvironmentCatalog {
    /// O fluxo de energia elétrica: o primeiro fluxo do tipo `Energy`. A potência dos nós
    /// é contada nele.
    pub fn fluxo_eletrico(&self) -> Option<&FlowDef> {
        self.flows.iter().find(|f| f.kind == FlowKind::Energy)
    }

    pub fn fluxo(&self, nome: &str) -> Option<&FlowDef> {
        self.flows.iter().find(|f| f.name == nome)
    }

    pub fn fator(&self, fluxo: &str) -> Option<&Factor> {
        self.factors.iter().find(|f| f.flow == fluxo)
    }

    /// O catálogo de partida do app: o fluxo "eletricidade" (kWh) com o fator a definir.
    /// Nenhum valor de fator vem de fábrica (decisão A8).
    pub fn inicial() -> EnvironmentCatalog {
        EnvironmentCatalog {
            flows: vec![FlowDef {
                name: "eletricidade".into(),
                kind: FlowKind::Energy,
                unit: "kWh".into(),
                direction: Direction::In,
                destination: None,
            }],
            factors: vec![Factor {
                flow: "eletricidade".into(),
                scope: 2,
                kg_co2e: None,
                source: A_DEFINIR.into(),
                year: None,
                range: None,
                didactic: false,
            }],
            ..EnvironmentCatalog::default()
        }
    }
}

/// O texto da fonte de um fator ainda sem fonte.
pub const A_DEFINIR: &str = "a definir";

/// O relógio do modelo: a hora de início e a unidade de tempo da simulação.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clock {
    pub start: String,
    pub time_unit: TimeUnit,
}

impl Default for Clock {
    fn default() -> Self {
        Clock {
            start: "06:00".into(),
            time_unit: TimeUnit::Second,
        }
    }
}

impl Clock {
    /// O instante `t` do modelo no relógio: o dia (0 = o primeiro) e os minutos desde a
    /// meia-noite. Sem hora de início legível, o dia começa às 0:00.
    pub fn relogio(&self, t: f64) -> (u32, f64) {
        let inicio = hora(&self.start).unwrap_or(0) as f64;
        let minutos = inicio + t * self.time_unit.em_horas() * 60.0;
        let dia = (minutos / 1440.0).floor().max(0.0);
        (dia as u32, minutos - dia * 1440.0)
    }

    /// `"HH:MM"` do instante `t` (com `+N` quando passa do primeiro dia).
    pub fn hora(&self, t: f64) -> String {
        let (d, m) = self.relogio(t);
        let m = m.round() as u32 % 1440;
        let h = format!("{:02}:{:02}", m / 60, m % 60);
        if d > 0 { format!("{h} +{d}") } else { h }
    }
}

/// A unidade de tempo do modelo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeUnit {
    #[default]
    #[serde(alias = "Segundo")]
    Second,
    #[serde(alias = "Minuto")]
    Minute,
    #[serde(alias = "Hora")]
    Hour,
}

impl TimeUnit {
    pub const TODAS: [TimeUnit; 3] = [TimeUnit::Second, TimeUnit::Minute, TimeUnit::Hour];

    /// Quantas horas tem uma unidade.
    pub fn em_horas(self) -> f64 {
        match self {
            TimeUnit::Second => 1.0 / 3600.0,
            TimeUnit::Minute => 1.0 / 60.0,
            TimeUnit::Hour => 1.0,
        }
    }
}

/// Um fluxo do catálogo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowDef {
    pub name: String,
    pub kind: FlowKind,
    pub unit: String,
    pub direction: Direction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowKind {
    Energy,
    Fuel,
    Water,
    Material,
    Waste,
    Effluent,
    Emission,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    In,
    Out,
}

/// Um fator de emissão: kg de CO₂ equivalente por unidade do fluxo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Factor {
    pub flow: String,
    /// Escopo 1, 2 ou 3.
    pub scope: u8,
    /// `None`: a definir. O indicador que depende dele mostra "falta fator".
    pub kg_co2e: Option<f64>,
    pub source: String,
    #[serde(default)]
    pub year: Option<u32>,
    /// Faixa de incerteza (mínimo, máximo), numa fase posterior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<(f64, f64)>,
    /// Valor didático (missões do Ensino): o rótulo acompanha os números até o relatório.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub didactic: bool,
}

impl Factor {
    /// O fator não tem fonte declarada.
    pub fn sem_fonte(&self) -> bool {
        let s = self.source.trim();
        s.is_empty() || s.eq_ignore_ascii_case(A_DEFINIR)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gas {
    pub name: String,
    pub kg_co2e: Option<f64>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WasteDestination {
    pub name: String,
    pub diverted: bool,
    pub kg_co2e: Option<f64>,
    pub cost: Option<f64>,
    pub source: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffluentCatalog {
    pub parameters: Vec<Parameter>,
    pub destinations: Vec<EffluentDestination>,
}

impl EffluentCatalog {
    pub fn vazio(&self) -> bool {
        self.parameters.is_empty() && self.destinations.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub unit: String,
    pub limit: Option<f64>,
    pub limit_source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffluentDestination {
    pub name: String,
    #[serde(default)]
    pub removal: BTreeMap<String, f64>,
    #[serde(default)]
    pub kwh_per_m3: f64,
    #[serde(default)]
    pub sludge_kg_per_m3: f64,
    #[serde(default)]
    pub cost_per_m3: Option<f64>,
}

/// A tarifa de energia (fase AM4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tariff {
    pub peak: Peak,
    pub demand_interval_min: u32,
    pub contracted_kw: Option<f64>,
    pub price_kwh_peak: Option<f64>,
    pub price_kwh_off_peak: Option<f64>,
    pub price_kw: Option<f64>,
    /// Preço de cada kW acima da demanda contratada (ultrapassagem).
    #[serde(default)]
    pub price_kw_overrun: Option<f64>,
    pub source: String,
}

impl Tariff {
    /// A tarifa de partida: pico das 18:00 às 21:00 em dias úteis, intervalo de 15 minutos,
    /// todos os preços a definir.
    pub fn inicial() -> Tariff {
        Tariff {
            peak: Peak {
                from: "18:00".into(),
                to: "21:00".into(),
                weekdays_only: true,
            },
            demand_interval_min: 15,
            contracted_kw: None,
            price_kwh_peak: None,
            price_kwh_off_peak: None,
            price_kw: None,
            price_kw_overrun: None,
            source: A_DEFINIR.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Peak {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub weekdays_only: bool,
}

/// A regra de rateio para o produto (fase AM7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Allocation {
    #[default]
    ByProcessTime,
    ByItems,
    ByMass,
    /// Não rateado: aparece como "estrutura".
    #[serde(rename = "None")]
    Structure,
}

/// Uma meta (fase AM9).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub indicator: String,
    pub scope: TargetScope,
    pub basis: Basis,
    pub baseline: Option<f64>,
    pub target: Option<f64>,
    pub due: Option<String>,
    /// Os cenários ligados à meta como ações (pelo nome).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TargetScope {
    Factory,
    Stage(String),
    Node(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Basis {
    Total,
    PerGoodItem,
    PerHour,
}

/// `"HH:MM"` em minutos desde a meia-noite, dentro de um dia.
pub fn hora(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// A energia (kWh) de um nó na janela de estatística de uma replicação, por grupo de
/// estado (na ordem de [`Grupo::TODOS`]), só pelas frações de tempo do relatório: a conta
/// simples que o UTSim mostra na aba Ambiental. A conta completa (setups, partidas,
/// desligamento em espera, janelas) é do UTSim Ambiental.
pub fn energia_por_grupo(
    def: &EnvironmentDef,
    bloco: &utsim_core::BlockReport,
    janela: f64,
    horas_por_unidade: f64,
) -> [f64; 5] {
    let mut g = [0.0; 5];
    let h = janela.max(0.0) * horas_por_unidade;
    for (estado, frac) in &bloco.states {
        if let Some(gr) = Grupo::de_estado(estado) {
            g[gr.indice()] += frac * h * def.power.no_estado(estado);
        }
    }
    let por_peca: f64 = def
        .energy_per
        .iter()
        .filter(|e| e.driver == EventDriver::Item)
        .map(|e| e.kwh)
        .sum();
    g[Grupo::Produzindo.indice()] += por_peca * bloco.input as f64;
    g
}

/// O fator (kg CO₂e por unidade) de um fluxo do catálogo; o de um resíduo é o do destino
/// dele. `None`: falta o fator.
fn fator(cat: &EnvironmentCatalog, fluxo: &str) -> Option<f64> {
    match cat.fluxo(fluxo) {
        Some(f) if f.kind == FlowKind::Waste => {
            let d = f.destination.as_deref()?;
            cat.waste_destinations
                .iter()
                .find(|x| x.name == d)
                .and_then(|x| x.kg_co2e)
        }
        _ => cat.fator(fluxo).and_then(|f| f.kg_co2e),
    }
}

/// Publica as métricas ambientais de cada nó com bloco no relatório da replicação, como
/// métricas do próprio bloco (`<nó>.energia_kwh`...): o UTSim Ensino e os experimentos as
/// leem como qualquer outra. É a conta simples, pelas frações de tempo do relatório e pelas
/// peças que entraram e saíram; a conta completa (setups, partidas, desligamento em espera,
/// massa, água) é do UTSim Ambiental. A simulação não muda.
///
/// Métricas: `energia_kwh`, `energia_kwh.<grupo>`, `energia_sem_valor_kwh`,
/// `kwh_por_peca` (com peças saindo), `fluxo.<fluxo>` (direcionador por peça, peça boa ou
/// tempo em estados) e `co2e_kg` (só quando todo fluxo usado tem fator).
pub fn add_to_report(model: &crate::Model, report: &mut utsim_core::RunReport) {
    let vazio = EnvironmentCatalog::inicial();
    let cat = model.environment.as_deref().unwrap_or(&vazio);
    let hu = cat.clock.time_unit.em_horas();
    let janela = report.end_time - report.stats_from;
    let eletrico = cat.fluxo_eletrico().map(|f| f.name.clone());
    for o in &model.objects {
        let Some(def) = o.environment.as_deref() else {
            continue;
        };
        let Some(b) = report.blocks.iter_mut().find(|b| b.name == o.name) else {
            continue;
        };
        let g = energia_por_grupo(def, b, janela, hu);
        let kwh: f64 = g.iter().sum();
        let mut m: Vec<(String, f64)> = vec![("energia_kwh".into(), kwh)];
        for gr in Grupo::TODOS {
            m.push((format!("energia_kwh.{}", gr.chave()), g[gr.indice()]));
        }
        let sem: f64 = Grupo::TODOS
            .iter()
            .filter(|x| x.sem_valor())
            .map(|x| g[x.indice()])
            .sum();
        m.push(("energia_sem_valor_kwh".into(), sem));
        if b.output > 0 {
            m.push(("kwh_por_peca".into(), kwh / b.output as f64));
        }
        let horas = |estado: &str| b.states.get(estado).copied().unwrap_or(0.0) * janela * hu;
        // Todo fluxo do catálogo sai, com 0 quando o nó não o usa: uma alavanca que troca o
        // bloco (estufa a gás por elétrica) não faz a métrica sumir.
        let mut fluxos: BTreeMap<String, f64> = cat
            .flows
            .iter()
            .filter(|f| Some(&f.name) != eletrico.as_ref())
            .map(|f| (f.name.clone(), 0.0))
            .collect();
        for f in &def.flows {
            let dir = match &f.driver {
                Driver::Item => b.input as f64,
                Driver::GoodItem => b.output as f64,
                Driver::Time(es) => es.iter().map(|e| horas(e.nome())).sum(),
                _ => continue,
            };
            *fluxos.entry(f.flow.clone()).or_default() += f.amount * dir;
        }
        let mut co2 = Some(0.0);
        if kwh > 0.0 {
            co2 = match (co2, eletrico.as_deref().and_then(|e| fator(cat, e))) {
                (Some(c), Some(k)) => Some(c + kwh * k),
                _ => None,
            };
        }
        for (f, q) in &fluxos {
            m.push((format!("fluxo.{f}"), *q));
            if *q != 0.0 {
                co2 = match (co2, fator(cat, f)) {
                    (Some(c), Some(k)) => Some(c + q * k),
                    _ => None,
                };
            }
        }
        if let Some(c) = co2 {
            m.push(("co2e_kg".into(), c));
        }
        b.custom.extend(m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hora_e_relogio() {
        assert_eq!(hora("18:30"), Some(1110));
        assert_eq!(hora("24:00"), None);
        assert_eq!(Clock::default().hora(3600.0), "07:00");
    }

    #[test]
    fn bloco_volta_igual() {
        let t = r#"(stage: Some("pintura"), power: (producing: 18.0, waiting: 9.0, by_state: {"blocked": 12.0}),
            energy_per: [(driver: Startup, kwh: 6.0)],
            flows: [(flow: "gas", amount: 2.4, driver: Tempo([Processing, Setup]))],
            standby: Some((after: 600.0)))"#;
        let d: EnvironmentDef = ron::from_str(t).unwrap();
        assert_eq!(d.power.no_estado("blocked"), 12.0);
        assert_eq!(d.power.no_estado("idle"), 9.0);
        let de_novo: EnvironmentDef = ron::from_str(&ron::to_string(&d).unwrap()).unwrap();
        assert_eq!(d, de_novo);
    }

    /// As métricas por nó saem no relatório: caso resolvido à mão.
    #[test]
    fn metricas_no_relatorio() {
        let m = crate::Model::parse(
            r#"Model(name: "m", run: (stop_time: 7200.0, warmup: 0.0, seed: 1, replications: 1),
               objects: [(name: "a", kind: Source(interarrival: Const(60.0))),
                         (name: "forno", kind: Processor(process: Const(30.0)),
                          environment: Some((power: (producing: 10.0, waiting: 4.0),
                            flows: [(flow: "gas", amount: 2.0, driver: Item)]))),
                         (name: "b", kind: Sink)],
               connections: [("a", "forno"), ("forno", "b")],
               environment: Some((
                 flows: [(name: "eletricidade", kind: Energy, unit: "kWh", direction: In),
                         (name: "gas", kind: Fuel, unit: "m3", direction: In)],
                 factors: [(flow: "eletricidade", scope: 2, kg_co2e: Some(0.5), source: "teste", didactic: true),
                           (flow: "gas", scope: 1, kg_co2e: Some(2.0), source: "teste")])))"#,
        )
        .unwrap();
        let r = &m.run_replications().unwrap()[0];
        let x = r.metrics();
        // 2 h: metade produzindo a 10 kW (10 kWh), metade esperando a 4 kW (4 kWh).
        let e = x["forno.energia_kwh"];
        assert!((e - 14.0).abs() < 0.1, "{e}");
        let gas = x["forno.fluxo.gas"];
        let pecas = x["forno.input"];
        assert!((gas - 2.0 * pecas).abs() < 1e-9);
        assert!((x["forno.co2e_kg"] - (e * 0.5 + gas * 2.0)).abs() < 1e-9);
        assert!((x["forno.energia_sem_valor_kwh"] - x["forno.energia_kwh.esperando"]).abs() < 1e-9);
        // Fluxo do catálogo que o nó não usa sai com 0.
        assert_eq!(x["forno.fluxo.gas"], gas);
        assert!(!x.contains_key("forno.fluxo.eletricidade"));
        // Sem bloco, nada novo.
        assert!(!x.contains_key("a.energia_kwh"));
        // A alavanca de uma missão muda a potência pelo patch.
        let mut set = std::collections::BTreeMap::new();
        set.insert(
            "forno.environment.power.waiting".to_string(),
            "1.0".to_string(),
        );
        let m2 = crate::experiment::apply_patches(&m, &set).unwrap();
        let e2 = m2.run_replications().unwrap()[0].metrics()["forno.energia_kwh"];
        assert!((e - e2 - 3.0).abs() < 0.1, "{e} {e2}");
    }
}
