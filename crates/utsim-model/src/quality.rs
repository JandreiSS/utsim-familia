//! Quality and occupational safety blocks of the model (`docs/formato_qualidade_seguranca.md`):
//! defects and measured characteristics drawn by a process (`quality`), inspection stations
//! (`inspection`), rework (`rework`), roles of the sinks (`sink_role`), personal protective
//! equipment required at a work station (`ppe`) and risk zones on the floor (`zones`).
//!
//! Every block is optional. A model without them builds exactly the same simulation as
//! before: the blocks only add an entry hook ([`QualityHook`]) to the objects that have
//! them, with its own random stream, so the process times of the object are not shifted.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_core::rng::{self, SimRng};
use utsim_core::script::{ItemScript, ScriptEnv};
use utsim_core::{Dist, Item, Value};

/// Prefix of the label that marks a defect type on an item: `defeito.<type>` = 1 (present)
/// or 0 (fixed by rework).
pub const DEFECT_PREFIX: &str = "defeito.";
/// Label with the number of defects present on the item.
pub const DEFECTS_LABEL: &str = "defeitos";
/// Label with the number of passes through rework stations.
pub const PASSES_LABEL: &str = "passagens";
/// Default label where an inspection writes the output port of the item.
pub const INSPECTION_LABEL: &str = "inspecao";
/// Suffix of the label with the instant a characteristic was measured (`<name>@t`).
pub const INSTANT_SUFFIX: &str = "@t";

/// Offset of the quality random streams, above the kernel ones (downtime, maintenance,
/// person use `1 << 40` to `1 << 42`).
const QUALITY_STREAMS: u64 = 1 << 43;

/// Defects and measured characteristics a process gives to the items it works on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QualityDef {
    /// Defect types drawn for every item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defects: Vec<DefectDef>,
    /// Characteristics measured on every item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characteristics: Vec<CharacteristicDef>,
    /// Value this process adds to each item: a scrapped item loses the values of every
    /// process it went through (internal failure in the cost of quality). Never read by the
    /// simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_cost: Option<f64>,
    /// Cost of each defect of this process that reaches the customer (external failure:
    /// warranty, return, claim). Never read by the simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape_cost: Option<f64>,
    /// Control plan lines of the measured characteristics (the Qualidade tab of the node,
    /// owner's direction of 2026-11-08). Never read by the simulation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub control: Vec<ControlLineDef>,
}

/// One line of the control plan of a characteristic measured at this process.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlLineDef {
    /// Name of a characteristic of this block.
    pub characteristic: String,
    /// Measurement method and instrument.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub method: String,
    /// The measurement system passed its study (MSA); absent: not studied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msa_ok: Option<bool>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub frequency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<u32>,
    /// Control chart (`imr`, `xbarr`, `xbars`, `p`, `np`, `c`, `u`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chart: String,
    /// What to do on a signal.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reaction: String,
    /// FMEA line it covers (free reference).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fmea: String,
}

/// A defect type: base probability per item, changed by factors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefectDef {
    /// Type name; the item gets the label `defeito.<name>`.
    pub name: String,
    /// Probability per item before the factors, in `[0, 1]`.
    pub probability: f64,
    /// Multipliers of the probability (the result is capped at 1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub factors: Vec<DefectFactor>,
}

/// Condition that multiplies the probability of a defect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DefectFactor {
    /// The item has the numeric label `label` equal to `equals` (operator skill, shift,
    /// material lot...).
    #[serde(alias = "Rotulo")]
    Label {
        label: String,
        equals: f64,
        times: f64,
    },
    /// The first `first` items of every cycle of `every` items of this object (the pieces
    /// right after a setup or a tool change).
    #[serde(alias = "Ciclo")]
    Cycle { every: u64, first: u64, times: f64 },
}

/// A characteristic measured on every item, kept as a numeric label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacteristicDef {
    /// Label name (`diametro`); the instant goes to `<name>@t`.
    pub name: String,
    /// Value of each item before drift and shifts.
    pub value: Dist,
    /// Added to the value per item of the cycle (tool wear).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub drift: f64,
    /// Items per cycle (setup, tool change): the drift starts again and a new `shift`
    /// is drawn. Absent: one cycle for the whole run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u64>,
    /// Offset drawn at the start of every cycle (the setup is never exactly the same).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shift: Option<Dist>,
    /// Lower specification limit. An item outside the limits gets `defeito.<name>` = 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsl: Option<f64>,
    /// Upper specification limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usl: Option<f64>,
}

impl CharacteristicDef {
    pub fn in_spec(&self, x: f64) -> bool {
        self.lsl.is_none_or(|l| x >= l) && self.usl.is_none_or(|u| x <= u)
    }
}

/// What an inspection does with a rejected item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectTo {
    /// Port 2 (rework); port 3 (scrap) once `max_rework` passes are used up.
    #[default]
    #[serde(alias = "Retrabalho")]
    Rework,
    /// Port 2 (scrap).
    #[serde(alias = "Refugo")]
    Scrap,
}

/// Inspection station: writes the output port of each item in a label, read by
/// `output: ByLabel("inspecao")`. Ports: 1 approved, 2 rework (or scrap with
/// `on_reject: Scrap`), 3 scrap after `max_rework` passes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InspectionDef {
    /// Label that receives the port (absent: `inspecao`).
    #[serde(default = "inspection_label")]
    pub label: String,
    /// Inspects one item in every `every` (1: all of them). Uninspected items pass.
    #[serde(default = "one_u64")]
    pub every: u64,
    /// Probability of detecting a defect that is present.
    #[serde(default = "one_f64")]
    pub detect: f64,
    /// Detection probability by defect type, instead of `detect`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub detect_by: BTreeMap<String, f64>,
    /// Probability of rejecting an item without defects (false alarm).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub false_reject: f64,
    /// Characteristics checked by measurement: the measured value (true value plus the
    /// error of the measurement system) is compared with the limits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub measure: Vec<MeasureDef>,
    #[serde(default)]
    pub on_reject: RejectTo,
    /// Rework passes an item may have; a rejected item that used them goes to scrap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rework: Option<u32>,
    /// Cost of inspecting one item (appraisal in the cost of quality). Never read by the
    /// simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}

impl Default for InspectionDef {
    fn default() -> Self {
        InspectionDef {
            label: inspection_label(),
            every: 1,
            detect: 1.0,
            detect_by: BTreeMap::new(),
            false_reject: 0.0,
            measure: Vec::new(),
            on_reject: RejectTo::Rework,
            max_rework: None,
            cost: None,
        }
    }
}

/// Port written by an inspection.
pub const PORT_APPROVED: f64 = 1.0;
pub const PORT_REJECTED: f64 = 2.0;
pub const PORT_SCRAP_AFTER_REWORK: f64 = 3.0;

impl InspectionDef {
    /// Highest port the inspection may write.
    pub fn ports(&self) -> usize {
        match (self.on_reject, self.max_rework) {
            (RejectTo::Rework, Some(_)) => 3,
            _ => 2,
        }
    }
}

/// A characteristic checked by measurement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasureDef {
    pub characteristic: String,
    /// Standard deviation of the measurement system (from the MSA).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub sigma: f64,
}

/// Rework station: counts the pass and fixes each defect with a probability.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReworkDef {
    /// Probability of fixing a defect that is present.
    #[serde(default = "one_f64")]
    pub fix: f64,
    /// Fix probability by defect type, instead of `fix`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fix_by: BTreeMap<String, f64>,
    /// Cost of one rework pass (internal failure). Never read by the simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}

impl Default for ReworkDef {
    fn default() -> Self {
        ReworkDef {
            fix: 1.0,
            fix_by: BTreeMap::new(),
            cost: None,
        }
    }
}

/// Role of a sink in the quality reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SinkRole {
    /// Scrapped items (internal failure).
    #[serde(alias = "Refugo")]
    Scrap,
    /// Items delivered: defects that reach it escaped (external failure).
    #[serde(alias = "Cliente")]
    Customer,
}

/// Kind of risk zone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneKind {
    /// Vehicle traffic (forklifts, tuggers).
    #[default]
    #[serde(alias = "Veiculo")]
    Vehicle,
    /// Reach of a machine.
    #[serde(alias = "Maquina")]
    Machine,
    #[serde(alias = "CargaSuspensa")]
    SuspendedLoad,
    #[serde(alias = "Ruido")]
    Noise,
    #[serde(alias = "Calor")]
    Heat,
    #[serde(alias = "Outro")]
    Other,
}

/// Risk zone: a polygon on the floor. The exposure analysis measures the time people spend
/// inside it; the simulation reads only its `ppe`, for operation rules with `require_ppe`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZoneDef {
    pub name: String,
    #[serde(default)]
    pub kind: ZoneKind,
    /// Vertices `(x, y)` in metres, in order.
    pub polygon: Vec<[f64; 2]>,
    /// Level of the risk matrix (1 lowest), for the colour in 3D.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// Protective equipment required to enter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ppe: Vec<String>,
}

/// Protective equipment names the 3D client knows how to dress (others are kept and
/// listed, but not drawn).
pub const PPE_NAMES: &[&str] = &[
    "capacete",
    "oculos",
    "protetor_auricular",
    "luvas",
    "botina",
    "colete",
    "mascara",
    "avental",
];

fn inspection_label() -> String {
    INSPECTION_LABEL.to_string()
}

fn one_u64() -> u64 {
    1
}

fn one_f64() -> f64 {
    1.0
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Problems of the quality blocks of one object, for `Model::validate`.
pub(crate) fn validate(
    name: &str,
    quality: Option<&QualityDef>,
    inspection: Option<&InspectionDef>,
    rework: Option<&ReworkDef>,
    outputs: usize,
    errors: &mut Vec<String>,
) {
    let prob = |what: &str, p: f64, errors: &mut Vec<String>| {
        if !(0.0..=1.0).contains(&p) || p.is_nan() {
            errors.push(format!("{name}: {what} deve estar entre 0 e 1 (está {p})"));
        }
    };
    if let Some(q) = quality {
        for d in &q.defects {
            if d.name.is_empty() {
                errors.push(format!("{name}: defeito sem nome"));
            }
            prob(
                &format!("probabilidade do defeito {}", d.name),
                d.probability,
                errors,
            );
            for f in &d.factors {
                match f {
                    DefectFactor::Label { times, .. } | DefectFactor::Cycle { times, .. }
                        if times.is_nan() || *times < 0.0 =>
                    {
                        errors.push(format!("{name}: fator do defeito {} negativo", d.name));
                    }
                    DefectFactor::Cycle { every, first, .. } if *every == 0 || first > every => {
                        errors.push(format!(
                            "{name}: ciclo do defeito {} precisa de every > 0 e first <= every",
                            d.name
                        ));
                    }
                    _ => {}
                }
            }
        }
        for c in &q.characteristics {
            if c.name.is_empty() {
                errors.push(format!("{name}: característica sem nome"));
            }
            if let (Some(l), Some(u)) = (c.lsl, c.usl) {
                if l > u {
                    errors.push(format!("{name}: {}: lsl maior que usl", c.name));
                }
            }
            if c.cycle == Some(0) {
                errors.push(format!("{name}: {}: cycle deve ser maior que 0", c.name));
            }
        }
    }
    if let Some(i) = inspection {
        if i.every == 0 {
            errors.push(format!("{name}: inspection.every deve ser maior que 0"));
        }
        prob("inspection.detect", i.detect, errors);
        prob("inspection.false_reject", i.false_reject, errors);
        for (k, p) in &i.detect_by {
            prob(&format!("inspection.detect_by[{k}]"), *p, errors);
        }
        if outputs < i.ports() {
            errors.push(format!(
                "{name}: a inspeção precisa de {} saídas (aprovado, {}), tem {outputs}",
                i.ports(),
                match (i.on_reject, i.max_rework) {
                    (RejectTo::Rework, Some(_)) => "retrabalho, refugo",
                    (RejectTo::Rework, None) => "retrabalho",
                    (RejectTo::Scrap, _) => "refugo",
                }
            ));
        }
    }
    if let Some(r) = rework {
        prob("rework.fix", r.fix, errors);
        for (k, p) in &r.fix_by {
            prob(&format!("rework.fix_by[{k}]"), *p, errors);
        }
    }
}

/// Entry hook of an object with quality blocks: draws defects and characteristics,
/// reworks and inspects, in this order, after the object's own `on_entry` script.
pub(crate) struct QualityHook {
    pub quality: Option<QualityDef>,
    pub inspection: Option<InspectionDef>,
    pub rework: Option<ReworkDef>,
    pub script: Option<Box<dyn ItemScript>>,
    rng: SimRng,
    /// Items seen by this object.
    count: u64,
    /// Per characteristic: offset of the current cycle.
    shifts: Vec<f64>,
}

impl QualityHook {
    pub fn new(
        seed: u64,
        replication: u32,
        block: usize,
        quality: Option<QualityDef>,
        inspection: Option<InspectionDef>,
        rework: Option<ReworkDef>,
        script: Option<Box<dyn ItemScript>>,
    ) -> Self {
        let n = quality.as_ref().map_or(0, |q| q.characteristics.len());
        QualityHook {
            quality,
            inspection,
            rework,
            script,
            rng: rng::stream(seed, replication, QUALITY_STREAMS + block as u64),
            count: 0,
            shifts: vec![0.0; n],
        }
    }

    fn uniform(&mut self) -> f64 {
        Dist::Uniform { min: 0.0, max: 1.0 }.sample(&mut self.rng)
    }

    fn generate(&mut self, item: &mut Item, now: f64) {
        let Some(q) = self.quality.take() else {
            return;
        };
        let k = self.count; // 0-based index of this item at the object
        for d in &q.defects {
            let mut p = d.probability;
            for f in &d.factors {
                match f {
                    DefectFactor::Label {
                        label,
                        equals,
                        times,
                    } => {
                        if item.label(label).and_then(Value::as_num) == Some(*equals) {
                            p *= times;
                        }
                    }
                    DefectFactor::Cycle {
                        every,
                        first,
                        times,
                    } => {
                        if *every > 0 && k % every < *first {
                            p *= times;
                        }
                    }
                }
            }
            let p = p.clamp(0.0, 1.0);
            // Always draw, so one defect does not shift the draws of the next.
            let u = self.uniform();
            if u < p {
                item.set_label(format!("{DEFECT_PREFIX}{}", d.name), 1.0);
            }
        }
        for (j, c) in q.characteristics.iter().enumerate() {
            let in_cycle = c.cycle.map_or(k, |n| k % n.max(1));
            if in_cycle == 0 {
                self.shifts[j] = c.shift.as_ref().map_or(0.0, |s| s.sample(&mut self.rng));
            }
            let x = c.value.sample(&mut self.rng) + self.shifts[j] + c.drift * in_cycle as f64;
            item.set_label(c.name.clone(), x);
            item.set_label(format!("{}{INSTANT_SUFFIX}", c.name), now);
            if !c.in_spec(x) {
                item.set_label(format!("{DEFECT_PREFIX}{}", c.name), 1.0);
            }
        }
        self.quality = Some(q);
        recount(item);
    }

    fn rework(&mut self, item: &mut Item) {
        let Some(r) = self.rework.take() else {
            return;
        };
        let passes = item
            .label(PASSES_LABEL)
            .and_then(Value::as_num)
            .unwrap_or(0.0);
        item.set_label(PASSES_LABEL, passes + 1.0);
        for name in present(item) {
            let p = r.fix_by.get(&name).copied().unwrap_or(r.fix);
            if self.uniform() < p {
                item.set_label(format!("{DEFECT_PREFIX}{name}"), 0.0);
                // A characteristic out of spec is brought back to the target.
                if let Some(Value::Num(_)) = item.label(&name) {
                    if let Some(t) = item_spec(item, &name).and_then(target) {
                        item.set_label(name.clone(), t);
                    }
                }
            }
        }
        self.rework = Some(r);
        recount(item);
    }

    fn inspect(&mut self, item: &mut Item) {
        let Some(i) = self.inspection.take() else {
            return;
        };
        let sampled = self.count % i.every.max(1) == 0;
        let mut rejected = false;
        if sampled {
            let defects = present(item);
            let measured: Vec<&str> = i
                .measure
                .iter()
                .map(|m| m.characteristic.as_str())
                .collect();
            for name in defects.iter().filter(|d| !measured.contains(&d.as_str())) {
                let p = i.detect_by.get(name).copied().unwrap_or(i.detect);
                if self.uniform() < p {
                    rejected = true;
                }
            }
            for m in &i.measure {
                let Some(x) = item.label(&m.characteristic).and_then(Value::as_num) else {
                    continue;
                };
                let error = if m.sigma > 0.0 {
                    Dist::Normal {
                        mean: 0.0,
                        sd: m.sigma,
                    }
                    .sample(&mut self.rng)
                } else {
                    0.0
                };
                let spec = item_spec(item, &m.characteristic);
                if let Some((lsl, usl)) = spec {
                    let y = x + error;
                    if lsl.is_some_and(|l| y < l) || usl.is_some_and(|u| y > u) {
                        rejected = true;
                    }
                }
            }
            if defects.is_empty() && i.false_reject > 0.0 && self.uniform() < i.false_reject {
                rejected = true;
            }
        }
        let port = if !rejected {
            PORT_APPROVED
        } else {
            match (i.on_reject, i.max_rework) {
                (RejectTo::Rework, Some(max)) => {
                    let passes = item
                        .label(PASSES_LABEL)
                        .and_then(Value::as_num)
                        .unwrap_or(0.0);
                    if passes >= f64::from(max) {
                        PORT_SCRAP_AFTER_REWORK
                    } else {
                        PORT_REJECTED
                    }
                }
                _ => PORT_REJECTED,
            }
        };
        item.set_label(i.label.clone(), port);
        self.inspection = Some(i);
    }
}

/// Specification limits of a characteristic, as written on the item by the process that
/// measured it (`<name>@lsl`, `<name>@usl`).
fn item_spec(item: &Item, name: &str) -> Option<(Option<f64>, Option<f64>)> {
    let lsl = item.label(&format!("{name}@lsl")).and_then(Value::as_num);
    let usl = item.label(&format!("{name}@usl")).and_then(Value::as_num);
    (lsl.is_some() || usl.is_some()).then_some((lsl, usl))
}

/// Value a rework brings an out-of-spec item to: the middle of the limits (or the one
/// limit there is).
fn target(spec: (Option<f64>, Option<f64>)) -> Option<f64> {
    match spec {
        (Some(l), Some(u)) => Some((l + u) / 2.0),
        (Some(l), None) => Some(l),
        (None, Some(u)) => Some(u),
        (None, None) => None,
    }
}

/// Names of the defect types present on the item (`defeito.<name>` = 1).
pub fn present(item: &Item) -> Vec<String> {
    present_in(&item.labels)
}

/// Like [`present`], on a label map (item log records).
pub fn present_in(labels: &BTreeMap<String, Value>) -> Vec<String> {
    labels
        .iter()
        .filter_map(|(k, v)| {
            let name = k.strip_prefix(DEFECT_PREFIX)?;
            (v.as_num().unwrap_or(0.0) > 0.0).then(|| name.to_string())
        })
        .collect()
}

fn recount(item: &mut Item) {
    let n = present(item).len();
    item.set_label(DEFECTS_LABEL, n as f64);
}

impl ItemScript for QualityHook {
    fn run(&mut self, item: &mut Item, env: &mut ScriptEnv) -> Result<(), String> {
        if let Some(s) = self.script.as_mut() {
            s.run(item, env)?;
        }
        self.generate(item, env.now);
        // The limits travel with the item, so an inspection elsewhere can measure it.
        if let Some(q) = &self.quality {
            for c in &q.characteristics {
                if let Some(l) = c.lsl {
                    item.set_label(format!("{}@lsl", c.name), l);
                }
                if let Some(u) = c.usl {
                    item.set_label(format!("{}@usl", c.name), u);
                }
            }
        }
        self.rework(item);
        self.inspect(item);
        self.count += 1;
        Ok(())
    }
}

/// A sink with a quality role (`sink_role`): the inner sink plus counts of what arrives,
/// reported as custom metrics of the block, so they reach `RunReport::metrics`
/// (`<sink>.pecas_defeituosas`, `<sink>.pct_defeituosas`, `<sink>.retrabalhadas`,
/// `<sink>.defeitos.<tipo>`). Counted in the statistics window only, like `input`.
pub(crate) struct QualitySink {
    inner: Box<dyn utsim_core::Behavior>,
    entradas: u64,
    defeituosas: u64,
    retrabalhadas: u64,
    por_tipo: BTreeMap<String, u64>,
}

impl QualitySink {
    pub fn new(inner: Box<dyn utsim_core::Behavior>) -> Self {
        QualitySink {
            inner,
            entradas: 0,
            defeituosas: 0,
            retrabalhadas: 0,
            por_tipo: BTreeMap::new(),
        }
    }
}

impl utsim_core::Behavior for QualitySink {
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }

    fn reset(&mut self, cx: &mut utsim_core::Ctx) {
        self.inner.reset(cx);
    }

    fn accepts(&self, item: &Item, content: usize, port: usize) -> bool {
        self.inner.accepts(item, content, port)
    }

    fn accepts_is_item_independent(&self) -> bool {
        self.inner.accepts_is_item_independent()
    }

    fn on_entry(&mut self, cx: &mut utsim_core::Ctx, item: utsim_core::ItemId, port: usize) {
        let it = cx.item(item);
        self.entradas += 1;
        let tipos = present(it);
        if !tipos.is_empty() {
            self.defeituosas += 1;
        }
        for t in tipos {
            *self.por_tipo.entry(t).or_default() += 1;
        }
        if it
            .label(PASSES_LABEL)
            .and_then(Value::as_num)
            .unwrap_or(0.0)
            > 0.0
        {
            self.retrabalhadas += 1;
        }
        self.inner.on_entry(cx, item, port);
    }

    fn on_exit(&mut self, cx: &mut utsim_core::Ctx, item: utsim_core::ItemId) {
        self.inner.on_exit(cx, item);
    }

    fn on_event(&mut self, cx: &mut utsim_core::Ctx, tag: u32, item: Option<utsim_core::ItemId>) {
        self.inner.on_event(cx, tag, item);
    }

    fn on_stop(&mut self, cx: &mut utsim_core::Ctx) {
        self.inner.on_stop(cx);
    }

    fn on_resume(&mut self, cx: &mut utsim_core::Ctx) {
        self.inner.on_resume(cx);
    }

    fn on_task(&mut self, cx: &mut utsim_core::Ctx, request: utsim_core::TaskRequest) {
        self.inner.on_task(cx, request);
    }

    fn work_in_progress(&self, cx: &utsim_core::Ctx) -> bool {
        self.inner.work_in_progress(cx)
    }

    fn on_warmup(&mut self, cx: &mut utsim_core::Ctx) {
        self.entradas = 0;
        self.defeituosas = 0;
        self.retrabalhadas = 0;
        self.por_tipo.clear();
        self.inner.on_warmup(cx);
    }

    fn custom_metrics(&self, now: f64) -> Vec<(String, f64)> {
        let mut v = self.inner.custom_metrics(now);
        v.push(("pecas_defeituosas".into(), self.defeituosas as f64));
        v.push((
            "pct_defeituosas".into(),
            if self.entradas > 0 {
                self.defeituosas as f64 / self.entradas as f64
            } else {
                0.0
            },
        ));
        v.push(("retrabalhadas".into(), self.retrabalhadas as f64));
        for (t, n) in &self.por_tipo {
            v.push((format!("defeitos.{t}"), *n as f64));
        }
        v
    }

    fn on_message(
        &mut self,
        cx: &mut utsim_core::Ctx,
        from: utsim_core::BlockId,
        tag: u32,
        payload: u64,
    ) {
        self.inner.on_message(cx, from, tag, payload);
    }

    fn restore(
        &mut self,
        cx: &mut utsim_core::Ctx,
        state: &utsim_core::BlockState,
    ) -> Result<(), String> {
        self.inner.restore(cx, state)
    }

    fn save_state(
        &self,
        cx: &utsim_core::Ctx,
        state: &mut utsim_core::BlockState,
    ) -> Option<String> {
        self.inner.save_state(cx, state)
    }
}
