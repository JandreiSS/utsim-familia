//! Exam mode (`docs/plano_ensino_e_jogo.md`, 4.5 and 4.6): the exam is drawn with a
//! recorded seed from the missions of a package, by concept and act. The student gets a
//! package without reference, hints or answer key, with a time window, a time limit and a
//! number of runs per mission. The teacher rebuilds the full exam from the seed and grades
//! on their machine only. [`semelhancas`] points pairs of students whose attempts look
//! copied.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::missao::{Decisoes, Missao, Previsao, Resposta};
use crate::pacote::{Arquivos, Ato, PacoteCarregado, hash_arquivos};

/// The exam rules that travel in the student's package (`pacote.ron`, field `prova`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Prova {
    /// Seed of the draw (the teacher rebuilds the exam with it).
    pub semente: u64,
    /// Window, in milliseconds since 1970 (UTC). Runs outside it are blocked in the app
    /// and ignored by the teacher.
    pub inicio: i64,
    pub fim: i64,
    /// Time limit counted from the moment the student opens the exam (0: only the window).
    #[serde(default)]
    pub duracao_min: u32,
    /// Runs allowed per mission (the teacher grades only the first ones).
    #[serde(default = "um")]
    pub tentativas: u32,
    /// Offset of the local time used to show the window, in minutes (-180: Brasília).
    #[serde(default = "brasilia")]
    pub fuso_min: i32,
}

fn um() -> u32 {
    1
}

fn brasilia() -> i32 {
    -180
}

impl Prova {
    /// Whether a run at `agora` (ms) is inside the window and, when the exam was opened
    /// at `aberta`, inside the time limit. The error says why not.
    pub fn pode_rodar(&self, agora: i64, aberta: Option<i64>) -> Result<(), String> {
        if agora < self.inicio {
            return Err(format!(
                "a prova começa em {}",
                data_hora(self.inicio, self.fuso_min)
            ));
        }
        if agora > self.fim {
            return Err(format!(
                "a prova terminou em {}",
                data_hora(self.fim, self.fuso_min)
            ));
        }
        if let Some(a) = aberta
            && self.duracao_min > 0
            && agora > a + i64::from(self.duracao_min) * 60_000
        {
            return Err(format!("o tempo de {} min acabou", self.duracao_min));
        }
        Ok(())
    }

    /// Milliseconds left at `agora` (window and time limit together).
    pub fn restante(&self, agora: i64, aberta: Option<i64>) -> i64 {
        let mut fim = self.fim;
        if let Some(a) = aberta
            && self.duracao_min > 0
        {
            fim = fim.min(a + i64::from(self.duracao_min) * 60_000);
        }
        (fim - agora).max(0)
    }
}

/// One rule of the draw: `quantidade` missions among those of act `ato` and with concept
/// `conceito` (each filter is optional).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Regra {
    #[serde(default)]
    pub ato: Option<u32>,
    #[serde(default)]
    pub conceito: Option<String>,
    pub quantidade: usize,
}

/// What the teacher keeps to rebuild the exam: the base package, the rules and the seed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Montagem {
    /// Exam id (the package id of the exam).
    pub id: String,
    pub titulo: String,
    /// Id and hash of the base package the missions come from.
    pub pacote_base: String,
    pub hash_base: String,
    pub regras: Vec<Regra>,
    pub prova: Prova,
}

/// The two packages of an exam.
#[derive(Clone, Debug)]
pub struct ProvaMontada {
    /// Mission ids in the order of the exam.
    pub missoes: Vec<String>,
    /// For the students: no reference, hints or answer key.
    pub aluno: Arquivos,
    /// For the teacher's grading: the missions as they are in the base package.
    pub professor: Arquivos,
}

fn splitmix(s: &mut u64) -> u64 {
    *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *s;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Draws the missions of the exam: the same seed and rules give the same exam. Missions
/// that ask for a model to deliver (act 7) stay out, since their answer key is a model.
pub fn sortear(p: &PacoteCarregado, regras: &[Regra], semente: u64) -> Result<Vec<String>, String> {
    let mut escolhidas: Vec<String> = Vec::new();
    for (i, r) in regras.iter().enumerate() {
        let mut candidatas: Vec<&str> = p
            .missoes
            .iter()
            .filter(|m| m.missao.modelagem.is_none())
            .filter(|m| r.ato.is_none_or(|a| a == m.ato))
            .filter(|m| {
                r.conceito
                    .as_ref()
                    .is_none_or(|c| m.missao.conceitos.contains(c))
            })
            .map(|m| m.missao.id.as_str())
            .filter(|id| !escolhidas.iter().any(|e| e == id))
            .collect();
        candidatas.sort_unstable();
        if candidatas.len() < r.quantidade {
            return Err(format!(
                "regra {}: pedidas {} missões, há {}",
                i + 1,
                r.quantidade,
                candidatas.len()
            ));
        }
        let mut s = semente ^ (i as u64).wrapping_mul(0xA24B_AED4_963E_E407);
        for k in 0..r.quantidade {
            let j = k + (splitmix(&mut s) % (candidatas.len() - k) as u64) as usize;
            candidatas.swap(k, j);
        }
        escolhidas.extend(candidatas[..r.quantidade].iter().map(|s| s.to_string()));
    }
    if escolhidas.is_empty() {
        return Err("a prova não tem missões".into());
    }
    Ok(escolhidas)
}

/// The mission as the student gets it in an exam: no reference, no hints, and the fixed
/// answers of the predictions and of the justification replaced by the first option.
pub fn sem_gabarito(m: &Missao) -> Missao {
    let mut m = m.clone();
    m.referencia = Decisoes::new();
    m.dicas.clear();
    for q in &mut m.justificativa {
        q.certa = 0;
    }
    for p in &mut m.previsao {
        if let Previsao::Escolha { resposta, .. } = p
            && matches!(resposta, Resposta::Fixa(_))
        {
            *resposta = Resposta::Fixa(0);
        }
    }
    m
}

fn ron_bonito<T: Serialize>(v: &T) -> Result<String, String> {
    ron::ser::to_string_pretty(
        v,
        ron::ser::PrettyConfig::default().extensions(ron::extensions::Extensions::IMPLICIT_SOME),
    )
    .map_err(|e| e.to_string())
}

/// Builds the exam from the base package (`arquivos`, already loaded as `p`).
pub fn montar(
    arquivos: &Arquivos,
    p: &PacoteCarregado,
    m: &Montagem,
) -> Result<ProvaMontada, String> {
    if m.id.trim().is_empty() || m.id.contains(['/', '\\']) {
        return Err("a prova precisa de um id sem barras".into());
    }
    if m.prova.fim <= m.prova.inicio {
        return Err("o fim da prova precisa ser depois do início".into());
    }
    let missoes = sortear(p, &m.regras, m.prova.semente)?;
    // Mission path of each id (the acts list the paths in the order of the missions).
    let caminhos: Vec<&String> = p.pacote.atos.iter().flat_map(|a| &a.missoes).collect();
    let caminho_de = |id: &str| -> Result<String, String> {
        let i = p
            .missoes
            .iter()
            .position(|x| x.missao.id == id)
            .ok_or_else(|| format!("missão '{id}' não encontrada"))?;
        Ok(caminhos[i].clone())
    };
    let escolhidos: Vec<String> = missoes
        .iter()
        .map(|id| caminho_de(id))
        .collect::<Result<_, _>>()?;

    let mut manifesto = p.pacote.clone();
    manifesto.id = m.id.clone();
    manifesto.titulo = m.titulo.clone();
    manifesto.descricao = format!("Prova montada de {} {}", p.pacote.titulo, p.pacote.versao);
    manifesto.atos = vec![Ato {
        numero: 1,
        titulo: m.titulo.clone(),
        missoes: escolhidos.clone(),
    }];
    manifesto.prova = Some(m.prova.clone());
    let manifesto = ron_bonito(&manifesto)?;

    // Every mission file of the base leaves, except the chosen ones.
    let todas: BTreeSet<&String> = caminhos.iter().copied().collect();
    let mut professor: Arquivos = arquivos
        .iter()
        .filter(|(k, _)| !todas.contains(k) || escolhidos.contains(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    // The statements of the missions left out leave too.
    for (c, mc) in caminhos.iter().zip(&p.missoes) {
        if !escolhidos.contains(c) {
            let dir = c.rsplit_once('/').map_or("", |(d, _)| d);
            let texto = if dir.is_empty() {
                mc.missao.texto.clone()
            } else {
                format!("{dir}/{}", mc.missao.texto)
            };
            professor.remove(&texto);
        }
    }
    professor.insert("pacote.ron".into(), manifesto.into_bytes());
    let mut aluno = professor.clone();
    for (c, id) in escolhidos.iter().zip(&missoes) {
        let mc = p.missao(id).ok_or("missão")?;
        let txt = ron_bonito(&sem_gabarito(&mc.missao))?;
        aluno.insert(c.clone(), txt.into_bytes());
    }
    Ok(ProvaMontada {
        missoes,
        aluno,
        professor,
    })
}

/// The teacher's package of an exam, ready to grade the deliveries: it carries the hash of
/// the student's package, so attempts made with it are not taken for another version.
pub fn pacote_do_professor(pm: &ProvaMontada) -> Result<PacoteCarregado, Vec<String>> {
    let mut p = crate::pacote::carregar(&pm.professor)?;
    p.hash = hash_arquivos(&pm.aluno);
    Ok(p)
}

/// One attempt as the similarity check sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Rastro {
    pub aluno: String,
    pub missao: String,
    /// Milliseconds since 1970.
    pub quando: i64,
    pub decisoes: Decisoes,
}

/// A pair of students worth a look, and why. The decision is always the teacher's.
#[derive(Clone, Debug, PartialEq)]
pub struct Semelhanca {
    pub a: String,
    pub b: String,
    pub missao: String,
    pub motivo: String,
}

/// Pairs with the same sequence of decisions in a mission whose variant changes from
/// student to student (`com_variante`), or with at least three attempts each at the same
/// times (within `janela_ms`).
pub fn semelhancas(
    rastros: &[Rastro],
    com_variante: &BTreeSet<String>,
    janela_ms: i64,
) -> Vec<Semelhanca> {
    let mut por: BTreeMap<(&str, &str), Vec<&Rastro>> = BTreeMap::new();
    for r in rastros {
        por.entry((r.missao.as_str(), r.aluno.as_str()))
            .or_default()
            .push(r);
    }
    for v in por.values_mut() {
        v.sort_by_key(|r| r.quando);
    }
    let mut missoes: BTreeMap<&str, Vec<(&str, &Vec<&Rastro>)>> = BTreeMap::new();
    for ((m, a), v) in &por {
        missoes.entry(m).or_default().push((a, v));
    }
    let mut out = Vec::new();
    for (missao, alunos) in missoes {
        for i in 0..alunos.len() {
            for j in i + 1..alunos.len() {
                let (a, ra) = alunos[i];
                let (b, rb) = alunos[j];
                let mut motivos = Vec::new();
                let seq = |v: &Vec<&Rastro>| -> Vec<Decisoes> {
                    v.iter().map(|r| r.decisoes.clone()).collect()
                };
                if com_variante.contains(missao) && ra.len() >= 2 && seq(ra) == seq(rb) {
                    motivos.push(format!(
                        "as mesmas {} decisões em variantes diferentes",
                        ra.len()
                    ));
                }
                if ra.len() >= 3
                    && ra.len() == rb.len()
                    && ra
                        .iter()
                        .zip(rb.iter())
                        .all(|(x, y)| (x.quando - y.quando).abs() <= janela_ms)
                {
                    motivos.push(format!(
                        "{} tentativas nos mesmos instantes (até {} s de diferença)",
                        ra.len(),
                        janela_ms / 1000
                    ));
                }
                if !motivos.is_empty() {
                    out.push(Semelhanca {
                        a: a.to_string(),
                        b: b.to_string(),
                        missao: missao.to_string(),
                        motivo: motivos.join(" e "),
                    });
                }
            }
        }
    }
    out
}

/// Days since 1970 to (year, month, day) in the proleptic Gregorian calendar.
fn civil(dias: i64) -> (i64, u32, u32) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn dias_de(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `AAAA-MM-DD HH:MM` in the local time of offset `fuso_min`.
pub fn data_hora(ms: i64, fuso_min: i32) -> String {
    let min = (ms.div_euclid(60_000)) + i64::from(fuso_min);
    let (dias, resto) = (min.div_euclid(1440), min.rem_euclid(1440));
    let (y, m, d) = civil(dias);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", resto / 60, resto % 60)
}

/// Reads `AAAA-MM-DD HH:MM` in the local time of offset `fuso_min`.
pub fn ler_data_hora(s: &str, fuso_min: i32) -> Option<i64> {
    let (data, hora) = s.trim().split_once(' ')?;
    let mut d = data.split('-').map(|x| x.parse::<i64>().ok());
    let (y, mo, dd) = (d.next()??, d.next()??, d.next()??);
    let mut h = hora.trim().split(':').map(|x| x.parse::<i64>().ok());
    let (hh, mm) = (h.next()??, h.next()??);
    if !(1..=12).contains(&mo)
        || !(1..=31).contains(&dd)
        || !(0..24).contains(&hh)
        || !(0..60).contains(&mm)
    {
        return None;
    }
    let dias = dias_de(y, mo as u32, dd as u32);
    if civil(dias) != (y, mo as u32, dd as u32) {
        return None;
    }
    Some(((dias * 1440 + hh * 60 + mm) - i64::from(fuso_min)) * 60_000)
}
