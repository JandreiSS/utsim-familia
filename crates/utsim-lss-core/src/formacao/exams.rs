//! Exams (`docs/plano_lean_seis_sigma.md`, 8.5; decision MB3): a question bank per module,
//! an exam drawn with a recorded seed (the same seed, the same exam), the data of the
//! practical problems varied per trainee by the seed, and the grading.
//!
//! - **Questions** live with their answer key in the restricted folder
//!   `formacao/questoes.lss` (MBB and instructors). Four kinds: multiple choice, true or
//!   false, a number with a tolerance and a **practical problem**: a data set and a
//!   question whose expected answer is computed by the common crates (`utsim-stats`,
//!   `utsim-quality`) from the data of the paper, never typed, so it never differs from
//!   what the tool shows.
//! - An **exam** of a class records the modules, how many questions of each difficulty,
//!   the seed and the questions drawn. It is applied in the trainee's app (exam mode) or on
//!   paper. In the app the class folder `formacao/turmas/<turma>.lss` gets one **paper**
//!   per trainee *without the key*; the trainee's answers come back as the trainee's own
//!   operations in that folder and are graded on the instructor's machine, which rebuilds
//!   the paper from the seed and the bank. On paper the instructor records the grade.
//! - The **results** go to `formacao/notas.lss` (MBB and instructors). The minimum grade
//!   is the exam's (70 % by default) and one reapplication is allowed.
//!
//! Without a server, an exam in the app does not prevent consultation; the manual says so
//! and the company decides whether the exam counts for certification (MB3).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{DEFAULT_MIN_GRADE, fields as tf, ids_in, ids_value};
use crate::date::Date;
use crate::domain::enum_value;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::portfolio;
use crate::programa;
use crate::state::{Entity, State};

pub mod fields {
    pub const MODULE: &str = "modulo";
    pub const QKIND: &str = "tipo_questao";
    pub const STATEMENT: &str = "enunciado";
    pub const OPTIONS: &str = "alternativas";
    pub const DIFFICULTY: &str = "dificuldade";
    /// The answer key: an option index, a boolean or a number (restricted folder only).
    pub const KEY: &str = "gabarito";
    pub const TOLERANCE: &str = "tolerancia";
    /// The recipe of a practical problem ([`super::Problem`], JSON).
    pub const PROBLEM: &str = "problema";
    // Exam.
    pub const SEED: &str = "semente";
    /// `quantidade_<1|2|3>`: questions of each difficulty.
    pub const COUNT_PREFIX: &str = "quantidade_";
    pub const QUESTIONS: &str = "questoes";
    pub const MODE: &str = "aplicacao";
    pub const RETAKE_OF: &str = "reaplicacao_de";
    pub const COUNTS_FOR_CERT: &str = "vale_certificacao";
    /// Class folder: `caderno_<person>`, the paper of a trainee without the key (JSON).
    pub const PAPER_PREFIX: &str = "caderno_";
    // Answers (class folder) and results (grades folder).
    pub const EXAM: &str = "prova";
    pub const ANSWERS: &str = "respostas";
    pub const SUBMITTED_ON: &str = "entregue_em";
    pub const SCORE: &str = "nota";
    pub const PASSED: &str = "aprovado";
    pub const DETAIL: &str = "detalhe";
    pub const GRADED_BY: &str = "corrigido_por";
    pub const GRADED_ON: &str = "corrigido_em";
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExamError {
    #[error("escreva o enunciado")]
    NoStatement,
    #[error("a múltipla escolha precisa de ao menos duas alternativas e de uma alternativa certa")]
    BadOptions,
    #[error("a resposta numérica precisa de um valor e de uma tolerância não negativa")]
    BadNumber,
    #[error("o problema prático não é válido: {0}")]
    BadProblem(String),
    #[error(
        "o banco tem {have} questões de dificuldade {difficulty} nos módulos escolhidos; a prova pede {need}"
    )]
    NotEnough {
        difficulty: u8,
        have: usize,
        need: usize,
    },
    #[error("a prova não tem questões")]
    Empty,
    #[error("questão {0} não está no banco")]
    MissingQuestion(Id),
    #[error("a pessoa já tem a reaplicação desta prova")]
    RetakeUsed,
}

// --- Deterministic generator ---------------------------------------------------------------

/// SplitMix64: small, fixed and the same on every machine and version (the exams and the
/// teaching data sets must never change for the same seed).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// A generator for one trainee: the exam's seed mixed with the person's id.
    pub fn for_person(seed: u64, person: Id) -> Self {
        let b = person.as_bytes();
        let mut x = seed ^ 0x5851_F42D_4C95_7F2D;
        for chunk in b.chunks(8) {
            let mut w = [0u8; 8];
            w[..chunk.len()].copy_from_slice(chunk);
            x = x.rotate_left(17) ^ u64::from_le_bytes(w);
            x = Rng(x).next_u64();
        }
        Rng(x)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform integer in `[0, n)`.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// Standard normal (Box-Muller).
    pub fn normal(&mut self) -> f64 {
        let u1 = self.uniform().max(1e-300);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

/// Rounds to `d` decimals (the data shown are the data computed on).
pub fn round_to(x: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (x * f).round() / f
}

// --- Practical problems -------------------------------------------------------------------------

/// The recipe of a practical problem: how its data are generated for a trainee and which
/// function of the common crates gives the expected answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "receita")]
pub enum Problem {
    /// Mean of a normal sample.
    #[serde(rename = "media")]
    Mean { n: usize, mean: f64, sd: f64 },
    /// Sample standard deviation.
    #[serde(rename = "desvio_padrao")]
    StdDev { n: usize, mean: f64, sd: f64 },
    /// Cpk of individual values (sigma within by the moving range).
    #[serde(rename = "cpk")]
    Cpk {
        n: usize,
        mean: f64,
        sd: f64,
        lsl: f64,
        usl: f64,
    },
    /// Ppk of individual values (overall sigma).
    #[serde(rename = "ppk")]
    Ppk {
        n: usize,
        mean: f64,
        sd: f64,
        lsl: f64,
        usl: f64,
    },
    /// Defects per million opportunities: the data are `[defects, units, opportunities]`.
    #[serde(rename = "dpmo")]
    Dpmo {
        defects: u32,
        units: u32,
        opportunities: u32,
    },
}

impl Problem {
    pub fn validate(&self) -> Result<(), ExamError> {
        let bad = |s: &str| Err(ExamError::BadProblem(s.to_string()));
        match *self {
            Problem::Mean { n, sd, .. } | Problem::StdDev { n, sd, .. } => {
                if !(2..=500).contains(&n) || sd.is_nan() || sd <= 0.0 {
                    return bad("de 2 a 500 valores e desvio-padrão positivo");
                }
            }
            Problem::Cpk {
                n, sd, lsl, usl, ..
            }
            | Problem::Ppk {
                n, sd, lsl, usl, ..
            } => {
                if !(5..=500).contains(&n)
                    || sd.is_nan()
                    || sd <= 0.0
                    || usl.is_nan()
                    || lsl.is_nan()
                    || usl <= lsl
                {
                    return bad("de 5 a 500 valores, desvio-padrão positivo e LSE acima do LIE");
                }
            }
            Problem::Dpmo {
                units,
                opportunities,
                ..
            } => {
                if units == 0 || opportunities == 0 {
                    return bad("unidades e oportunidades positivas");
                }
            }
        }
        Ok(())
    }

    /// The data of one trainee (rounded to two decimals; counts are whole).
    pub fn data(&self, rng: &mut Rng) -> Vec<f64> {
        match *self {
            Problem::Mean { n, mean, sd }
            | Problem::StdDev { n, mean, sd }
            | Problem::Cpk { n, mean, sd, .. }
            | Problem::Ppk { n, mean, sd, .. } => (0..n)
                .map(|_| round_to(mean + sd * rng.normal(), 2))
                .collect(),
            Problem::Dpmo {
                defects,
                units,
                opportunities,
            } => {
                let f = 0.8 + 0.4 * rng.uniform();
                vec![
                    (defects as f64 * f).round().max(0.0),
                    units as f64,
                    opportunities as f64,
                ]
            }
        }
    }

    /// The expected answer for `data`, by the common crates.
    pub fn expected(&self, data: &[f64]) -> Option<f64> {
        let indiv = || data.iter().map(|x| vec![*x]).collect::<Vec<_>>();
        let cap = |lsl: f64, usl: f64| {
            let spec = utsim_quality::capability::Spec {
                lsl: Some(lsl),
                usl: Some(usl),
                target: None,
            };
            utsim_quality::capability::capability(
                &indiv(),
                &spec,
                &utsim_quality::capability::Options::default(),
            )
            .ok()
        };
        let v = match *self {
            Problem::Mean { .. } => utsim_stats::descriptive::mean(data),
            Problem::StdDev { .. } => utsim_stats::descriptive::std_dev(data),
            Problem::Cpk { lsl, usl, .. } => cap(lsl, usl)?.cpk.value,
            Problem::Ppk { lsl, usl, .. } => cap(lsl, usl)?.ppk.value,
            Problem::Dpmo { .. } => {
                let [d, u, o] = data else { return None };
                utsim_quality::capability::dpmo(*d, *u, *o)
            }
        };
        v.is_finite().then_some(v)
    }

    /// What the trainee is asked, besides the statement.
    pub fn prompt(&self) -> String {
        match *self {
            Problem::Mean { .. } => "Calcule a média dos dados.".into(),
            Problem::StdDev { .. } => "Calcule o desvio-padrão amostral dos dados.".into(),
            Problem::Cpk { lsl, usl, .. } => format!(
                "Calcule o Cpk (valores individuais, sigma de curto prazo pela amplitude móvel) com LIE = {lsl} e LSE = {usl}."
            ),
            Problem::Ppk { lsl, usl, .. } => {
                format!("Calcule o Ppk (sigma global) com LIE = {lsl} e LSE = {usl}.")
            }
            Problem::Dpmo { .. } => {
                "Os dados são defeitos, unidades e oportunidades por unidade. Calcule o DPMO."
                    .into()
            }
        }
    }
}

// --- Questions --------------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum QuestionKind {
    #[default]
    #[serde(rename = "multipla_escolha")]
    MultipleChoice,
    #[serde(rename = "verdadeiro_falso")]
    TrueFalse,
    #[serde(rename = "numerica")]
    Numeric,
    #[serde(rename = "problema_pratico")]
    Practical,
}

impl QuestionKind {
    pub const ALL: [QuestionKind; 4] = [
        QuestionKind::MultipleChoice,
        QuestionKind::TrueFalse,
        QuestionKind::Numeric,
        QuestionKind::Practical,
    ];

    pub fn label(self) -> &'static str {
        match self {
            QuestionKind::MultipleChoice => "Múltipla escolha",
            QuestionKind::TrueFalse => "Verdadeiro ou falso",
            QuestionKind::Numeric => "Resposta numérica",
            QuestionKind::Practical => "Problema prático",
        }
    }
}

/// A question of the bank, with its key.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub id: Id,
    pub module: Option<Id>,
    pub kind: QuestionKind,
    pub statement: String,
    pub options: Vec<String>,
    /// 1 (easy) to 3 (hard).
    pub difficulty: u8,
    pub key: Value,
    pub tolerance: f64,
    pub problem: Option<Problem>,
}

impl Question {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            module: e.id(fields::MODULE),
            kind: e.typed(fields::QKIND).unwrap_or_default(),
            statement: e.text(fields::STATEMENT),
            options: super::strings_in(e, fields::OPTIONS),
            difficulty: e
                .integer(fields::DIFFICULTY)
                .map_or(1, |d| d.clamp(1, 3) as u8),
            key: e.get(fields::KEY).cloned().unwrap_or(Value::Null),
            tolerance: e.number(fields::TOLERANCE).unwrap_or(0.0).max(0.0),
            problem: e.typed(fields::PROBLEM),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Question, id)
    }
}

/// The bank of a questions folder, by module, difficulty and id.
pub fn bank(state: &State) -> Vec<Question> {
    let mut v: Vec<Question> = state
        .of_kind(Kind::Question)
        .filter(|(_, e)| !e.is_deleted())
        .map(|(id, e)| Question::read(id, e))
        .collect();
    v.sort_by_key(|q| (q.module, q.difficulty, q.id));
    v
}

/// What a new question records.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QuestionInput {
    pub module: Option<Id>,
    pub kind: QuestionKind,
    pub statement: String,
    pub options: Vec<String>,
    pub difficulty: u8,
    /// Multiple choice: the index of the right option.
    pub right_option: Option<usize>,
    pub true_false: bool,
    pub number: Option<f64>,
    pub tolerance: f64,
    pub problem: Option<Problem>,
}

pub fn add_question(id: Id, input: &QuestionInput) -> Result<Vec<Edit>, ExamError> {
    if input.statement.trim().is_empty() {
        return Err(ExamError::NoStatement);
    }
    let options: Vec<String> = input
        .options
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let (key, tol, problem) = match input.kind {
        QuestionKind::MultipleChoice => match input.right_option {
            Some(i) if options.len() >= 2 && i < options.len() => (Value::from(i), 0.0, None),
            _ => return Err(ExamError::BadOptions),
        },
        QuestionKind::TrueFalse => (Value::from(input.true_false), 0.0, None),
        QuestionKind::Numeric => match input.number {
            Some(x) if x.is_finite() && input.tolerance >= 0.0 => {
                (Value::from(x), input.tolerance, None)
            }
            _ => return Err(ExamError::BadNumber),
        },
        QuestionKind::Practical => {
            let p = input
                .problem
                .clone()
                .ok_or_else(|| ExamError::BadProblem("escolha a receita".into()))?;
            p.validate()?;
            if input.tolerance.is_nan() || input.tolerance < 0.0 {
                return Err(ExamError::BadNumber);
            }
            (Value::Null, input.tolerance, Some(p))
        }
    };
    let r = Question::entity_ref(id);
    let mut e = vec![
        Edit::new(r, fields::QKIND, enum_value(input.kind)),
        Edit::new(r, fields::STATEMENT, input.statement.trim()),
        Edit::new(r, fields::DIFFICULTY, input.difficulty.clamp(1, 3)),
        Edit::new(
            r,
            fields::OPTIONS,
            Value::from(options.into_iter().map(Value::from).collect::<Vec<_>>()),
        ),
        Edit::new(r, fields::KEY, key),
        Edit::new(r, fields::TOLERANCE, tol),
    ];
    if let Some(m) = input.module {
        e.push(Edit::new(r, fields::MODULE, m.to_string()));
    }
    if let Some(p) = problem {
        e.push(Edit::new(
            r,
            fields::PROBLEM,
            serde_json::to_value(p).expect("problem"),
        ));
    }
    Ok(e)
}

/// Whether `given` is right for `q` with the data of the paper. Numbers are right within
/// the tolerance, limits included.
pub fn is_right(q: &Question, data: &[f64], given: &Value) -> bool {
    match q.kind {
        QuestionKind::MultipleChoice => {
            given.as_u64().is_some() && given.as_u64() == q.key.as_u64()
        }
        QuestionKind::TrueFalse => given.as_bool().is_some() && given.as_bool() == q.key.as_bool(),
        QuestionKind::Numeric => match (number_of(given), q.key.as_f64()) {
            (Some(a), Some(e)) => within(a, e, q.tolerance),
            _ => false,
        },
        QuestionKind::Practical => {
            let exp = q.problem.as_ref().and_then(|p| p.expected(data));
            match (number_of(given), exp) {
                (Some(a), Some(e)) => within(a, e, q.tolerance),
                _ => false,
            }
        }
    }
}

/// `|a - e| <= tol`, with a relative slack of 1e-9 so the limits are inside.
pub fn within(a: f64, e: f64, tol: f64) -> bool {
    (a - e).abs() <= tol + 1e-9 * (1.0 + e.abs())
}

/// A number typed as text ("1,25" or "1.25") or given as a number.
pub fn number_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_number(s),
        _ => None,
    }
}

pub fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim().replace(' ', "");
    // The last separator is the decimal one ("1.234,5" and "1,234.5" are both 1234.5).
    let t = match (t.rfind(','), t.rfind('.')) {
        (Some(c), Some(p)) if c > p => t.replace('.', "").replace(',', "."),
        (Some(_), None) => t.replace(',', "."),
        _ => t.replace(',', ""),
    };
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// The expected answer of a question for the data of a paper (for the instructor's
/// correction screen).
pub fn expected(q: &Question, data: &[f64]) -> Value {
    match q.kind {
        QuestionKind::Practical => q
            .problem
            .as_ref()
            .and_then(|p| p.expected(data))
            .map_or(Value::Null, Value::from),
        _ => q.key.clone(),
    }
}

// --- Exams ------------------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Mode {
    /// In the trainee's app, exam mode.
    #[default]
    #[serde(rename = "app")]
    App,
    /// On paper, the instructor records the grade.
    #[serde(rename = "papel")]
    Paper,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Exam {
    pub id: Id,
    pub class: Option<Id>,
    pub name: String,
    pub modules: Vec<Id>,
    pub seed: u64,
    /// Questions per difficulty 1, 2 and 3.
    pub counts: [u32; 3],
    pub questions: Vec<Id>,
    pub mode: Mode,
    pub min_grade: f64,
    pub retake_of: Option<Id>,
    pub date: Option<Date>,
    /// The company's decision (MB3): whether the exam counts for certification.
    pub counts_for_cert: bool,
}

impl Exam {
    pub fn read(id: Id, e: &Entity) -> Self {
        let count = |d: u8| {
            e.integer(&format!("{}{d}", fields::COUNT_PREFIX))
                .map_or(0, |n| n.clamp(0, 500) as u32)
        };
        Self {
            id,
            class: e.id(tf::CLASS),
            name: e.text(tf::NAME),
            modules: ids_in(e, tf::MODULES),
            seed: e
                .get(fields::SEED)
                .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or(v.as_u64()))
                .unwrap_or(0),
            counts: [count(1), count(2), count(3)],
            questions: ids_in(e, fields::QUESTIONS),
            mode: e.typed(fields::MODE).unwrap_or_default(),
            min_grade: e
                .number(tf::MIN_GRADE)
                .filter(|x| (0.0..=1.0).contains(x))
                .unwrap_or(DEFAULT_MIN_GRADE),
            retake_of: e.id(fields::RETAKE_OF),
            date: e.opt_text(tf::DATE).and_then(|s| s.parse().ok()),
            counts_for_cert: e
                .get(fields::COUNTS_FOR_CERT)
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Exam, id)
    }

    /// The original exam (itself, or the one it reapplies).
    pub fn root(&self) -> Id {
        self.retake_of.unwrap_or(self.id)
    }
}

pub fn exams(state: &State) -> Vec<Exam> {
    let mut v: Vec<Exam> = state
        .of_kind(Kind::Exam)
        .filter(|(_, e)| !e.is_deleted())
        .map(|(id, e)| Exam::read(id, e))
        .collect();
    v.sort_by_key(|x| (x.date, x.id));
    v
}

pub fn exams_of(all: &[Exam], class: Id) -> Vec<&Exam> {
    all.iter().filter(|x| x.class == Some(class)).collect()
}

/// Draws the questions: from the bank of the modules, per difficulty, in an order shuffled
/// by the seed. The same bank, modules, counts and seed give the same questions in the
/// same order.
pub fn draw(
    bank: &[Question],
    modules: &[Id],
    counts: [u32; 3],
    seed: u64,
) -> Result<Vec<Id>, ExamError> {
    if counts.iter().all(|c| *c == 0) {
        return Err(ExamError::Empty);
    }
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for d in 1..=3u8 {
        let need = counts[d as usize - 1] as usize;
        let mut pool: Vec<Id> = bank
            .iter()
            .filter(|q| q.difficulty == d)
            .filter(|q| modules.is_empty() || q.module.is_some_and(|m| modules.contains(&m)))
            .map(|q| q.id)
            .collect();
        pool.sort();
        if pool.len() < need {
            return Err(ExamError::NotEnough {
                difficulty: d,
                have: pool.len(),
                need,
            });
        }
        rng.shuffle(&mut pool);
        out.extend(pool.into_iter().take(need));
    }
    Ok(out)
}

/// What a new exam records.
#[derive(Debug, Clone, PartialEq)]
pub struct ExamInput {
    pub class: Id,
    pub name: String,
    pub modules: Vec<Id>,
    pub counts: [u32; 3],
    pub seed: u64,
    pub mode: Mode,
    pub min_grade: f64,
    pub date: Option<Date>,
    pub retake_of: Option<Id>,
    pub counts_for_cert: bool,
}

/// Edits of a new exam of the program journal (the questions drawn, never the key).
pub fn new_exam(
    program: Id,
    id: Id,
    input: &ExamInput,
    bank: &[Question],
) -> Result<Vec<Edit>, ExamError> {
    let qs = draw(bank, &input.modules, input.counts, input.seed)?;
    let r = Exam::entity_ref(id);
    let mut e = vec![
        Edit::new(r, tf::CLASS, input.class.to_string()),
        Edit::new(r, tf::PROGRAM, program.to_string()),
        Edit::new(r, tf::NAME, input.name.trim()),
        Edit::new(r, tf::MODULES, ids_value(&input.modules)),
        // As text: a u64 does not fit a JSON number read as f64 everywhere.
        Edit::new(r, fields::SEED, input.seed.to_string()),
        Edit::new(r, fields::QUESTIONS, ids_value(&qs)),
        Edit::new(r, fields::MODE, enum_value(input.mode)),
        Edit::new(r, tf::MIN_GRADE, input.min_grade.clamp(0.0, 1.0)),
        Edit::new(r, fields::COUNTS_FOR_CERT, input.counts_for_cert),
    ];
    for (i, c) in input.counts.iter().enumerate() {
        e.push(Edit::new(
            r,
            format!("{}{}", fields::COUNT_PREFIX, i + 1),
            *c,
        ));
    }
    if let Some(d) = input.date {
        e.push(Edit::new(r, tf::DATE, d.to_string()));
    }
    if let Some(o) = input.retake_of {
        e.push(Edit::new(r, fields::RETAKE_OF, o.to_string()));
    }
    Ok(e)
}

// --- Papers -----------------------------------------------------------------------------------

/// One item of a trainee's paper: what is shown, without the key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    #[serde(rename = "questao")]
    pub question: Id,
    #[serde(rename = "tipo")]
    pub kind: QuestionKind,
    #[serde(rename = "enunciado")]
    pub statement: String,
    #[serde(rename = "alternativas", default)]
    pub options: Vec<String>,
    #[serde(rename = "dados", default)]
    pub data: Vec<f64>,
}

/// The paper of a trainee: the exam's questions in a per-person order, with the data of the
/// practical problems generated for that person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paper {
    #[serde(rename = "prova")]
    pub exam: Id,
    #[serde(rename = "pessoa")]
    pub person: Id,
    #[serde(rename = "nome")]
    pub name: String,
    #[serde(rename = "itens")]
    pub items: Vec<Item>,
}

/// Builds the paper of `person` (deterministic: the instructor rebuilds it to grade).
pub fn paper(exam: &Exam, bank: &[Question], person: Id) -> Result<Paper, ExamError> {
    let mut rng = Rng::for_person(exam.seed, person);
    let mut order = exam.questions.clone();
    rng.shuffle(&mut order);
    let mut items = Vec::new();
    for qid in order {
        let q = bank
            .iter()
            .find(|q| q.id == qid)
            .ok_or(ExamError::MissingQuestion(qid))?;
        let (statement, data) = match (&q.kind, &q.problem) {
            (QuestionKind::Practical, Some(p)) => {
                (format!("{}\n{}", q.statement, p.prompt()), p.data(&mut rng))
            }
            _ => (q.statement.clone(), Vec::new()),
        };
        let options = match q.kind {
            QuestionKind::MultipleChoice => q.options.clone(),
            QuestionKind::TrueFalse => vec!["Verdadeiro".into(), "Falso".into()],
            _ => Vec::new(),
        };
        items.push(Item {
            question: q.id,
            kind: q.kind,
            statement,
            options,
            data,
        });
    }
    Ok(Paper {
        exam: exam.id,
        person,
        name: exam.name.clone(),
        items,
    })
}

pub fn paper_field(person: Id) -> String {
    format!("{}{}", fields::PAPER_PREFIX, portfolio::id_key(person))
}

/// Edits that publish an exam in the class folder: the exam's public fields and one paper
/// per trainee, never the key nor the recipes.
pub fn publish(exam: &Exam, bank: &[Question], trainees: &[Id]) -> Result<Vec<Edit>, ExamError> {
    let r = Exam::entity_ref(exam.id);
    let mut e = vec![
        Edit::new(
            r,
            tf::CLASS,
            exam.class.map(|c| c.to_string()).unwrap_or_default(),
        ),
        Edit::new(r, tf::NAME, exam.name.clone()),
        Edit::new(r, fields::MODE, enum_value(exam.mode)),
    ];
    if let Some(d) = exam.date {
        e.push(Edit::new(r, tf::DATE, d.to_string()));
    }
    for p in trainees {
        let pp = paper(exam, bank, *p)?;
        e.push(Edit::new(
            r,
            paper_field(*p),
            serde_json::to_value(pp).expect("paper"),
        ));
    }
    Ok(e)
}

/// The papers published in a class folder: `(exam, name, person → paper)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Published {
    pub exam: Id,
    pub name: String,
    pub date: Option<Date>,
    pub papers: BTreeMap<Id, Paper>,
}

pub fn published(class_state: &State) -> Vec<Published> {
    let mut v: Vec<Published> = class_state
        .of_kind(Kind::Exam)
        .filter(|(_, e)| !e.is_deleted())
        .map(|(id, e)| {
            let mut papers = BTreeMap::new();
            for (k, fv) in &e.fields {
                if let Some(p) = k
                    .strip_prefix(fields::PAPER_PREFIX)
                    .and_then(programa::id_from_key)
                {
                    if let Ok(pp) = serde_json::from_value::<Paper>(fv.value.clone()) {
                        papers.insert(p, pp);
                    }
                }
            }
            Published {
                exam: id,
                name: e.text(tf::NAME),
                date: e.opt_text(tf::DATE).and_then(|s| s.parse().ok()),
                papers,
            }
        })
        .collect();
    v.sort_by_key(|p| (p.date, p.exam));
    v
}

// --- Answers ----------------------------------------------------------------------------------

/// A trainee's answers to an exam (class folder, written by the trainee).
#[derive(Debug, Clone, PartialEq)]
pub struct Answers {
    pub id: Id,
    pub exam: Option<Id>,
    pub person: Option<Id>,
    pub answers: BTreeMap<Id, Value>,
    pub submitted_on: Option<Date>,
}

impl Answers {
    pub fn read(id: Id, e: &Entity) -> Self {
        let answers = e
            .get(fields::ANSWERS)
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| Some((k.parse().ok()?, v.clone())))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            id,
            exam: e.id(fields::EXAM),
            person: e.id(tf::PERSON),
            answers,
            submitted_on: e
                .opt_text(fields::SUBMITTED_ON)
                .and_then(|s| s.parse().ok()),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::ExamAnswer, id)
    }
}

pub fn all_answers(class_state: &State) -> Vec<Answers> {
    let mut v: Vec<Answers> = class_state
        .of_kind(Kind::ExamAnswer)
        .filter(|(_, e)| !e.is_deleted())
        .map(|(id, e)| Answers::read(id, e))
        .collect();
    v.sort_by_key(|a| (a.exam, a.person, a.id));
    v
}

/// Edits of a trainee's answers (the person first: the class folder's check reads it).
/// `submit` closes the paper.
pub fn answer(
    id: Id,
    exam: Id,
    person: Id,
    answers: &BTreeMap<Id, Value>,
    submit: Option<Date>,
) -> Vec<Edit> {
    let r = Answers::entity_ref(id);
    let map: serde_json::Map<String, Value> = answers
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    let mut e = vec![
        Edit::new(r, tf::PERSON, person.to_string()),
        Edit::new(r, fields::EXAM, exam.to_string()),
        Edit::new(r, fields::ANSWERS, Value::Object(map)),
    ];
    if let Some(d) = submit {
        e.push(Edit::new(r, fields::SUBMITTED_ON, d.to_string()));
    }
    e
}

// --- Grading ----------------------------------------------------------------------------------

/// The correction of one item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemGrade {
    #[serde(rename = "questao")]
    pub question: Id,
    #[serde(rename = "certa")]
    pub right: bool,
    #[serde(rename = "esperada")]
    pub expected: Value,
    #[serde(rename = "dada")]
    pub given: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grade {
    pub items: Vec<ItemGrade>,
    /// Right answers over the items, 0 to 1.
    pub score: f64,
    pub passed: bool,
}

/// Grades a trainee's answers against the paper rebuilt from the seed and the bank.
pub fn grade(
    exam: &Exam,
    bank: &[Question],
    person: Id,
    answers: &BTreeMap<Id, Value>,
) -> Result<Grade, ExamError> {
    let p = paper(exam, bank, person)?;
    let mut items = Vec::new();
    for it in &p.items {
        let q = bank
            .iter()
            .find(|q| q.id == it.question)
            .ok_or(ExamError::MissingQuestion(it.question))?;
        let given = answers.get(&q.id).cloned().unwrap_or(Value::Null);
        items.push(ItemGrade {
            question: q.id,
            right: is_right(q, &it.data, &given),
            expected: expected(q, &it.data),
            given,
        });
    }
    if items.is_empty() {
        return Err(ExamError::Empty);
    }
    let right = items.iter().filter(|i| i.right).count();
    let score = right as f64 / items.len() as f64;
    Ok(Grade {
        passed: score + 1e-9 >= exam.min_grade,
        score,
        items,
    })
}

/// A result of the grades folder.
#[derive(Debug, Clone, PartialEq)]
pub struct ExamResult {
    pub id: Id,
    pub exam: Option<Id>,
    pub person: Option<Id>,
    pub class: Option<Id>,
    pub score: f64,
    pub passed: bool,
    pub mode: Mode,
    pub graded_by: Option<Id>,
    pub graded_on: Option<Date>,
    pub detail: Vec<ItemGrade>,
}

impl ExamResult {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            exam: e.id(fields::EXAM),
            person: e.id(tf::PERSON),
            class: e.id(tf::CLASS),
            score: e.number(fields::SCORE).unwrap_or(0.0).clamp(0.0, 1.0),
            passed: e.boolean(fields::PASSED),
            mode: e.typed(fields::MODE).unwrap_or_default(),
            graded_by: e.id(fields::GRADED_BY),
            graded_on: e.opt_text(fields::GRADED_ON).and_then(|s| s.parse().ok()),
            detail: e.typed(fields::DETAIL).unwrap_or_default(),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::ExamResult, id)
    }
}

pub fn results(grades_state: &State) -> Vec<ExamResult> {
    let mut v: Vec<ExamResult> = grades_state
        .of_kind(Kind::ExamResult)
        .filter(|(_, e)| !e.is_deleted())
        .map(|(id, e)| ExamResult::read(id, e))
        .collect();
    v.sort_by_key(|r| (r.graded_on, r.id));
    v
}

/// Edits of a result. `id` is kept per `(exam, person)` by the caller (a new grading of
/// the same paper replaces the old one).
#[allow(clippy::too_many_arguments)]
pub fn record_result(
    id: Id,
    exam: &Exam,
    person: Id,
    score: f64,
    detail: &[ItemGrade],
    by: Id,
    date: Date,
) -> Vec<Edit> {
    let r = ExamResult::entity_ref(id);
    let score = score.clamp(0.0, 1.0);
    let mut e = vec![
        Edit::new(r, fields::EXAM, exam.id.to_string()),
        Edit::new(r, tf::PERSON, person.to_string()),
        Edit::new(r, fields::SCORE, score),
        Edit::new(r, fields::PASSED, score + 1e-9 >= exam.min_grade),
        Edit::new(r, fields::MODE, enum_value(exam.mode)),
        Edit::new(r, fields::GRADED_BY, by.to_string()),
        Edit::new(r, fields::GRADED_ON, date.to_string()),
        Edit::new(
            r,
            fields::DETAIL,
            serde_json::to_value(detail).expect("detail"),
        ),
    ];
    if let Some(c) = exam.class {
        e.push(Edit::new(r, tf::CLASS, c.to_string()));
    }
    e
}

/// The standing of a person in the exams of a class: the original exam and its single
/// reapplication. The best attempt counts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Standing {
    pub attempts: usize,
    pub best: Option<f64>,
    pub passed: bool,
    /// Failed the first attempt and has no reapplication yet.
    pub may_retake: bool,
    /// The result that passed (the evidence of the certification).
    pub evidence: Option<Id>,
}

/// The standing of `person` in the exams of `class` that count for certification.
pub fn standing(all: &[Exam], results: &[ExamResult], class: Id, person: Id) -> Standing {
    let mine = exams_of(all, class);
    let roots: Vec<Id> = {
        let mut r: Vec<Id> = mine
            .iter()
            .filter(|x| x.counts_for_cert)
            .map(|x| x.root())
            .collect();
        r.sort();
        r.dedup();
        r
    };
    let mut s = Standing::default();
    // Each original exam with its one reapplication: every original must be passed.
    let mut all_passed = !roots.is_empty();
    let mut evidence = None;
    for root in &roots {
        let family: Vec<&&Exam> = mine.iter().filter(|x| x.root() == *root).collect();
        let rs: Vec<&ExamResult> = results
            .iter()
            .filter(|r| r.person == Some(person))
            .filter(|r| family.iter().any(|x| Some(x.id) == r.exam))
            .collect();
        s.attempts += rs.len();
        let best = rs
            .iter()
            .map(|r| r.score)
            .fold(None::<f64>, |a, b| Some(a.map_or(b, |a| a.max(b))));
        s.best = match (s.best, best) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let ok = rs.iter().find(|r| r.passed);
        match ok {
            Some(r) => evidence = evidence.or(Some(r.id)),
            None => {
                all_passed = false;
                let has_retake = family.iter().any(|x| x.retake_of.is_some());
                let retake_graded = rs.iter().any(|r| {
                    family
                        .iter()
                        .any(|x| x.retake_of.is_some() && Some(x.id) == r.exam)
                });
                if !rs.is_empty() && !(has_retake && retake_graded) {
                    s.may_retake = true;
                }
            }
        }
    }
    s.passed = all_passed;
    s.evidence = if all_passed { evidence } else { None };
    s
}

/// The default questions of the demonstration bank for a module of tool `tool`, so a new
/// program can draw an exam at once. Written for UTSim.
pub fn sample_questions(module: Id, tools: &[String]) -> Vec<QuestionInput> {
    let mut v = Vec::new();
    let q = |kind, statement: &str, difficulty| QuestionInput {
        module: Some(module),
        kind,
        statement: statement.into(),
        difficulty,
        ..QuestionInput::default()
    };
    for t in tools {
        match t.as_str() {
            "capacidade" => {
                v.push(QuestionInput {
                    problem: Some(Problem::Cpk {
                        n: 30,
                        mean: 10.0,
                        sd: 0.2,
                        lsl: 9.4,
                        usl: 10.6,
                    }),
                    tolerance: 0.05,
                    ..q(
                        QuestionKind::Practical,
                        "Diâmetros de eixos medidos em sequência na linha.",
                        2,
                    )
                });
                v.push(QuestionInput {
                    true_false: false,
                    ..q(
                        QuestionKind::TrueFalse,
                        "Um Cpk de 0,8 indica um processo capaz.",
                        1,
                    )
                });
            }
            "pareto" => v.push(QuestionInput {
                options: vec![
                    "Ordenar as categorias da maior para a menor frequência".into(),
                    "Mostrar a evolução da média no tempo".into(),
                    "Comparar duas médias".into(),
                ],
                right_option: Some(0),
                ..q(
                    QuestionKind::MultipleChoice,
                    "O que o gráfico de Pareto faz?",
                    1,
                )
            }),
            "testes_hipotese" | "cep" => v.push(QuestionInput {
                problem: Some(Problem::Mean {
                    n: 20,
                    mean: 50.0,
                    sd: 2.0,
                }),
                tolerance: 0.01,
                ..q(
                    QuestionKind::Practical,
                    "Tempos de atendimento, em minutos.",
                    1,
                )
            }),
            "msa" => v.push(QuestionInput {
                number: Some(10.0),
                tolerance: 0.0,
                ..q(
                    QuestionKind::Numeric,
                    "Até quantos por cento de %GRR o sistema de medição é aceito sem ressalvas?",
                    2,
                )
            }),
            "fmea" => v.push(QuestionInput {
                options: vec![
                    "Severidade, ocorrência e detecção".into(),
                    "Custo, prazo e escopo".into(),
                    "Média, mediana e moda".into(),
                ],
                right_option: Some(0),
                ..q(
                    QuestionKind::MultipleChoice,
                    "Quais notas compõem a prioridade de ação do FMEA?",
                    1,
                )
            }),
            _ => {}
        }
    }
    if v.is_empty() {
        v.push(QuestionInput {
            problem: Some(Problem::Dpmo {
                defects: 25,
                units: 500,
                opportunities: 4,
            }),
            tolerance: 1.0,
            ..q(QuestionKind::Practical, "Inspeção final de um lote.", 2)
        });
    }
    v
}
