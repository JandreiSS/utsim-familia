//! The "pieces" of the second autonomy step (`docs/plano_ensino_e_jogo.md`, 3.2 and EN5):
//! the student assigns tasks to stations ([`Balanceamento`]) or places machines on a grid
//! ([`Arranjo`]), and the app compiles the decisions into model keys.
//!
//! Decisions use the same map as the levers: `tarefa.<id>` is the station (1, 2...) of a
//! task, `pos.<maquina>` is the cell (0 at the top left, row by row) of a machine.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::missao::{Decisoes, numero_ron};

fn verdadeiro() -> bool {
    true
}

/// A line balancing problem.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Balanceamento {
    pub tarefas: Vec<Tarefa>,
    /// Takt time (the demand rhythm), in the time unit of the model.
    pub takt: f64,
    /// Stations available in the model (`est1` to `estN`).
    pub estacoes: u32,
    /// Coefficient of variation of the task times in the simulation (0: constant).
    #[serde(default)]
    pub cv: f64,
    /// Whether a station may pass the takt (when false, that breaks the gate).
    #[serde(default)]
    pub permite_passar_takt: bool,
    /// Whether the student assigns the tasks (false: the assignment is fixed).
    #[serde(default = "verdadeiro")]
    pub editavel: bool,
    /// The assignment of the initial state: task id to station.
    pub inicial: BTreeMap<String, u32>,
    /// Name prefix of the station objects (`est` gives `est1`...).
    #[serde(default = "prefixo_estacao")]
    pub prefixo: String,
    /// Models of a mixed-model line, in the order of their label value (1, 2...). Empty:
    /// a single model. With models, `Tarefa::tempo` is the average weighted by the mix
    /// (what the balancing uses) and `Tarefa::tempos` has the time of each model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mix: Vec<ModeloMix>,
    /// Item label holding the model number (1-based, the order of `mix`).
    #[serde(default = "rotulo_modelo")]
    pub rotulo: String,
}

fn prefixo_estacao() -> String {
    "est".into()
}

fn rotulo_modelo() -> String {
    "modelo".into()
}

/// One model of a mixed-model line.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModeloMix {
    pub id: String,
    pub nome: String,
    /// Share of the demand (relative weight).
    pub parcela: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tarefa {
    pub id: String,
    pub nome: String,
    pub tempo: f64,
    /// Tasks that must be done before this one.
    #[serde(default)]
    pub depois_de: Vec<String>,
    /// Time of the task for each model of a mixed-model line (model id to time).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tempos: BTreeMap<String, f64>,
}

/// The station load and the indicators of an assignment.
#[derive(Clone, Debug, PartialEq)]
pub struct ResumoBalanceamento {
    /// Load of each station 1..=N (index 0 is station 1).
    pub cargas: Vec<f64>,
    /// Stations with at least one task.
    pub estacoes_usadas: u32,
    /// Largest station load.
    pub ciclo: f64,
    /// Sum of the task times over (used stations × takt).
    pub eficiencia: f64,
    /// 1 − efficiency.
    pub atraso: f64,
}

impl Balanceamento {
    pub fn chave(id: &str) -> String {
        format!("tarefa.{id}")
    }

    pub fn soma(&self) -> f64 {
        self.tarefas.iter().map(|t| t.tempo).sum()
    }

    /// The time of a task for one model (its average `tempo` when the model is missing).
    pub fn tempo_modelo(t: &Tarefa, modelo: &str) -> f64 {
        t.tempos.get(modelo).copied().unwrap_or(t.tempo)
    }

    /// The average of the model times weighted by the mix (`tempo` without a mix).
    pub fn tempo_ponderado(&self, t: &Tarefa) -> f64 {
        let total: f64 = self.mix.iter().map(|m| m.parcela).sum();
        if self.mix.is_empty() || total <= 0.0 {
            return t.tempo;
        }
        self.mix
            .iter()
            .map(|m| m.parcela * Self::tempo_modelo(t, &m.id))
            .sum::<f64>()
            / total
    }

    /// The load of each station for each model: `[modelo][estação - 1]`, in the order of
    /// `mix` (empty without a mix).
    pub fn cargas_modelos(&self, a: &BTreeMap<String, u32>) -> Vec<Vec<f64>> {
        self.mix
            .iter()
            .map(|m| {
                let mut c = vec![0.0; self.estacoes as usize];
                for t in &self.tarefas {
                    if let Some(&s) = a.get(&t.id)
                        && s >= 1
                        && (s as usize) <= c.len()
                    {
                        c[s as usize - 1] += Self::tempo_modelo(t, &m.id);
                    }
                }
                c
            })
            .collect()
    }

    /// The largest station load of any single model (the station cycle without a mix).
    pub fn pico(&self, a: &BTreeMap<String, u32>) -> f64 {
        if self.mix.is_empty() {
            return self.resumo(a).ciclo;
        }
        self.cargas_modelos(a)
            .iter()
            .flatten()
            .copied()
            .fold(0.0, f64::max)
    }

    /// The process time text of a station load (Gamma with the coefficient of variation).
    fn tempo_ron(&self, c: f64) -> String {
        if c <= 0.0 || self.cv <= 0.0 {
            format!("Const({})", numero_ron(c))
        } else {
            let shape = 1.0 / (self.cv * self.cv);
            format!(
                "Gamma(shape: {}, scale: {})",
                numero_ron(shape),
                numero_ron(c / shape)
            )
        }
    }

    /// The station of every task in `d` (initial value where missing).
    pub fn atribuicao(&self, d: &Decisoes) -> BTreeMap<String, u32> {
        self.tarefas
            .iter()
            .map(|t| {
                let v = d
                    .get(&Self::chave(&t.id))
                    .map(|v| v.round().max(0.0) as u32)
                    .or_else(|| self.inicial.get(&t.id).copied())
                    .unwrap_or(1);
                (t.id.clone(), v)
            })
            .collect()
    }

    pub fn decisoes(&self, a: &BTreeMap<String, u32>) -> Decisoes {
        a.iter().map(|(k, v)| (Self::chave(k), *v as f64)).collect()
    }

    pub fn resumo(&self, a: &BTreeMap<String, u32>) -> ResumoBalanceamento {
        let mut cargas = vec![0.0; self.estacoes as usize];
        for t in &self.tarefas {
            if let Some(&s) = a.get(&t.id)
                && s >= 1
                && (s as usize) <= cargas.len()
            {
                cargas[s as usize - 1] += t.tempo;
            }
        }
        let usadas = cargas.iter().filter(|c| **c > 0.0).count() as u32;
        let ciclo = cargas.iter().copied().fold(0.0, f64::max);
        let eficiencia = if usadas > 0 {
            self.soma() / (usadas as f64 * self.takt)
        } else {
            0.0
        };
        ResumoBalanceamento {
            cargas,
            estacoes_usadas: usadas,
            ciclo,
            eficiencia,
            atraso: 1.0 - eficiencia,
        }
    }

    /// Why an assignment is not allowed (`None` when it is).
    pub fn violacao(&self, a: &BTreeMap<String, u32>) -> Option<String> {
        for t in &self.tarefas {
            let s = a.get(&t.id).copied().unwrap_or(0);
            if s < 1 || s > self.estacoes {
                return Some(format!(
                    "a tarefa \"{}\" precisa de uma estação de 1 a {}",
                    t.nome, self.estacoes
                ));
            }
        }
        for t in &self.tarefas {
            let s = a[&t.id];
            for p in &t.depois_de {
                let sp = a.get(p).copied().unwrap_or(0);
                if sp > s {
                    let nome_p = self
                        .tarefas
                        .iter()
                        .find(|x| &x.id == p)
                        .map_or(p.as_str(), |x| &x.nome);
                    return Some(format!(
                        "precedência: \"{}\" (estação {s}) vem antes de \"{nome_p}\" (estação {sp}), que precisa ser feita primeiro",
                        t.nome
                    ));
                }
            }
        }
        if !self.permite_passar_takt {
            let r = self.resumo(a);
            for (i, c) in r.cargas.iter().enumerate() {
                if *c > self.takt + 1e-9 {
                    return Some(format!(
                        "a estação {} tem {} de trabalho, mais que o takt de {}",
                        i + 1,
                        numero_ron(*c),
                        numero_ron(self.takt)
                    ));
                }
            }
        }
        None
    }

    /// The model keys: the process time of each station (`Const(0.0)` when empty). With
    /// a mix, each station also gets a recipe with the time of each model (label value
    /// 1, 2... in the order of `mix`), and `process` is the time of the first model.
    pub fn ajustes(&self, a: &BTreeMap<String, u32>) -> BTreeMap<String, String> {
        if self.mix.is_empty() {
            return self
                .resumo(a)
                .cargas
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    (
                        format!("{}{}.process", self.prefixo, i + 1),
                        self.tempo_ron(*c),
                    )
                })
                .collect();
        }
        let cm = self.cargas_modelos(a);
        let mut out = BTreeMap::new();
        for i in 0..self.estacoes as usize {
            let est = format!("{}{}", self.prefixo, i + 1);
            out.insert(format!("{est}.process"), self.tempo_ron(cm[0][i]));
            let tempos: Vec<String> = cm
                .iter()
                .enumerate()
                .map(|(k, c)| format!("(value: {}.0, time: {})", k + 1, self.tempo_ron(c[i])))
                .collect();
            out.insert(
                format!("{est}.recipe"),
                format!(
                    "Some((label: \"{}\", times: [{}]))",
                    self.rotulo,
                    tempos.join(", ")
                ),
            );
        }
        out
    }

    /// Problems of the definition itself (unknown predecessors, cycles).
    pub fn validar(&self) -> Vec<String> {
        let mut erros = Vec::new();
        let ids: BTreeSet<&str> = self.tarefas.iter().map(|t| t.id.as_str()).collect();
        if ids.len() != self.tarefas.len() {
            erros.push("balanceamento: tarefa repetida".into());
        }
        for t in &self.tarefas {
            if t.tempo <= 0.0 {
                erros.push(format!("balanceamento: tarefa '{}' sem tempo", t.id));
            }
            for p in &t.depois_de {
                if !ids.contains(p.as_str()) {
                    erros.push(format!(
                        "balanceamento: '{}' depende de '{p}', que não existe",
                        t.id
                    ));
                }
            }
            if !self.inicial.contains_key(&t.id) {
                erros.push(format!("balanceamento: '{}' sem estação inicial", t.id));
            }
        }
        if self.ordem_topologica().is_none() {
            erros.push("balanceamento: as precedências formam um ciclo".into());
        }
        if self.takt <= 0.0 || self.estacoes == 0 {
            erros.push("balanceamento: takt e número de estações precisam ser positivos".into());
        }
        erros.extend(self.validar_mix());
        erros
    }

    /// Problems of the mix: repeated or empty models, missing model times, a `tempo` that
    /// is not the weighted average of the model times.
    fn validar_mix(&self) -> Vec<String> {
        let mut erros = Vec::new();
        let modelos: BTreeSet<&str> = self.mix.iter().map(|m| m.id.as_str()).collect();
        if modelos.len() != self.mix.len() {
            erros.push("balanceamento: modelo repetido no mix".into());
        }
        if self
            .mix
            .iter()
            .any(|m| m.parcela.is_nan() || m.parcela <= 0.0)
        {
            erros.push("balanceamento: parcela do mix precisa ser positiva".into());
        }
        for t in &self.tarefas {
            if self.mix.is_empty() {
                if !t.tempos.is_empty() {
                    erros.push(format!(
                        "balanceamento: '{}' tem tempos por modelo, mas não há mix",
                        t.id
                    ));
                }
                continue;
            }
            for k in t.tempos.keys() {
                if !modelos.contains(k.as_str()) {
                    erros.push(format!(
                        "balanceamento: '{}' tem tempo do modelo '{k}', que não está no mix",
                        t.id
                    ));
                }
            }
            for m in &self.mix {
                match t.tempos.get(&m.id) {
                    None => erros.push(format!(
                        "balanceamento: '{}' sem tempo do modelo '{}'",
                        t.id, m.id
                    )),
                    Some(v) if *v < 0.0 => erros.push(format!(
                        "balanceamento: '{}' com tempo negativo no modelo '{}'",
                        t.id, m.id
                    )),
                    _ => {}
                }
            }
            let media = self.tempo_ponderado(t);
            if (media - t.tempo).abs() > 0.05 {
                erros.push(format!(
                    "balanceamento: o tempo de '{}' ({}) precisa ser a média ponderada pelo mix ({})",
                    t.id,
                    numero_ron(t.tempo),
                    numero_ron((media * 100.0).round() / 100.0)
                ));
            }
        }
        erros
    }

    /// Tasks in an order that respects the precedences.
    pub fn ordem_topologica(&self) -> Option<Vec<usize>> {
        let n = self.tarefas.len();
        let idx: BTreeMap<&str, usize> = self
            .tarefas
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.as_str(), i))
            .collect();
        let mut grau = vec![0usize; n];
        for t in &self.tarefas {
            grau[idx[t.id.as_str()]] = t
                .depois_de
                .iter()
                .filter(|p| idx.contains_key(p.as_str()))
                .count();
        }
        let mut out = Vec::new();
        let mut feito = vec![false; n];
        while out.len() < n {
            let i = (0..n).find(|&i| !feito[i] && grau[i] == 0)?;
            feito[i] = true;
            out.push(i);
            let id = &self.tarefas[i].id;
            for (j, t) in self.tarefas.iter().enumerate() {
                if t.depois_de.contains(id) {
                    grau[j] -= 1;
                }
            }
        }
        Some(out)
    }

    /// The optimal assignment: fewest stations within the takt, then the smallest cycle.
    /// Exact search (depth first with bounds), for the small problems of the missions.
    pub fn otimo(&self) -> Option<BTreeMap<String, u32>> {
        let ordem = self.ordem_topologica()?;
        let minimo = (self.soma() / self.takt - 1e-9).ceil().max(1.0) as u32;
        for n in minimo..=self.estacoes {
            let mut melhor: Option<(f64, Vec<u32>)> = None;
            let mut atual = vec![0u32; self.tarefas.len()];
            let mut cargas = vec![0.0; n as usize];
            self.busca(&ordem, 0, n, &mut atual, &mut cargas, 0, &mut melhor);
            if let Some((_, v)) = melhor {
                return Some(
                    self.tarefas
                        .iter()
                        .zip(v)
                        .map(|(t, s)| (t.id.clone(), s))
                        .collect(),
                );
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn busca(
        &self,
        ordem: &[usize],
        k: usize,
        n: u32,
        atual: &mut Vec<u32>,
        cargas: &mut Vec<f64>,
        usadas: u32,
        melhor: &mut Option<(f64, Vec<u32>)>,
    ) {
        let ciclo = cargas.iter().copied().fold(0.0, f64::max);
        if melhor.as_ref().is_some_and(|(c, _)| ciclo >= *c - 1e-9) {
            return;
        }
        if k == ordem.len() {
            *melhor = Some((ciclo, atual.clone()));
            return;
        }
        let i = ordem[k];
        let t = &self.tarefas[i];
        let idx = |id: &str| self.tarefas.iter().position(|x| x.id == id);
        let min_s = t
            .depois_de
            .iter()
            .filter_map(|p| idx(p))
            .map(|j| atual[j])
            .max()
            .unwrap_or(1)
            .max(1);
        // Symmetry: never open a station beyond the next unused one.
        let max_s = (usadas + 1).min(n);
        for s in min_s..=max_s {
            let c = cargas[s as usize - 1] + t.tempo;
            if c > self.takt + 1e-9 {
                continue;
            }
            cargas[s as usize - 1] = c;
            atual[i] = s;
            self.busca(ordem, k + 1, n, atual, cargas, usadas.max(s), melhor);
            cargas[s as usize - 1] -= t.tempo;
            atual[i] = 0;
        }
    }
}

/// A layout problem: machines placed on the cells of a grid, flows between them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Arranjo {
    pub colunas: u32,
    pub linhas: u32,
    /// Side of a cell, in the length unit of the model (metres).
    pub celula: f64,
    pub maquinas: Vec<MaquinaArranjo>,
    pub fluxos: Vec<Fluxo>,
    /// The cell of each machine in the initial state.
    pub inicial: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MaquinaArranjo {
    pub id: String,
    pub nome: String,
    /// Cell fixed by the building (a dock): the student cannot move it.
    #[serde(default)]
    pub fixa: bool,
}

/// A product family (or an item) and its route through the machines.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Fluxo {
    pub nome: String,
    /// Trips per period.
    pub volume: f64,
    pub roteiro: Vec<String>,
}

impl Arranjo {
    pub fn chave(id: &str) -> String {
        format!("pos.{id}")
    }

    pub fn celulas(&self) -> u32 {
        self.colunas * self.linhas
    }

    /// Centre of cell `c` on the floor.
    pub fn centro(&self, c: u32) -> [f64; 3] {
        let col = c % self.colunas;
        let lin = c / self.colunas;
        [col as f64 * self.celula, -(lin as f64) * self.celula, 0.0]
    }

    /// Straight-line distance between two cells (the transfers of the model use the
    /// straight line too, so the from-to matrix of the run gives the same distances).
    pub fn distancia(&self, a: u32, b: u32) -> f64 {
        let (p, q) = (self.centro(a), self.centro(b));
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
    }

    pub fn posicoes(&self, d: &Decisoes) -> BTreeMap<String, u32> {
        self.maquinas
            .iter()
            .map(|m| {
                let v = if m.fixa {
                    self.inicial.get(&m.id).copied()
                } else {
                    d.get(&Self::chave(&m.id))
                        .map(|v| v.round().max(0.0) as u32)
                        .or_else(|| self.inicial.get(&m.id).copied())
                }
                .unwrap_or(0);
                (m.id.clone(), v)
            })
            .collect()
    }

    pub fn decisoes(&self, p: &BTreeMap<String, u32>) -> Decisoes {
        self.maquinas
            .iter()
            .filter(|m| !m.fixa)
            .filter_map(|m| p.get(&m.id).map(|c| (Self::chave(&m.id), *c as f64)))
            .collect()
    }

    /// Trips × distance of every flow (the moment of transport).
    pub fn momento(&self, p: &BTreeMap<String, u32>) -> f64 {
        self.fluxos
            .iter()
            .map(|f| {
                f.volume
                    * f.roteiro
                        .windows(2)
                        .map(|w| self.distancia(p[&w[0]], p[&w[1]]))
                        .sum::<f64>()
            })
            .sum()
    }

    /// The from-to matrix: trips and trips × distance per ordered pair of machines.
    pub fn de_para(&self, p: &BTreeMap<String, u32>) -> BTreeMap<(String, String), (f64, f64)> {
        let mut m: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
        for f in &self.fluxos {
            for w in f.roteiro.windows(2) {
                let e = m.entry((w[0].clone(), w[1].clone())).or_default();
                e.0 += f.volume;
                e.1 += f.volume * self.distancia(p[&w[0]], p[&w[1]]);
            }
        }
        m
    }

    pub fn violacao(&self, p: &BTreeMap<String, u32>) -> Option<String> {
        let mut ocupadas: BTreeMap<u32, &str> = BTreeMap::new();
        for m in &self.maquinas {
            let c = p[&m.id];
            if c >= self.celulas() {
                return Some(format!("\"{}\" está fora da grade", m.nome));
            }
            if let Some(outra) = ocupadas.insert(c, &m.nome) {
                return Some(format!(
                    "\"{}\" e \"{outra}\" estão na mesma célula",
                    m.nome
                ));
            }
        }
        None
    }

    /// The model keys: the position of every object named `<maquina>` or `<maquina>__...`.
    pub fn ajustes(
        &self,
        p: &BTreeMap<String, u32>,
        objetos: &[String],
    ) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for o in objetos {
            let base = o.split("__").next().unwrap_or(o);
            if let Some(c) = p.get(base) {
                let [x, y, z] = self.centro(*c);
                out.insert(
                    format!("{o}.position"),
                    format!("({}, {}, {})", numero_ron(x), numero_ron(y), numero_ron(z)),
                );
            }
        }
        out
    }

    pub fn validar(&self) -> Vec<String> {
        let mut erros = Vec::new();
        let ids: BTreeSet<&str> = self.maquinas.iter().map(|m| m.id.as_str()).collect();
        if ids.len() != self.maquinas.len() {
            erros.push("arranjo: máquina repetida".into());
        }
        for f in &self.fluxos {
            for m in &f.roteiro {
                if !ids.contains(m.as_str()) {
                    erros.push(format!(
                        "arranjo: o fluxo '{}' passa por '{m}', que não existe",
                        f.nome
                    ));
                }
            }
        }
        for m in &self.maquinas {
            if !self.inicial.contains_key(&m.id) {
                erros.push(format!("arranjo: '{}' sem célula inicial", m.id));
            }
        }
        if self.maquinas.len() as u32 > self.celulas() {
            erros.push("arranjo: mais máquinas que células".into());
        }
        if let Some(v) = self.violacao(&self.inicial) {
            erros.push(format!("arranjo: estado inicial inválido: {v}"));
        }
        erros
    }

    /// The layout with the smallest moment of transport. When every flow goes from a
    /// fixed machine (the dock) to a free one and back, sorting by volume and distance is
    /// exact; otherwise the search is exhaustive (limited to small grids).
    pub fn otimo(&self) -> Option<BTreeMap<String, u32>> {
        let livres: Vec<&MaquinaArranjo> = self.maquinas.iter().filter(|m| !m.fixa).collect();
        let fixas: BTreeMap<String, u32> = self
            .maquinas
            .iter()
            .filter(|m| m.fixa)
            .map(|m| (m.id.clone(), self.inicial[&m.id]))
            .collect();
        let ocupadas: BTreeSet<u32> = fixas.values().copied().collect();
        let celulas: Vec<u32> = (0..self.celulas())
            .filter(|c| !ocupadas.contains(c))
            .collect();
        // Star flows around one fixed machine.
        let estrela = fixas.len() == 1 && {
            let doca = fixas.keys().next().expect("1");
            self.fluxos
                .iter()
                .all(|f| f.roteiro.iter().filter(|m| *m != doca).count() <= 1)
        };
        if estrela {
            let doca = fixas.keys().next().expect("1").clone();
            let cd = fixas[&doca];
            let mut peso: BTreeMap<&str, f64> =
                livres.iter().map(|m| (m.id.as_str(), 0.0)).collect();
            for f in &self.fluxos {
                let viagens = f.roteiro.windows(2).count() as f64;
                if let Some(m) = f.roteiro.iter().find(|m| **m != doca) {
                    *peso.get_mut(m.as_str())? += f.volume * viagens;
                }
            }
            let mut ms: Vec<(&str, f64)> = peso.into_iter().collect();
            ms.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
            let mut cs = celulas.clone();
            cs.sort_by(|a, b| {
                self.distancia(cd, *a)
                    .total_cmp(&self.distancia(cd, *b))
                    .then(a.cmp(b))
            });
            let mut p = fixas.clone();
            for ((m, _), c) in ms.into_iter().zip(cs) {
                p.insert(m.to_string(), c);
            }
            return Some(p);
        }
        // Exhaustive search with pruning, over injective placements of the free machines.
        // The moment of the pairs already placed only grows as more machines are placed,
        // so a branch whose partial moment already reaches the best one is cut. The order
        // of the search is the declared one, so ties keep the first optimum found.
        let ids: Vec<String> = self.maquinas.iter().map(|m| m.id.clone()).collect();
        let n = ids.len();
        let indice = |id: &str| ids.iter().position(|x| x == id);
        let mut w = vec![vec![0.0; n]; n];
        for f in &self.fluxos {
            for par in f.roteiro.windows(2) {
                let (a, b) = (indice(&par[0])?, indice(&par[1])?);
                if a != b {
                    w[a][b] += f.volume;
                    w[b][a] += f.volume;
                }
            }
        }
        let mut pos: Vec<Option<u32>> = vec![None; n];
        for (id, c) in &fixas {
            pos[indice(id)?] = Some(*c);
        }
        let livres: Vec<usize> = livres
            .iter()
            .map(|m| indice(&m.id))
            .collect::<Option<_>>()?;
        // The pairs between fixed machines do not depend on the search.
        let mut base = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                if let (Some(a), Some(b)) = (pos[i], pos[j]) {
                    base += w[i][j] * self.distancia(a, b);
                }
            }
        }
        let mut busca = Busca {
            arranjo: self,
            ids,
            w,
            livres,
            usada: vec![false; celulas.len()],
            celulas,
            pos,
            nos: 0,
            estourou: false,
            melhor: None,
        };
        busca.descer(0, base);
        if busca.estourou {
            return None;
        }
        busca.melhor.map(|(_, p)| p)
    }
}

/// Nodes the layout search may visit before it gives up (the grid is too large).
const LIMITE_DA_BUSCA: u64 = 30_000_000;

struct Busca<'a> {
    arranjo: &'a Arranjo,
    ids: Vec<String>,
    /// Trips between each pair of machines, both ways.
    w: Vec<Vec<f64>>,
    livres: Vec<usize>,
    celulas: Vec<u32>,
    usada: Vec<bool>,
    pos: Vec<Option<u32>>,
    nos: u64,
    estourou: bool,
    melhor: Option<(f64, BTreeMap<String, u32>)>,
}

impl Busca<'_> {
    fn descer(&mut self, k: usize, parcial: f64) {
        self.nos += 1;
        if self.nos > LIMITE_DA_BUSCA {
            self.estourou = true;
            return;
        }
        if let Some((b, _)) = &self.melhor {
            if parcial - 1e-6 >= *b - 1e-9 {
                return;
            }
        }
        if k == self.livres.len() {
            let p: BTreeMap<String, u32> = self
                .ids
                .iter()
                .zip(&self.pos)
                .filter_map(|(id, c)| c.map(|c| (id.clone(), c)))
                .collect();
            let v = self.arranjo.momento(&p);
            if self.melhor.as_ref().is_none_or(|(b, _)| v < *b - 1e-9) {
                self.melhor = Some((v, p));
            }
            return;
        }
        let m = self.livres[k];
        for i in 0..self.celulas.len() {
            if self.usada[i] {
                continue;
            }
            let c = self.celulas[i];
            let mut extra = 0.0;
            for (j, cj) in self.pos.iter().enumerate() {
                if let Some(cj) = cj {
                    if self.w[m][j] > 0.0 {
                        extra += self.w[m][j] * self.arranjo.distancia(c, *cj);
                    }
                }
            }
            self.usada[i] = true;
            self.pos[m] = Some(c);
            self.descer(k + 1, parcial + extra);
            self.pos[m] = None;
            self.usada[i] = false;
            if self.estourou {
                return;
            }
        }
    }
}

/// Terms of the pieces usable in measures and limits: `balanceamento.estacoes`,
/// `balanceamento.ciclo`, `balanceamento.eficiencia`, `balanceamento.atraso`,
/// `balanceamento.minimo` (the theoretical minimum of stations),
/// `balanceamento.pico` (the largest station load of any single model of a mix) and
/// `arranjo.momento`. `None` for any other key.
pub fn termo(missao: &crate::missao::Missao, chave: &str, d: &Decisoes) -> Option<f64> {
    match chave {
        "arranjo.momento" => {
            let a = missao.arranjo.as_ref()?;
            Some(a.momento(&a.posicoes(d)))
        }
        k if k.starts_with("balanceamento.") => {
            let b = missao.balanceamento.as_ref()?;
            let r = b.resumo(&b.atribuicao(d));
            match &k["balanceamento.".len()..] {
                "estacoes" => Some(r.estacoes_usadas as f64),
                "ciclo" => Some(r.ciclo),
                "eficiencia" => Some(r.eficiencia),
                "atraso" => Some(r.atraso),
                // Theoretical minimum number of stations: ceil(sum of times / takt).
                "minimo" => Some((b.soma() / b.takt - 1e-9).ceil()),
                "pico" => Some(b.pico(&b.atribuicao(d))),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether `chave` names a term of the pieces.
pub fn e_termo(chave: &str) -> bool {
    chave == "arranjo.momento"
        || matches!(
            chave,
            "balanceamento.estacoes"
                | "balanceamento.ciclo"
                | "balanceamento.eficiencia"
                | "balanceamento.atraso"
                | "balanceamento.minimo"
                | "balanceamento.pico"
        )
}
