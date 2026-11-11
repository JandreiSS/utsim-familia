//! Physical characteristics of people: fatigue and recovery, learning curve, walking
//! speed with a load, skills, individual pace and the manual lifting reference.
//!
//! Everything here is optional and off by default. A mobile resource (a
//! [`Transporter`](../../utsim_objects/struct.Transporter.html) used as a person) with a
//! [`Person`] profile (set with [`crate::Simulation::set_person`]) has its state updated
//! **only at events** (state changes of the person, start of a task), by closed formulas,
//! never by a time step. A block without a profile behaves exactly as before.
//!
//! - **Fatigue** ([`Fatigue`]). `Allowance(p)`: the rest allowance of work measurement, a
//!   fixed fraction that multiplies the operation times by `1 + p`, with no state.
//!   `Dynamic`: a level `F ∈ [0, 1)` that grows exponentially while working,
//!   `F(t) = 1 − (1 − F0)·e^(−λ·t)`, and falls while resting, `F(t) = F0·e^(−μ·t)`. The
//!   rate `λ` depends on the activity: work at a station or handling (`work`), walking
//!   empty (`walk`), walking with a load (`walk + carry·kg`). Every other state (idle, off
//!   shift, pause, broken, waiting) recovers with `μ` (`recovery`). The operation times
//!   started at level `F` are multiplied by `1 + β·F` (`time_factor`) and the walking speed
//!   by `1 − α·F` (`speed_factor`). With `limit`, a person whose level reached the limit
//!   takes a pause ([`crate::State::Resting`]) when it finishes its task, long enough to
//!   come back to `target`: `ln(F/target)/μ`.
//! - **Learning** ([`Learning`]). The `n`-th repetition of a task by the same person takes
//!   `max(first·n^(−b), 1)` times the standard time, with `b = −log₂ r` (Wright's curve,
//!   rate `r`, 85 % by default). Each station (and its setup), loading and unloading is a
//!   task of its own.
//! - **Walking with a load** ([`LoadedWalk`]). The speed drops by `per_kg` per kg above
//!   `above` kg, down to `min`; the weight of an item is its label `weight_label`.
//! - **Skills.** A request may name a skill ([`crate::Ctx::request_task_requiring`]);
//!   dispatchers give it only to members whose profile lists it ([`crate::Ctx::qualified`]).
//! - **Individual pace** ([`Variation`]). A multiplier `v` drawn at the start of each
//!   replication from the person's own random stream (so turning it on never shifts the
//!   other draws): times are divided by `v`, the speed multiplied by `v`.
//! - **Manual lifting** ([`Lift`], [`recommended_weight_limit`]): a model check only, the
//!   revised NIOSH lifting equation; the kernel never reads it.
//! - **Who the person is** ([`Sex`], `height_m`, `mass_kg`, `age`, `experience_years`,
//!   [`Capacity`]): records. The 3D body uses sex, height and mass; the measured load
//!   capacity enters the model warnings. The kernel never reads them.
//!
//! The metrics go to the block's `custom` metrics (see [`METRICS`]).

use serde::{Deserialize, Serialize};

use crate::dist::Dist;

/// Load constant of the revised NIOSH lifting equation (kg): the recommended weight
/// limit under ideal conditions (Waters et al., 1993).
pub const NIOSH_LOAD_CONSTANT: f64 = 23.0;

/// Task key of the loading of items by a carrier (learning curve).
pub const TASK_LOAD: &str = "carga";
/// Task key of the unloading of items by a carrier (learning curve).
pub const TASK_UNLOAD: &str = "descarga";

/// Profile of a person (see the module docs). Every part is optional.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Person {
    #[serde(skip_serializing_if = "Option::is_none", alias = "fadiga")]
    pub fatigue: Option<Fatigue>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "aprendizado")]
    pub learning: Option<Learning>,
    /// What this person is qualified for ("torno", "empilhadeira"...).
    #[serde(skip_serializing_if = "Vec::is_empty", alias = "habilidades")]
    pub skills: Vec<String>,
    /// Walking speed with a load.
    #[serde(skip_serializing_if = "Option::is_none", alias = "caminhada_carregado")]
    pub walk_speed_loaded: Option<LoadedWalk>,
    /// Individual pace drawn once per replication.
    #[serde(skip_serializing_if = "Option::is_none", alias = "variacao")]
    pub variation: Option<Variation>,
    /// Label with the weight of an item, in kg.
    #[serde(skip_serializing_if = "is_weight", alias = "rotulo_peso")]
    pub weight_label: String,
    /// Geometry of the manual lift, for the NIOSH check (model warnings only).
    #[serde(skip_serializing_if = "Option::is_none", alias = "levantamento")]
    pub lift: Option<Lift>,
    /// Sex (`None`: not informed). Chooses the 3D body; no fatigue default depends on it.
    #[serde(skip_serializing_if = "Option::is_none", alias = "sexo")]
    pub sex: Option<Sex>,
    /// Height in metres (the 3D body is scaled to it).
    #[serde(skip_serializing_if = "Option::is_none", alias = "altura_m")]
    pub height_m: Option<f64>,
    /// Body mass in kg (record; the 3D body's width follows the body mass index).
    #[serde(skip_serializing_if = "Option::is_none", alias = "massa_kg")]
    pub mass_kg: Option<f64>,
    /// Age in years (record only).
    #[serde(skip_serializing_if = "Option::is_none", alias = "idade")]
    pub age: Option<u32>,
    /// Years of experience in the job (record only).
    #[serde(skip_serializing_if = "Option::is_none", alias = "experiencia_anos")]
    pub experience_years: Option<f64>,
    /// Measured physical capacity of this person.
    #[serde(skip_serializing_if = "Option::is_none", alias = "capacidade")]
    pub capacity: Option<Capacity>,
}

/// Sex of a person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    #[serde(alias = "Feminino")]
    Female,
    #[serde(alias = "Masculino")]
    Male,
}

/// Measured capacity of a person. Every part is optional: what was not measured is not
/// assumed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capacity {
    /// Heaviest load this person lifts in the job, in kg.
    #[serde(skip_serializing_if = "Option::is_none", alias = "carga_kg")]
    pub load_kg: Option<f64>,
    /// Greatest force this person exerts (push or pull), in N.
    #[serde(skip_serializing_if = "Option::is_none", alias = "forca_n")]
    pub force_n: Option<f64>,
}

fn is_weight(label: &String) -> bool {
    label == "weight"
}

impl Default for Person {
    fn default() -> Self {
        Person {
            fatigue: None,
            learning: None,
            skills: Vec::new(),
            walk_speed_loaded: None,
            variation: None,
            weight_label: "weight".into(),
            lift: None,
            sex: None,
            height_m: None,
            mass_kg: None,
            age: None,
            experience_years: None,
            capacity: None,
        }
    }
}

/// Fatigue model.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fatigue {
    /// Rest allowance: the operation times are multiplied by `1 + p` (`0.15` = 15 %).
    #[serde(alias = "Tolerancia")]
    Allowance(f64),
    /// Exponential fatigue and recovery (rates per time unit of the model).
    #[serde(alias = "Dinamica")]
    Dynamic {
        /// `λ` while working at a station, loading or unloading.
        #[serde(default = "work_rate", alias = "trabalho")]
        work: f64,
        /// `λ` while walking empty.
        #[serde(default = "walk_rate", alias = "caminhada")]
        walk: f64,
        /// Extra `λ` per kg carried while walking with a load.
        #[serde(default = "carry_rate", alias = "carga")]
        carry: f64,
        /// `μ` while not working (idle, pause, off shift, stopped).
        #[serde(default = "recovery_rate", alias = "recuperacao")]
        recovery: f64,
        /// `β`: operation times × `1 + β·F`.
        #[serde(default = "time_factor", alias = "fator_tempo")]
        time_factor: f64,
        /// `α`: walking speed × `1 − α·F`.
        #[serde(default = "speed_factor", alias = "fator_velocidade")]
        speed_factor: f64,
        /// Level that triggers a pause at the end of the task (`None`: no automatic pause).
        #[serde(default, skip_serializing_if = "Option::is_none", alias = "limite")]
        limit: Option<f64>,
        /// Level the pause brings the person back to.
        #[serde(default = "target_level", alias = "alvo")]
        target: f64,
    },
}

// Defaults of the dynamic model, per second (estimates; see the manual).
fn work_rate() -> f64 {
    1.0e-4
}
fn walk_rate() -> f64 {
    5.0e-5
}
fn carry_rate() -> f64 {
    5.0e-6
}
fn recovery_rate() -> f64 {
    1.0e-3
}
fn time_factor() -> f64 {
    0.2
}
fn speed_factor() -> f64 {
    0.2
}
fn target_level() -> f64 {
    0.1
}

impl Fatigue {
    /// The dynamic model with every default.
    pub fn dynamic() -> Self {
        Fatigue::Dynamic {
            work: work_rate(),
            walk: walk_rate(),
            carry: carry_rate(),
            recovery: recovery_rate(),
            time_factor: time_factor(),
            speed_factor: speed_factor(),
            limit: None,
            target: target_level(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        match *self {
            Fatigue::Allowance(p) if !(ok(p) && p <= 10.0) => {
                Err("fatigue: Allowance(p) exige p entre 0 e 10 (0.15 = 15 %)".into())
            }
            Fatigue::Allowance(_) => Ok(()),
            Fatigue::Dynamic {
                work,
                walk,
                carry,
                recovery,
                time_factor,
                speed_factor,
                limit,
                target,
            } => {
                if ![work, walk, carry, recovery, time_factor]
                    .into_iter()
                    .all(ok)
                {
                    return Err(
                        "fatigue: work, walk, carry, recovery e time_factor devem ser finitos e >= 0"
                            .into(),
                    );
                }
                if !(ok(speed_factor) && speed_factor < 1.0) {
                    return Err("fatigue: speed_factor deve ficar em [0, 1)".into());
                }
                if let Some(l) = limit {
                    if !(l > 0.0 && l < 1.0) {
                        return Err("fatigue: limit deve ficar em (0, 1)".into());
                    }
                    if !(target > 0.0 && target < l) {
                        return Err("fatigue: target deve ficar em (0, limit)".into());
                    }
                    if recovery <= 0.0 {
                        return Err("fatigue: com limit, recovery deve ser > 0".into());
                    }
                }
                Ok(())
            }
        }
    }
}

/// Wright's learning curve.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Learning {
    /// Learning rate `r`: each doubling of the repetitions multiplies the time by `r`.
    #[serde(alias = "taxa")]
    pub rate: f64,
    /// Time of the first repetition, as a multiple of the standard time (the floor).
    #[serde(alias = "primeira")]
    pub first: f64,
}

impl Default for Learning {
    fn default() -> Self {
        Learning {
            rate: 0.85,
            first: 2.0,
        }
    }
}

impl Learning {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.rate > 0.0 && self.rate <= 1.0) {
            return Err("learning: rate deve ficar em (0, 1] (0.85 = 85 %)".into());
        }
        if !(self.first.is_finite() && self.first >= 1.0) {
            return Err("learning: first deve ser finito e >= 1".into());
        }
        Ok(())
    }

    /// Exponent `b = −log₂ r`.
    pub fn exponent(&self) -> f64 {
        -self.rate.log2()
    }

    /// Multiplier of the standard time for the `n`-th repetition (`n >= 1`).
    pub fn factor(&self, n: u64) -> f64 {
        let n = n.max(1) as f64;
        (self.first * n.powf(-self.exponent())).max(1.0)
    }
}

/// Walking speed with a load: `speed·(1 − per_kg·(kg − above))`, not below `min` (nor
/// above the empty speed).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoadedWalk {
    #[serde(alias = "por_kg")]
    pub per_kg: f64,
    #[serde(alias = "acima_de")]
    pub above: f64,
    #[serde(alias = "minima")]
    pub min: f64,
}

impl Default for LoadedWalk {
    fn default() -> Self {
        LoadedWalk {
            per_kg: 0.01,
            above: 5.0,
            min: 0.8,
        }
    }
}

impl LoadedWalk {
    pub fn validate(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        if ok(self.per_kg) && ok(self.above) && ok(self.min) {
            Ok(())
        } else {
            Err("walk_speed_loaded: per_kg, above e min devem ser finitos e >= 0".into())
        }
    }

    /// Speed with `kg` carried, from the empty speed `speed`.
    pub fn speed(&self, speed: f64, kg: f64) -> f64 {
        let excess = (kg - self.above).max(0.0);
        let reduced = speed * (1.0 - self.per_kg * excess);
        reduced.max(self.min.min(speed))
    }
}

/// Individual pace: a draw of `dist`, truncated to `[min, max]` (redrawn when outside;
/// clamped after 1000 tries).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Variation {
    #[serde(alias = "dist")]
    pub dist: Dist,
    #[serde(alias = "minimo")]
    pub min: f64,
    #[serde(alias = "maximo")]
    pub max: f64,
}

impl Default for Variation {
    fn default() -> Self {
        Variation {
            dist: Dist::Normal { mean: 1.0, sd: 0.1 },
            min: 0.7,
            max: 1.3,
        }
    }
}

impl Variation {
    pub fn validate(&self) -> Result<(), String> {
        self.dist
            .validate()
            .map_err(|e| format!("variation.dist: {e}"))?;
        if !(self.min.is_finite() && self.max.is_finite() && self.min > 0.0 && self.min <= self.max)
        {
            return Err("variation: exige 0 < min <= max, finitos".into());
        }
        Ok(())
    }

    pub fn draw(&self, rng: &mut crate::rng::SimRng) -> f64 {
        let mut x = self.dist.sample(rng);
        for _ in 0..1000 {
            if (self.min..=self.max).contains(&x) {
                return x;
            }
            x = self.dist.sample(rng);
        }
        x.clamp(self.min, self.max)
    }
}

/// Geometry of a manual lift for the revised NIOSH lifting equation (metric form):
/// distances in cm, angle in degrees; the frequency and coupling multipliers come from the
/// tables of the publication and are given directly (1 = best case).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lift {
    /// `H`: horizontal distance from the ankles to the hands (cm).
    #[serde(alias = "horizontal")]
    pub horizontal: f64,
    /// `V`: height of the hands at the origin (cm).
    #[serde(alias = "vertical")]
    pub vertical: f64,
    /// `D`: vertical travel of the load (cm).
    #[serde(alias = "deslocamento")]
    pub distance: f64,
    /// `A`: asymmetry (trunk twist) angle (degrees).
    #[serde(alias = "assimetria")]
    pub asymmetry: f64,
    /// `FM`: frequency multiplier (table of the publication).
    #[serde(alias = "frequencia")]
    pub frequency: f64,
    /// `CM`: coupling multiplier (table of the publication).
    #[serde(alias = "pega")]
    pub coupling: f64,
}

impl Default for Lift {
    fn default() -> Self {
        Lift {
            horizontal: 25.0,
            vertical: 75.0,
            distance: 25.0,
            asymmetry: 0.0,
            frequency: 1.0,
            coupling: 1.0,
        }
    }
}

impl Lift {
    pub fn validate(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        if ![
            self.horizontal,
            self.vertical,
            self.distance,
            self.asymmetry,
        ]
        .into_iter()
        .all(ok)
        {
            return Err("lift: distâncias e ângulo devem ser finitos e >= 0".into());
        }
        if !(ok(self.frequency)
            && self.frequency <= 1.0
            && ok(self.coupling)
            && self.coupling <= 1.0)
        {
            return Err("lift: frequency e coupling devem ficar em [0, 1]".into());
        }
        Ok(())
    }

    /// The six multipliers `(HM, VM, DM, AM, FM, CM)`.
    pub fn multipliers(&self) -> [f64; 6] {
        let hm = if self.horizontal > 63.0 {
            0.0
        } else {
            (25.0 / self.horizontal.max(25.0)).min(1.0)
        };
        let vm = if self.vertical > 175.0 {
            0.0
        } else {
            (1.0 - 0.003 * (self.vertical - 75.0).abs()).max(0.0)
        };
        let dm = if self.distance > 175.0 {
            0.0
        } else {
            (0.82 + 4.5 / self.distance.max(25.0)).min(1.0)
        };
        let am = if self.asymmetry > 135.0 {
            0.0
        } else {
            1.0 - 0.0032 * self.asymmetry
        };
        [hm, vm, dm, am, self.frequency, self.coupling]
    }
}

/// Recommended weight limit `RWL = 23 kg × HM × VM × DM × AM × FM × CM`.
pub fn recommended_weight_limit(lift: &Lift) -> f64 {
    NIOSH_LOAD_CONSTANT * lift.multipliers().iter().product::<f64>()
}

/// Lifting index `LI = load / RWL` (infinite when the RWL is 0).
pub fn lifting_index(load_kg: f64, lift: &Lift) -> f64 {
    let rwl = recommended_weight_limit(lift);
    if rwl > 0.0 {
        load_kg / rwl
    } else {
        f64::INFINITY
    }
}

impl Person {
    /// Every problem of the profile.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(f) = &self.fatigue {
            out.extend(f.validate().err());
        }
        if let Some(l) = &self.learning {
            out.extend(l.validate().err());
        }
        if let Some(w) = &self.walk_speed_loaded {
            out.extend(w.validate().err());
        }
        if let Some(v) = &self.variation {
            out.extend(v.validate().err());
        }
        if let Some(l) = &self.lift {
            out.extend(l.validate().err());
        }
        if self.skills.iter().any(|s| s.trim().is_empty()) {
            out.push("skills: os nomes das habilidades não podem ser vazios".into());
        }
        if self.weight_label.trim().is_empty() {
            out.push("weight_label não pode ser vazio".into());
        }
        let positive = |v: Option<f64>| v.is_none_or(|v| v.is_finite() && v > 0.0);
        if !positive(self.height_m) {
            out.push("height_m deve ser finito e > 0 (metros)".into());
        }
        if !positive(self.mass_kg) {
            out.push("mass_kg deve ser finito e > 0 (kg)".into());
        }
        if self.age.is_some_and(|a| a > 120) {
            out.push("age deve ficar entre 0 e 120 anos".into());
        }
        if self
            .experience_years
            .is_some_and(|e| !(e.is_finite() && e >= 0.0))
        {
            out.push("experience_years deve ser finito e >= 0".into());
        }
        if let Some(c) = &self.capacity {
            if !positive(c.load_kg) {
                out.push("capacity.load_kg deve ser finito e > 0 (kg)".into());
            }
            if !positive(c.force_n) {
                out.push("capacity.force_n deve ser finito e > 0 (N)".into());
            }
        }
        out
    }

    pub fn has_skill(&self, skill: &str) -> bool {
        self.skills.iter().any(|s| s == skill)
    }
}

/// Fatigue after `dt` of work at rate `λ` from `f0`, and its integral over `dt`.
pub fn fatigue_work(f0: f64, lambda: f64, dt: f64) -> (f64, f64) {
    if lambda <= 0.0 || dt <= 0.0 {
        return (f0, f0 * dt.max(0.0));
    }
    let e = (-lambda * dt).exp();
    (1.0 - (1.0 - f0) * e, dt - (1.0 - f0) * (1.0 - e) / lambda)
}

/// Fatigue after `dt` of rest at rate `μ` from `f0`, and its integral over `dt`.
pub fn fatigue_rest(f0: f64, mu: f64, dt: f64) -> (f64, f64) {
    if mu <= 0.0 || dt <= 0.0 {
        return (f0, f0 * dt.max(0.0));
    }
    let e = (-mu * dt).exp();
    (f0 * e, f0 * (1.0 - e) / mu)
}

/// Rest needed to go from `f` down to `target` at rate `μ`.
pub fn rest_duration(f: f64, target: f64, mu: f64) -> f64 {
    if f <= target || mu <= 0.0 || target <= 0.0 {
        0.0
    } else {
        (f / target).ln() / mu
    }
}

/// Names of the metrics a person with a profile adds to its `custom` metrics (the fatigue
/// ones only with the dynamic model, the learning ones only with a learning curve).
pub const METRICS: &[&str] = &[
    "person.pace",
    "person.fatigue_mean",
    "person.fatigue_max",
    "person.fatigue_end",
    "person.time_above_limit",
    "person.rests",
    "person.rest_time",
    "person.break_time",
    "person.repetitions",
    "person.learning_factor",
    "person.learning_gain",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wright_curve_eight_repetitions() {
        // T_8 = T_1 · r³ with r = 0.85 (8 = 2³), while above the floor.
        let l = Learning {
            rate: 0.85,
            first: 3.0,
        };
        let t1 = l.factor(1);
        let t8 = l.factor(8);
        assert!((t1 - 3.0).abs() < 1e-12);
        assert!((t8 - 3.0 * 0.85f64.powi(3)).abs() < 1e-12);
        assert!((l.factor(2) - 3.0 * 0.85).abs() < 1e-12);
        // Floor at the standard time.
        assert_eq!(l.factor(10_000), 1.0);
    }

    #[test]
    fn closed_forms_match_by_hand() {
        // λ = 0.01, 50 time units from F0 = 0.2: F = 1 − 0.8·e^(−0.5).
        let (f, area) = fatigue_work(0.2, 0.01, 50.0);
        let e = (-0.5f64).exp();
        assert!((f - (1.0 - 0.8 * e)).abs() < 1e-12);
        assert!((area - (50.0 - 0.8 * (1.0 - e) / 0.01)).abs() < 1e-9);
        // μ = 0.02, 30 from 0.6: F = 0.6·e^(−0.6).
        let (g, area) = fatigue_rest(0.6, 0.02, 30.0);
        assert!((g - 0.6 * (-0.6f64).exp()).abs() < 1e-12);
        assert!((area - 0.6 * (1.0 - (-0.6f64).exp()) / 0.02).abs() < 1e-9);
        // Pause from 0.6 to 0.1 at μ = 0.02: ln 6 / 0.02.
        assert!((rest_duration(0.6, 0.1, 0.02) - 6f64.ln() / 0.02).abs() < 1e-12);
        let (back, _) = fatigue_rest(0.6, 0.02, rest_duration(0.6, 0.1, 0.02));
        assert!((back - 0.1).abs() < 1e-12);
    }

    #[test]
    fn niosh_by_hand() {
        // Ideal geometry: RWL = 23 kg.
        assert!((recommended_weight_limit(&Lift::default()) - 23.0).abs() < 1e-12);
        // H = 40, V = 30, D = 50, A = 45, FM = 0.94, CM = 0.95.
        let lift = Lift {
            horizontal: 40.0,
            vertical: 30.0,
            distance: 50.0,
            asymmetry: 45.0,
            frequency: 0.94,
            coupling: 0.95,
        };
        let by_hand = 23.0 * 0.625 * 0.865 * 0.91 * 0.856 * 0.94 * 0.95;
        assert!((recommended_weight_limit(&lift) - by_hand).abs() < 1e-9);
        assert!((lifting_index(by_hand * 2.0, &lift) - 2.0).abs() < 1e-9);
        // Beyond the limits of the equation the RWL is 0.
        let far = Lift {
            horizontal: 70.0,
            ..Lift::default()
        };
        assert_eq!(recommended_weight_limit(&far), 0.0);
    }

    #[test]
    fn loaded_walk() {
        let w = LoadedWalk::default();
        assert_eq!(w.speed(1.3, 3.0), 1.3);
        assert!((w.speed(1.3, 25.0) - 1.3 * 0.8).abs() < 1e-12);
        assert_eq!(w.speed(1.3, 60.0), 0.8);
        // Never faster than empty.
        assert_eq!(w.speed(0.5, 60.0), 0.5);
    }
}
