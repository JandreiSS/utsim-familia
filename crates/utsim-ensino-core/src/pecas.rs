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
}

fn prefixo_estacao() -> String {
    "est".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tarefa {
    pub id: String,
    pub nome: String,
    pub tempo: f64,
    /// Tasks that must be done before this one.
    #[serde(default)]
    pub depois_de: Vec<String>,
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

    /// The model keys: the process time of each station (`Const(0.0)` when empty).
    pub fn ajustes(&self, a: &BTreeMap<String, u32>) -> BTreeMap<String, String> {
        let r = self.resumo(a);
        r.cargas
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let txt = if *c <= 0.0 || self.cv <= 0.0 {
                    format!("Const({})", numero_ron(*c))
                } else {
                    let shape = 1.0 / (self.cv * self.cv);
                    format!(
                        "Gamma(shape: {}, scale: {})",
                        numero_ron(shape),
                        numero_ron(c / shape)
                    )
                };
                (format!("{}{}.process", self.prefixo, i + 1), txt)
            })
            .collect()
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
        // Exhaustive: injective placements of the free machines.
        let mut total: f64 = 1.0;
        for i in 0..livres.len() {
            total *= (celulas.len() - i) as f64;
        }
        if total > 3.0e6 {
            return None;
        }
        let mut melhor: Option<(f64, BTreeMap<String, u32>)> = None;
        let mut p = fixas.clone();
        let mut usada = vec![false; celulas.len()];
        self.permutar(&livres, 0, &celulas, &mut usada, &mut p, &mut melhor);
        melhor.map(|(_, p)| p)
    }

    fn permutar(
        &self,
        livres: &[&MaquinaArranjo],
        k: usize,
        celulas: &[u32],
        usada: &mut Vec<bool>,
        p: &mut BTreeMap<String, u32>,
        melhor: &mut Option<(f64, BTreeMap<String, u32>)>,
    ) {
        if k == livres.len() {
            let v = self.momento(p);
            if melhor.as_ref().is_none_or(|(b, _)| v < *b - 1e-9) {
                *melhor = Some((v, p.clone()));
            }
            return;
        }
        for (i, c) in celulas.iter().enumerate() {
            if usada[i] {
                continue;
            }
            usada[i] = true;
            p.insert(livres[k].id.clone(), *c);
            self.permutar(livres, k + 1, celulas, usada, p, melhor);
            usada[i] = false;
        }
        p.remove(&livres[k].id);
    }
}

/// Terms of the pieces usable in measures and limits: `balanceamento.estacoes`,
/// `balanceamento.ciclo`, `balanceamento.eficiencia`, `balanceamento.atraso`,
/// `balanceamento.minimo` (the theoretical minimum of stations) and
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
        )
}
