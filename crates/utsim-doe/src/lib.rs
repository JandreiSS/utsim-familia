//! Planejamento de experimentos além dos dois níveis (etapa E3 de
//! `docs/plano_estatistica.md`, decisão D5): os planos de Plackett-Burman para triagem, o
//! composto central (rotacionável, de face centrada ou com `α` dado, num bloco ou em dois
//! com blocagem ortogonal) e o Box-Behnken; o ajuste de um modelo de primeira ou segunda
//! ordem pela regressão de `utsim_stats`, o ponto estacionário com a sua natureza (máximo,
//! mínimo ou sela) e a desejabilidade de várias respostas com a busca do melhor ponto.
//!
//! O fatorial `2^k` e as frações `2^(k−p)` continuam em `utsim_stats::doe`.
//!
//! Os níveis são codificados: `−1` e `+1` nos extremos do fatorial, `0` no centro e `±α`
//! nos pontos axiais. [`Factor`] converte para as unidades do modelo.

use serde::{Deserialize, Serialize};

pub use utsim_stats::regression::Regression;

/// Mais fatores de um composto central (o núcleo é o fatorial completo `2^k`).
pub const MAX_CCD_FACTORS: usize = 6;

// --- fatores ----------------------------------------------------------------------------

/// Um fator com os níveis baixo e alto nas unidades do modelo (os `−1` e `+1` codificados).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Factor {
    pub name: String,
    pub low: f64,
    pub high: f64,
}

impl Factor {
    pub fn new(name: &str, low: f64, high: f64) -> Self {
        Self {
            name: name.to_string(),
            low,
            high,
        }
    }

    pub fn center(&self) -> f64 {
        (self.low + self.high) / 2.0
    }

    pub fn half_range(&self) -> f64 {
        (self.high - self.low) / 2.0
    }

    /// O valor no modelo de um nível codificado.
    pub fn natural(&self, coded: f64) -> f64 {
        self.center() + coded * self.half_range()
    }

    /// Confere que o nível baixo é menor que o alto (senão a codificação divide por zero).
    pub fn check(&self) -> Result<(), String> {
        if self.low.is_finite() && self.high.is_finite() && self.low < self.high {
            Ok(())
        } else {
            Err(format!(
                "o fator {} precisa do nível baixo menor que o alto",
                self.name
            ))
        }
    }

    /// O nível codificado de um valor no modelo.
    pub fn coded(&self, natural: f64) -> f64 {
        (natural - self.center()) / self.half_range()
    }
}

// --- planos -----------------------------------------------------------------------------

/// O papel de um ponto no plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    /// Um vértice do fatorial (ou uma linha de Plackett-Burman, ou uma aresta do Box-Behnken).
    Factorial,
    /// Um ponto axial do composto central (um fator em `±α`, os outros no centro).
    Axial,
    Center,
}

/// Uma corrida do plano.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// A posição na ordem padrão, a partir de 1.
    pub std_order: usize,
    /// A posição na ordem de execução, a partir de 1 (sorteada dentro de cada bloco).
    pub run_order: usize,
    /// O bloco, a partir de 1.
    pub block: usize,
    pub kind: Kind,
    pub coded: Vec<f64>,
}

/// O tipo de plano.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DesignKind {
    PlackettBurman,
    CentralComposite,
    BoxBehnken,
}

/// Um plano com as corridas em ordem padrão.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub kind: DesignKind,
    pub k: usize,
    /// O `α` dos pontos axiais do composto central.
    pub alpha: Option<f64>,
    pub blocks: usize,
    /// A semente do sorteio da ordem de execução.
    pub seed: u64,
    pub points: Vec<Point>,
}

impl Design {
    /// As corridas na ordem de execução.
    pub fn run_sequence(&self) -> Vec<&Point> {
        let mut v: Vec<&Point> = self.points.iter().collect();
        v.sort_by_key(|p| p.run_order);
        v
    }

    /// As linhas codificadas em ordem padrão (para [`Model::fit`]).
    pub fn matrix(&self) -> Vec<Vec<f64>> {
        self.points.iter().map(|p| p.coded.clone()).collect()
    }

    /// O raio da região do plano: `α` no composto central, `√2` no Box-Behnken (as arestas
    /// ficam a essa distância do centro), `1` no Plackett-Burman.
    pub fn radius(&self) -> f64 {
        match self.kind {
            DesignKind::CentralComposite => self.alpha.unwrap_or(1.0),
            DesignKind::BoxBehnken => std::f64::consts::SQRT_2,
            DesignKind::PlackettBurman => 1.0,
        }
    }

    fn new(kind: DesignKind, k: usize, alpha: Option<f64>, seed: u64, rows: Vec<Row>) -> Self {
        let blocks = rows.iter().map(|r| r.block).max().unwrap_or(1);
        let mut points: Vec<Point> = rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| Point {
                std_order: i + 1,
                run_order: 0,
                block: r.block,
                kind: r.kind,
                coded: r.coded,
            })
            .collect();
        // A ordem de execução: os blocos em sequência, as corridas sorteadas dentro de cada um.
        let mut rng = SplitMix64(seed);
        let mut next = 1;
        for b in 1..=blocks {
            let mut idx: Vec<usize> = (0..points.len())
                .filter(|&i| points[i].block == b)
                .collect();
            for i in (1..idx.len()).rev() {
                let j = (rng.next() % (i as u64 + 1)) as usize;
                idx.swap(i, j);
            }
            for i in idx {
                points[i].run_order = next;
                next += 1;
            }
        }
        Self {
            kind,
            k,
            alpha,
            blocks,
            seed,
            points,
        }
    }
}

struct Row {
    block: usize,
    kind: Kind,
    coded: Vec<f64>,
}

/// SplitMix64, para sortear a ordem de execução de uma semente gravada.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

// --- Plackett-Burman --------------------------------------------------------------------

/// As linhas geradoras dos planos cíclicos de Plackett e Burman (1946).
const PB_GENERATORS: [(usize, &str); 3] = [
    (12, "++-+++---+-"),
    (20, "++--++++-+-+----++-"),
    (24, "+++++-+-++--++--+-+----"),
];

/// Os tamanhos de Plackett-Burman disponíveis (8 e 16 pela construção de Sylvester).
pub const PB_RUNS: [usize; 5] = [8, 12, 16, 20, 24];

/// O menor número de corridas de Plackett-Burman para `k` fatores (`None` acima de 23).
pub fn pb_runs(k: usize) -> Option<usize> {
    PB_RUNS.iter().copied().find(|&n| n > k)
}

/// A matriz `n × (n − 1)` de Plackett-Burman com `n` corridas (8, 12, 16, 20 ou 24).
pub fn pb_matrix(n: usize) -> Option<Vec<Vec<i8>>> {
    if let Some((_, g)) = PB_GENERATORS.iter().find(|(m, _)| *m == n) {
        let g: Vec<i8> = g.bytes().map(|c| if c == b'+' { 1 } else { -1 }).collect();
        let m = g.len();
        let mut rows: Vec<Vec<i8>> = (0..m)
            .map(|i| (0..m).map(|j| g[(j + m - i) % m]).collect())
            .collect();
        rows.push(vec![-1; m]);
        return Some(rows);
    }
    if n == 8 || n == 16 {
        // Sylvester: H(2m) = [[H, H], [H, −H]]; sem a primeira coluna (toda +1), e com os
        // sinais trocados para a primeira linha ficar toda em −1, como nos cíclicos.
        let mut h: Vec<Vec<i8>> = vec![vec![1]];
        while h.len() < n {
            let m = h.len();
            let mut g = vec![vec![0i8; 2 * m]; 2 * m];
            for i in 0..m {
                for j in 0..m {
                    g[i][j] = h[i][j];
                    g[i][j + m] = h[i][j];
                    g[i + m][j] = h[i][j];
                    g[i + m][j + m] = -h[i][j];
                }
            }
            h = g;
        }
        return Some(
            h.into_iter()
                .map(|r| r[1..].iter().map(|v| -v).collect())
                .collect(),
        );
    }
    None
}

/// O plano de Plackett-Burman para `k` fatores: as primeiras `k` colunas da matriz com
/// `runs` corridas (`None`: o menor tamanho que cabe), mais `center_points` centros.
pub fn plackett_burman(
    k: usize,
    runs: Option<usize>,
    center_points: usize,
    seed: u64,
) -> Result<Design, String> {
    if k < 2 {
        return Err("o Plackett-Burman precisa de ao menos 2 fatores".into());
    }
    let n = match runs {
        Some(n) => n,
        None => pb_runs(k).ok_or("o Plackett-Burman vai até 23 fatores (24 corridas)")?,
    };
    if n <= k {
        return Err(format!(
            "{n} corridas de Plackett-Burman cabem até {} fatores",
            n.saturating_sub(1)
        ));
    }
    let m = pb_matrix(n).ok_or(format!(
        "não há Plackett-Burman com {n} corridas (use 8, 12, 16, 20 ou 24)"
    ))?;
    let mut rows: Vec<Row> = m
        .into_iter()
        .map(|r| Row {
            block: 1,
            kind: Kind::Factorial,
            coded: r[..k].iter().map(|&v| v as f64).collect(),
        })
        .collect();
    rows.extend(centers(k, center_points, 1));
    Ok(Design::new(DesignKind::PlackettBurman, k, None, seed, rows))
}

fn centers(k: usize, n: usize, block: usize) -> impl Iterator<Item = Row> {
    (0..n).map(move |_| Row {
        block,
        kind: Kind::Center,
        coded: vec![0.0; k],
    })
}

// --- composto central -------------------------------------------------------------------

/// A distância dos pontos axiais do composto central.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Alpha {
    /// `α = (2^k)^(1/4)`: a variância da previsão só depende da distância ao centro.
    Rotatable,
    /// `α = 1`: três níveis por fator, os axiais nas faces do cubo.
    FaceCentered,
    /// Com dois blocos, o `α` que deixa os blocos ortogonais aos termos do modelo.
    OrthogonalBlocks,
    Value(f64),
}

/// O composto central em `k` fatores (2 a 6): o fatorial `2^k`, os `2k` axiais em `±α` e os
/// centros. Com `blocks = 2`, o fatorial com `centers.0` centros fica no bloco 1 e os
/// axiais com `centers.1` centros no bloco 2; com um bloco os centros são `centers.0 +
/// centers.1`.
pub fn central_composite(
    k: usize,
    alpha: Alpha,
    centers_per_block: (usize, usize),
    blocks: usize,
    seed: u64,
) -> Result<Design, String> {
    if !(2..=MAX_CCD_FACTORS).contains(&k) {
        return Err(format!(
            "o composto central vai de 2 a {MAX_CCD_FACTORS} fatores"
        ));
    }
    if !(1..=2).contains(&blocks) {
        return Err("o composto central tem um ou dois blocos".into());
    }
    let nf = (1usize << k) as f64;
    let (c1, c2) = centers_per_block;
    let a = match alpha {
        Alpha::Rotatable => nf.powf(0.25),
        Alpha::FaceCentered => 1.0,
        Alpha::OrthogonalBlocks => {
            if blocks != 2 {
                return Err("o α de blocos ortogonais pede dois blocos".into());
            }
            // Montgomery (2017), eq. 11.17: α = √(n_F (2k + n_ca) / (2 (n_F + n_cf))).
            (nf * (2.0 * k as f64 + c2 as f64) / (2.0 * (nf + c1 as f64))).sqrt()
        }
        Alpha::Value(v) => {
            if !(v.is_finite() && v > 0.0) {
                return Err("o α precisa ser um número positivo".into());
            }
            v
        }
    };
    let b2 = if blocks == 2 { 2 } else { 1 };
    let mut rows: Vec<Row> = utsim_stats::doe::full_factorial(k)
        .into_iter()
        .map(|r| Row {
            block: 1,
            kind: Kind::Factorial,
            coded: r.into_iter().map(f64::from).collect(),
        })
        .collect();
    rows.extend(centers(k, c1, 1));
    for i in 0..k {
        for s in [-a, a] {
            let mut x = vec![0.0; k];
            x[i] = s;
            rows.push(Row {
                block: b2,
                kind: Kind::Axial,
                coded: x,
            });
        }
    }
    rows.extend(centers(k, c2, b2));
    Ok(Design::new(
        DesignKind::CentralComposite,
        k,
        Some(a),
        seed,
        rows,
    ))
}

// --- Box-Behnken ------------------------------------------------------------------------

/// O Box-Behnken em `k` fatores (3 a 5): para cada par de fatores, os quatro pontos `±1`
/// com os outros no centro, e os centros. Nenhuma corrida fica num vértice do cubo.
pub fn box_behnken(k: usize, center_points: usize, seed: u64) -> Result<Design, String> {
    if !(3..=5).contains(&k) {
        return Err("o Box-Behnken vai de 3 a 5 fatores".into());
    }
    let mut rows = Vec::new();
    for i in 0..k {
        for j in i + 1..k {
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let mut x = vec![0.0; k];
                x[i] = a;
                x[j] = b;
                rows.push(Row {
                    block: 1,
                    kind: Kind::Factorial,
                    coded: x,
                });
            }
        }
    }
    rows.extend(centers(k, center_points, 1));
    Ok(Design::new(DesignKind::BoxBehnken, k, None, seed, rows))
}

// --- modelo -----------------------------------------------------------------------------

/// Um termo do modelo nos fatores codificados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Term {
    Linear(usize),
    Interaction(usize, usize),
    Square(usize),
}

impl Term {
    pub fn value(self, x: &[f64]) -> f64 {
        match self {
            Term::Linear(i) => x[i],
            Term::Interaction(i, j) => x[i] * x[j],
            Term::Square(i) => x[i] * x[i],
        }
    }

    /// O nome com as letras dos fatores: `A`, `AB`, `A²`.
    pub fn name(self) -> String {
        let l = utsim_stats::doe::letter;
        match self {
            Term::Linear(i) => l(i).to_string(),
            Term::Interaction(i, j) => format!("{}{}", l(i), l(j)),
            Term::Square(i) => format!("{}²", l(i)),
        }
    }
}

/// Os termos de primeira ordem (efeitos principais, para a triagem).
pub fn linear_terms(k: usize) -> Vec<Term> {
    (0..k).map(Term::Linear).collect()
}

/// Os termos do modelo de segunda ordem completo: lineares, interações e quadrados.
pub fn quadratic_terms(k: usize) -> Vec<Term> {
    let mut t = linear_terms(k);
    for i in 0..k {
        for j in i + 1..k {
            t.push(Term::Interaction(i, j));
        }
    }
    t.extend((0..k).map(Term::Square));
    t
}

/// O modelo ajustado: a constante e um coeficiente por termo.
#[derive(Debug, Clone)]
pub struct Model {
    pub k: usize,
    pub terms: Vec<Term>,
    pub intercept: f64,
    pub coefs: Vec<f64>,
    pub regression: Regression,
}

/// A natureza do ponto estacionário pelos autovalores da parte quadrática.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Nature {
    Maximum,
    Minimum,
    Saddle,
    /// Algum autovalor perto de zero: uma crista, sem ponto único.
    Ridge,
}

/// O ponto estacionário do modelo de segunda ordem.
#[derive(Debug, Clone, PartialEq)]
pub struct Stationary {
    pub coded: Vec<f64>,
    pub predicted: f64,
    pub eigenvalues: Vec<f64>,
    pub nature: Nature,
}

impl Model {
    /// Ajusta `terms` às respostas `y` das linhas codificadas `x` (cada linha uma corrida),
    /// pela regressão de `utsim_stats` com intervalos de confiança `level`.
    pub fn fit(terms: &[Term], x: &[Vec<f64>], y: &[f64], level: f64) -> Result<Self, String> {
        if x.len() != y.len() {
            return Err("o plano e as respostas têm números de corridas diferentes".into());
        }
        let k = x.first().map_or(0, Vec::len);
        if x.iter().any(|r| r.len() != k) || terms.iter().any(|t| !term_fits(*t, k)) {
            return Err("os termos não cabem no número de fatores do plano".into());
        }
        if y.iter().any(|v| !v.is_finite()) {
            return Err("há respostas vazias ou não numéricas".into());
        }
        let names: Vec<String> = terms.iter().map(|t| t.name()).collect();
        let cols: Vec<Vec<f64>> = terms
            .iter()
            .map(|t| x.iter().map(|r| t.value(r)).collect())
            .collect();
        let regression = utsim_stats::regression::fit(&names, &cols, y, level)?;
        let intercept = regression.coefs[0].estimate;
        let coefs = regression.coefs[1..].iter().map(|c| c.estimate).collect();
        Ok(Self {
            k,
            terms: terms.to_vec(),
            intercept,
            coefs,
            regression,
        })
    }

    pub fn predict(&self, x: &[f64]) -> f64 {
        self.intercept
            + self
                .terms
                .iter()
                .zip(&self.coefs)
                .map(|(t, c)| c * t.value(x))
                .sum::<f64>()
    }

    /// O gradiente `b` e a matriz simétrica `B` com `ŷ = b0 + x'b + x'Bx`.
    fn parts(&self) -> (Vec<f64>, Vec<Vec<f64>>) {
        let mut b = vec![0.0; self.k];
        let mut bb = vec![vec![0.0; self.k]; self.k];
        for (t, &c) in self.terms.iter().zip(&self.coefs) {
            match *t {
                Term::Linear(i) => b[i] += c,
                Term::Square(i) => bb[i][i] += c,
                Term::Interaction(i, j) => {
                    bb[i][j] += c / 2.0;
                    bb[j][i] += c / 2.0;
                }
            }
        }
        (b, bb)
    }

    /// O ponto estacionário `x_s = −B⁻¹b/2`. `None` sem termos quadráticos ou com `B`
    /// singular.
    pub fn stationary(&self) -> Option<Stationary> {
        if !self.terms.iter().any(|t| matches!(t, Term::Square(_))) {
            return None;
        }
        let (b, bb) = self.parts();
        let rhs: Vec<f64> = b.iter().map(|v| -v / 2.0).collect();
        let coded = solve(&bb, &rhs)?;
        let eigenvalues = eigen_symmetric(&bb);
        let escala = eigenvalues.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let tol = 1e-6 * escala.max(1e-300);
        let nature = if eigenvalues.iter().any(|v| v.abs() <= tol) {
            Nature::Ridge
        } else if eigenvalues.iter().all(|v| *v < 0.0) {
            Nature::Maximum
        } else if eigenvalues.iter().all(|v| *v > 0.0) {
            Nature::Minimum
        } else {
            Nature::Saddle
        };
        Some(Stationary {
            predicted: self.predict(&coded),
            coded,
            eigenvalues,
            nature,
        })
    }
}

fn term_fits(t: Term, k: usize) -> bool {
    match t {
        Term::Linear(i) | Term::Square(i) => i < k,
        Term::Interaction(i, j) => i < k && j < k && i != j,
    }
}

/// Resolve `A x = b` por eliminação com pivô parcial.
fn solve(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    let mut m: Vec<Vec<f64>> = a
        .iter()
        .zip(b)
        .map(|(r, &v)| {
            let mut r = r.clone();
            r.push(v);
            r
        })
        .collect();
    let escala = a
        .iter()
        .flatten()
        .fold(0.0f64, |s, v| s.max(v.abs()))
        .max(1e-300);
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        if m[p][c].abs() <= 1e-12 * escala {
            return None;
        }
        m.swap(c, p);
        for r in 0..n {
            if r != c {
                let f = m[r][c] / m[c][c];
                let piv = m[c].clone();
                for (x, q) in m[r].iter_mut().zip(&piv) {
                    *x -= f * q;
                }
            }
        }
    }
    Some((0..n).map(|i| m[i][n] / m[i][i]).collect())
}

/// Os autovalores de uma matriz simétrica pelo método de Jacobi, em ordem crescente.
pub fn eigen_symmetric(a: &[Vec<f64>]) -> Vec<f64> {
    let n = a.len();
    let mut m = a.to_vec();
    let total: f64 = a.iter().flatten().map(|v| v * v).sum();
    for _ in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| m[i][j] * m[i][j])
            .sum();
        if off <= 1e-24 * total.max(1e-300) {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if m[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (m[q][q] - m[p][p]) / (2.0 * m[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in m.iter_mut() {
                    let (rp, rq) = (row[p], row[q]);
                    row[p] = c * rp - s * rq;
                    row[q] = s * rp + c * rq;
                }
                let (lp, lq) = (m[p].clone(), m[q].clone());
                for (r, (pr, qr)) in lp.into_iter().zip(lq).enumerate() {
                    m[p][r] = c * pr - s * qr;
                    m[q][r] = s * pr + c * qr;
                }
            }
        }
    }
    let mut v: Vec<f64> = (0..n).map(|i| m[i][i]).collect();
    v.sort_by(f64::total_cmp);
    v
}

// --- desejabilidade ---------------------------------------------------------------------

/// O objetivo de uma resposta (Derringer e Suich, 1980).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Goal {
    /// `d = 0` até `low`, `1` a partir de `high`.
    Maximize { low: f64, high: f64 },
    /// `d = 1` até `low`, `0` a partir de `high`.
    Minimize { low: f64, high: f64 },
    /// `d = 1` em `target`, `0` fora de `[low, high]`.
    Target { low: f64, target: f64, high: f64 },
}

/// A desejabilidade de uma resposta: o objetivo, os pesos da forma da curva (`1`: reta;
/// maior: exige chegar mais perto) e a importância na média geométrica.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Desirability {
    pub goal: Goal,
    /// O peso abaixo do alvo (ou o único, em maximizar e minimizar).
    pub weight: f64,
    /// O peso acima do alvo (só no alvo).
    pub weight_upper: f64,
    pub importance: f64,
}

impl Desirability {
    pub fn new(goal: Goal) -> Self {
        Self {
            goal,
            weight: 1.0,
            weight_upper: 1.0,
            importance: 1.0,
        }
    }

    /// Confere os limites e os pesos.
    pub fn check(&self) -> Result<(), String> {
        let ok = match self.goal {
            Goal::Maximize { low, high } | Goal::Minimize { low, high } => low < high,
            Goal::Target { low, target, high } => low < target && target < high,
        };
        if !ok {
            return Err("os limites da desejabilidade precisam estar em ordem crescente".into());
        }
        if [self.weight, self.weight_upper, self.importance]
            .iter()
            .any(|w| !(w.is_finite() && *w > 0.0))
        {
            return Err("os pesos e a importância precisam ser positivos".into());
        }
        Ok(())
    }

    /// A desejabilidade `d ∈ [0, 1]` de uma resposta `y`.
    pub fn value(&self, y: f64) -> f64 {
        if !y.is_finite() {
            return 0.0;
        }
        // A fração vai a [0, 1] antes da potência: base negativa daria NaN ou d errado.
        let u = |a: f64, b: f64| (a / b).clamp(0.0, 1.0);
        let r = match self.goal {
            Goal::Maximize { low, high } => u(y - low, high - low).powf(self.weight),
            Goal::Minimize { low, high } => u(high - y, high - low).powf(self.weight),
            Goal::Target { low, target, high } => {
                if y <= target {
                    u(y - low, target - low).powf(self.weight)
                } else {
                    u(high - y, high - target).powf(self.weight_upper)
                }
            }
        };
        if r.is_finite() {
            r.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

impl Desirability {
    /// Quanto `y` fica fora da faixa aceitável (`d > 0`), em larguras da faixa; `0` dentro.
    pub fn shortfall(&self, y: f64) -> f64 {
        if !y.is_finite() {
            return f64::INFINITY;
        }
        match self.goal {
            Goal::Maximize { low, high } => (low - y).max(0.0) / (high - low),
            Goal::Minimize { low, high } => (y - high).max(0.0) / (high - low),
            Goal::Target { low, high, .. } => {
                ((low - y).max(0.0) + (y - high).max(0.0)) / (high - low)
            }
        }
    }
}

/// A desejabilidade geral: a média geométrica das `d` com as importâncias como expoentes.
pub fn overall(d: &[f64], importance: &[f64]) -> f64 {
    let total: f64 = importance.iter().sum();
    if d.is_empty() || d.len() != importance.len() || total <= 0.0 {
        return 0.0;
    }
    if d.iter().any(|v| !(v.is_finite() && *v > 0.0)) {
        return 0.0;
    }
    let soma: f64 = d.iter().zip(importance).map(|(v, w)| w * v.ln()).sum();
    (soma / total).exp()
}

/// A região onde a busca procura o melhor ponto, em unidades codificadas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Region {
    /// O cubo `[−r, r]^k` (planos de face centrada, Box-Behnken, triagem).
    Cube(f64),
    /// A esfera de raio `r` (composto rotacionável: `r = α`).
    Sphere(f64),
}

impl Region {
    fn project(self, x: &mut [f64]) {
        match self {
            Region::Cube(r) => x.iter_mut().for_each(|v| *v = v.clamp(-r, r)),
            Region::Sphere(r) => {
                let n = x.iter().map(|v| v * v).sum::<f64>().sqrt();
                if n > r {
                    x.iter_mut().for_each(|v| *v *= r / n);
                }
            }
        }
    }

    fn radius(self) -> f64 {
        match self {
            Region::Cube(r) | Region::Sphere(r) => r,
        }
    }
}

/// O melhor ponto achado pela busca da desejabilidade.
#[derive(Debug, Clone, PartialEq)]
pub struct Optimum {
    pub coded: Vec<f64>,
    pub responses: Vec<f64>,
    pub desirabilities: Vec<f64>,
    pub overall: f64,
}

/// Procura na `region` o ponto de maior desejabilidade geral das respostas previstas por
/// `models` (um por resposta, mesmo número de fatores): uma grade inicial e a busca por
/// coordenadas a partir dos melhores pontos dela. O resultado é determinístico. Com
/// `overall` zero nenhum ponto atende a todos os limites: o ponto devolvido é o que fica
/// menos fora das faixas, e cabe avisar quem usa.
pub fn optimize(
    models: &[Model],
    desirabilities: &[Desirability],
    region: Region,
) -> Result<Optimum, String> {
    if models.is_empty() || models.len() != desirabilities.len() {
        return Err("é preciso uma desejabilidade para cada resposta".into());
    }
    let k = models[0].k;
    if models.iter().any(|m| m.k != k) {
        return Err("os modelos têm números de fatores diferentes".into());
    }
    for d in desirabilities {
        d.check()?;
    }
    let r = region.radius();
    if !(r.is_finite() && r > 0.0) {
        return Err("o raio da região precisa ser positivo".into());
    }
    let imp: Vec<f64> = desirabilities.iter().map(|d| d.importance).collect();
    let eval = |x: &[f64]| -> (Vec<f64>, Vec<f64>, f64) {
        let y: Vec<f64> = models.iter().map(|m| m.predict(x)).collect();
        let d: Vec<f64> = y
            .iter()
            .zip(desirabilities)
            .map(|(v, d)| d.value(*v))
            .collect();
        let o = overall(&d, &imp);
        (y, d, o)
    };
    // A nota da busca: a desejabilidade geral e, para desempatar quando ela é zero (alguma
    // resposta fora dos limites), a média das d menos o quanto as respostas ficam fora das
    // faixas, que ainda aponta para onde melhorar.
    let nota = |x: &[f64]| -> (f64, f64) {
        let (y, d, o) = eval(x);
        let fora: f64 = y
            .iter()
            .zip(desirabilities)
            .map(|(v, d)| d.shortfall(*v))
            .sum();
        (o, d.iter().sum::<f64>() / d.len() as f64 - fora)
    };
    let maior = |a: (f64, f64), b: (f64, f64)| a.0 > b.0 || (a.0 == b.0 && a.1 > b.1);
    // Os pontos iniciais: uma grade de uns 20 mil pontos quando cabe (até 9 fatores); com
    // mais fatores, o centro, os pontos ±r nos eixos e 20 mil pontos sorteados de semente fixa.
    let niveis = ((20_000f64).powf(1.0 / k as f64).floor() as usize).clamp(3, 21);
    let mut pontos: Vec<Vec<f64>> = Vec::new();
    match niveis.checked_pow(k as u32).filter(|t| *t <= 50_000) {
        Some(total) => {
            for idx in 0..total {
                pontos.push(
                    (0..k)
                        .map(|i| {
                            let n = (idx / niveis.pow(i as u32)) % niveis;
                            -r + 2.0 * r * n as f64 / (niveis - 1) as f64
                        })
                        .collect(),
                );
            }
        }
        None => {
            pontos.push(vec![0.0; k]);
            for i in 0..k {
                for s in [-r, r] {
                    let mut x = vec![0.0; k];
                    x[i] = s;
                    pontos.push(x);
                }
            }
            let mut rng = SplitMix64(0x5EED);
            for _ in 0..20_000 {
                pontos.push(
                    (0..k)
                        .map(|_| -r + 2.0 * r * (rng.next() >> 11) as f64 / (1u64 << 53) as f64)
                        .collect(),
                );
            }
        }
    }
    let mut candidatos: Vec<((f64, f64), Vec<f64>)> = pontos
        .into_iter()
        .map(|mut x| {
            region.project(&mut x);
            (nota(&x), x)
        })
        .collect();
    candidatos.sort_by(|a, b| b.0.0.total_cmp(&a.0.0).then(b.0.1.total_cmp(&a.0.1)));
    candidatos.truncate(5);
    let mut melhor: Option<((f64, f64), Vec<f64>)> = None;
    for (mut o, mut x) in candidatos {
        let mut passo = r / (niveis - 1) as f64;
        while passo > 1e-7 * r {
            let mut melhorou = false;
            for i in 0..k {
                for s in [-passo, passo] {
                    let mut z = x.clone();
                    z[i] += s;
                    region.project(&mut z);
                    let oz = nota(&z);
                    if maior(oz, o) {
                        o = oz;
                        x = z;
                        melhorou = true;
                    }
                }
            }
            if !melhorou {
                passo /= 2.0;
            }
        }
        if melhor.as_ref().is_none_or(|(m, _)| maior(o, *m)) {
            melhor = Some((o, x));
        }
    }
    let (_, coded) = melhor.ok_or("a busca não achou pontos")?;
    let (responses, desirabilities, overall) = eval(&coded);
    Ok(Optimum {
        coded,
        responses,
        desirabilities,
        overall,
    })
}

#[cfg(test)]
mod tests;
