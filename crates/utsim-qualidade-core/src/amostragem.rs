//! Acceptance sampling by attributes (stage QS11 of `docs/plano_qualidade_e_seguranca.md`,
//! section 3.3), computed by the program itself from the binomial, hypergeometric and
//! Poisson distributions: probability of acceptance, operating characteristic (OC) curve,
//! average outgoing quality (AOQ) and its limit (AOQL) under rectifying inspection,
//! average total inspection (ATI) and average sample number (ASN).
//!
//! Plans are designed from the producer's risk (acceptable quality level `p1`, risk
//! `alfa`) and the consumer's risk (limiting quality `p2`, risk `beta`). The tables of the
//! sampling standards are **not** embedded: whoever has the right to use them registers
//! the plans by CSV ([`planos_csv`]).

use serde::{Deserialize, Serialize};
use utsim_stats::special::ln_choose;

/// Distribution of the number of defectives in the sample.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Distribuicao {
    /// Sample from a process (or a large lot).
    Binomial,
    /// Sample without replacement from a lot of `lote` items.
    Hipergeometrica { lote: u64 },
    /// Approximation for small `p` and large `n`.
    Poisson,
}

/// A sampling plan.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Plano {
    /// Inspect `n` items, accept the lot with at most `c` defectives.
    Simples { n: u64, c: u64 },
    /// First sample `n1`: accept with at most `c1`, reject with `r1` or more. Otherwise a
    /// second sample `n2`, and accept when the total is at most `c2`.
    Duplo {
        n1: u64,
        c1: u64,
        r1: u64,
        n2: u64,
        c2: u64,
    },
}

/// `P(D = d)` for a sample of `n` with fraction defective `p`.
pub fn prob(d: u64, n: u64, p: f64, dist: Distribuicao) -> f64 {
    let p = p.clamp(0.0, 1.0);
    match dist {
        Distribuicao::Binomial => {
            if d > n {
                return 0.0;
            }
            if p == 0.0 {
                return if d == 0 { 1.0 } else { 0.0 };
            }
            if p == 1.0 {
                return if d == n { 1.0 } else { 0.0 };
            }
            (ln_choose(n as f64, d as f64) + d as f64 * p.ln() + (n - d) as f64 * (1.0 - p).ln())
                .exp()
        }
        Distribuicao::Poisson => {
            let l = n as f64 * p;
            if l == 0.0 {
                return if d == 0 { 1.0 } else { 0.0 };
            }
            (d as f64 * l.ln() - l - utsim_stats::special::ln_gamma(d as f64 + 1.0)).exp()
        }
        Distribuicao::Hipergeometrica { lote } => {
            let big_d = (p * lote as f64).round() as u64;
            hiper(d, n, big_d, lote)
        }
    }
}

/// `P(D = d)` drawing `n` without replacement from `lote` items with `defeituosos`.
fn hiper(d: u64, n: u64, defeituosos: u64, lote: u64) -> f64 {
    if n > lote || d > n || d > defeituosos || n - d > lote - defeituosos {
        return 0.0;
    }
    (ln_choose(defeituosos as f64, d as f64)
        + ln_choose((lote - defeituosos) as f64, (n - d) as f64)
        - ln_choose(lote as f64, n as f64))
    .exp()
}

/// `P(D <= c)`.
pub fn acumulada(c: u64, n: u64, p: f64, dist: Distribuicao) -> f64 {
    (0..=c.min(n))
        .map(|d| prob(d, n, p, dist))
        .sum::<f64>()
        .min(1.0)
}

/// Probabilities of the outcomes of a plan at fraction defective `p`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Desempenho {
    pub p: f64,
    /// Probability of accepting the lot.
    pub pa: f64,
    /// Accepted on the first sample (double plans; the whole `pa` for single ones).
    pub pa1: f64,
    /// Probability of taking a second sample.
    pub p_segunda: f64,
    /// Average sample number.
    pub asn: f64,
    /// Average outgoing quality under rectifying inspection (rejected lots are fully
    /// inspected and the defectives found replaced). Needs a lot size.
    pub aoq: Option<f64>,
    /// Average total inspection per lot. Needs a lot size.
    pub ati: Option<f64>,
}

/// The performance of `plano` at `p`, with the lot size for AOQ and ATI.
pub fn desempenho(plano: Plano, p: f64, dist: Distribuicao, lote: Option<u64>) -> Desempenho {
    let lote = lote.or(match dist {
        Distribuicao::Hipergeometrica { lote } => Some(lote),
        _ => None,
    });
    match plano {
        Plano::Simples { n, c } => {
            let pa = acumulada(c, n, p, dist);
            let (aoq, ati) = match lote {
                Some(big_n) if big_n >= n => (
                    Some(pa * p * (big_n - n) as f64 / big_n as f64),
                    Some(n as f64 + (1.0 - pa) * (big_n - n) as f64),
                ),
                _ => (None, None),
            };
            Desempenho {
                p,
                pa,
                pa1: pa,
                p_segunda: 0.0,
                asn: n as f64,
                aoq,
                ati,
            }
        }
        Plano::Duplo { n1, c1, r1, n2, c2 } => {
            let pa1 = acumulada(c1, n1, p, dist);
            let mut pa2 = 0.0;
            let mut p_segunda = 0.0;
            for d1 in (c1 + 1)..r1.min(n1 + 1) {
                let p1 = prob(d1, n1, p, dist);
                p_segunda += p1;
                if c2 >= d1 {
                    // The second sample comes from what is left of the lot.
                    let dist2 = match dist {
                        Distribuicao::Hipergeometrica { lote } => Distribuicao::Hipergeometrica {
                            lote: lote.saturating_sub(n1),
                        },
                        d => d,
                    };
                    pa2 += p1 * acumulada(c2 - d1, n2, p, dist2);
                }
            }
            let pa = (pa1 + pa2).min(1.0);
            let (aoq, ati) = match lote {
                Some(big_n) if big_n >= n1 + n2 => (
                    Some(
                        p * (pa1 * (big_n - n1) as f64 + pa2 * (big_n - n1 - n2) as f64)
                            / big_n as f64,
                    ),
                    Some(n1 as f64 * pa1 + (n1 + n2) as f64 * pa2 + big_n as f64 * (1.0 - pa)),
                ),
                _ => (None, None),
            };
            Desempenho {
                p,
                pa,
                pa1,
                p_segunda,
                asn: n1 as f64 + n2 as f64 * p_segunda,
                aoq,
                ati,
            }
        }
    }
}

/// The OC curve (and AOQ, ATI) at `pontos` fractions defective from 0 to `p_max`.
pub fn curva(
    plano: Plano,
    dist: Distribuicao,
    lote: Option<u64>,
    p_max: f64,
    pontos: usize,
) -> Vec<Desempenho> {
    let k = pontos.max(2);
    (0..k)
        .map(|i| desempenho(plano, p_max * i as f64 / (k - 1) as f64, dist, lote))
        .collect()
}

/// Average outgoing quality limit: the largest AOQ over the fractions defective, with the
/// fraction where it happens (needs a lot size).
pub fn aoql(plano: Plano, dist: Distribuicao, lote: u64) -> (f64, f64) {
    let mut melhor = (0.0, 0.0);
    // Coarse grid, then a finer one around the maximum.
    let mut lo = 0.0;
    let mut hi = 1.0;
    for _ in 0..4 {
        let pts = curva(plano, dist, Some(lote), hi, 201);
        for d in pts.iter().filter(|d| d.p >= lo) {
            let a = d.aoq.unwrap_or(0.0);
            if a > melhor.0 {
                melhor = (a, d.p);
            }
        }
        let passo = (hi - lo) / 200.0;
        lo = (melhor.1 - 2.0 * passo).max(0.0);
        hi = (melhor.1 + 2.0 * passo).min(1.0);
    }
    melhor
}

/// The two risks a plan must meet.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Riscos {
    /// Acceptable quality level (fraction defective).
    pub p1: f64,
    /// Producer's risk: chance of rejecting a lot at `p1`.
    pub alfa: f64,
    /// Limiting quality (fraction defective).
    pub p2: f64,
    /// Consumer's risk: chance of accepting a lot at `p2`.
    pub beta: f64,
}

impl Riscos {
    pub fn problemas(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (n, v) in [
            ("p1", self.p1),
            ("alfa", self.alfa),
            ("p2", self.p2),
            ("beta", self.beta),
        ] {
            if !(v > 0.0 && v < 1.0) {
                out.push(format!("{n} deve estar entre 0 e 1 (exclusive)"));
            }
        }
        if self.p1 >= self.p2 {
            out.push(
                "o nível de qualidade aceitável p1 deve ser menor que a qualidade limite p2".into(),
            );
        }
        out
    }

    /// Whether `plano` meets both risks.
    pub fn atende(&self, plano: Plano, dist: Distribuicao) -> bool {
        let a = desempenho(plano, self.p1, dist, None).pa;
        let b = desempenho(plano, self.p2, dist, None).pa;
        a >= 1.0 - self.alfa - 1e-12 && b <= self.beta + 1e-12
    }
}

/// Largest sample searched.
pub const N_MAX: u64 = 5000;

/// The single plan with the smallest sample that meets both risks (`None`: no plan up to
/// [`N_MAX`]).
pub fn projetar_simples(r: &Riscos, dist: Distribuicao) -> Option<Plano> {
    if !r.problemas().is_empty() {
        return None;
    }
    let teto = match dist {
        Distribuicao::Hipergeometrica { lote } => lote.min(N_MAX),
        _ => N_MAX,
    };
    for c in 0..=200u64 {
        // Smallest n with Pa(p2) <= beta (Pa falls as n grows): bisection.
        let ok_consumidor = |n: u64| acumulada(c, n, r.p2, dist) <= r.beta + 1e-12;
        if !ok_consumidor(teto) {
            continue;
        }
        let (mut lo, mut hi) = (c.max(1), teto);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if ok_consumidor(mid) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let plano = Plano::Simples { n: lo, c };
        if r.atende(plano, dist) {
            return Some(plano);
        }
    }
    None
}

/// A double plan with two equal samples (`n2 = n1`, `r1 = c2 + 1`) meeting both risks,
/// with the smallest average sample number at `p1` (`None`: none found up to the limits).
pub fn projetar_duplo(r: &Riscos, dist: Distribuicao) -> Option<Plano> {
    if !r.problemas().is_empty() {
        return None;
    }
    let simples = projetar_simples(r, dist)?;
    let Plano::Simples { n: ns, .. } = simples else {
        return None;
    };
    let mut melhor: Option<(f64, Plano)> = None;
    for n1 in 1..=ns {
        for c1 in 0..=n1.min(15) {
            for c2 in (c1 + 1)..=(c1 + 15).min(2 * n1) {
                let plano = Plano::Duplo {
                    n1,
                    c1,
                    r1: c2 + 1,
                    n2: n1,
                    c2,
                };
                if !r.atende(plano, dist) {
                    continue;
                }
                let asn = desempenho(plano, r.p1, dist, None).asn;
                if melhor.is_none_or(|(a, _)| asn < a) {
                    melhor = Some((asn, plano));
                }
                break; // a larger c2 only accepts more: try the next c1
            }
        }
    }
    melhor.map(|(_, p)| p)
}

/// A plan registered by the user: `nome;n;c` (single) or `nome;n1;c1;r1;n2;c2` (double).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanoCadastrado {
    pub nome: String,
    pub plano: Plano,
}

pub fn planos_csv(texto: &str) -> Result<Vec<PlanoCadastrado>, String> {
    let mut out = Vec::new();
    for (k, linha) in texto.trim_start_matches('\u{feff}').lines().enumerate() {
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with("nome") {
            continue;
        }
        let sep = if linha.contains(';') { ';' } else { ',' };
        let c: Vec<&str> = linha.split(sep).map(str::trim).collect();
        let num = |i: usize| -> Result<u64, String> {
            c[i].parse()
                .map_err(|_| format!("linha {}: coluna {} deve ser inteiro", k + 1, i + 1))
        };
        let plano = match c.len() {
            3 => Plano::Simples {
                n: num(1)?,
                c: num(2)?,
            },
            6 => Plano::Duplo {
                n1: num(1)?,
                c1: num(2)?,
                r1: num(3)?,
                n2: num(4)?,
                c2: num(5)?,
            },
            _ => {
                return Err(format!(
                    "linha {}: esperado nome;n;c ou nome;n1;c1;r1;n2;c2",
                    k + 1
                ));
            }
        };
        if let Plano::Duplo { c1, r1, c2, .. } = plano {
            if !(c1 < r1 && c2 >= c1 && r1 <= c2 + 1) {
                return Err(format!("linha {}: deve valer c1 < r1 <= c2 + 1", k + 1));
            }
        }
        out.push(PlanoCadastrado {
            nome: c[0].to_string(),
            plano,
        });
    }
    Ok(out)
}

/// The plan applied to an inspection station of the model, as an approximation the
/// simulator can run item by item: one item in every `lote / n` inspected (the sampling
/// fraction of the plan). The lot decision and the rectifying inspection of rejected lots
/// are not simulated; the report of the plan gives them analytically.
pub fn para_inspecao(
    plano: Plano,
    lote: u64,
    base: &utsim_model::InspectionDef,
) -> utsim_model::InspectionDef {
    let n = match plano {
        Plano::Simples { n, .. } => n,
        Plano::Duplo { n1, .. } => n1,
    };
    let every = (lote as f64 / n.max(1) as f64).round().max(1.0) as u64;
    utsim_model::InspectionDef {
        every,
        ..base.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binom_direto(c: u64, n: u64, p: f64) -> f64 {
        (0..=c)
            .map(|d| utsim_stats::hypothesis::binomial_pmf(d, n, p))
            .sum()
    }

    #[test]
    fn acceptance_matches_the_direct_binomial_and_hypergeometric() {
        for (n, c, p) in [(50, 1, 0.02), (80, 2, 0.05), (125, 3, 0.01), (20, 0, 0.1)] {
            let a = acumulada(c, n, p, Distribuicao::Binomial);
            assert!((a - binom_direto(c, n, p)).abs() < 1e-10, "{n} {c} {p}");
        }
        // Lot of 20 with 2 defectives, sample of 5, accept with 0: C(18,5)/C(20,5).
        let pa = acumulada(0, 5, 0.1, Distribuicao::Hipergeometrica { lote: 20 });
        assert!((pa - 8568.0 / 15504.0).abs() < 1e-12);
        // Poisson: n p = 2, P(D <= 1) = 3 e^-2.
        let pp = acumulada(1, 100, 0.02, Distribuicao::Poisson);
        assert!((pp - 3.0 * (-2.0f64).exp()).abs() < 1e-12);
    }

    #[test]
    fn designed_single_plan_meets_both_risks_with_the_smallest_sample() {
        let r = Riscos {
            p1: 0.01,
            alfa: 0.05,
            p2: 0.06,
            beta: 0.10,
        };
        let plano = projetar_simples(&r, Distribuicao::Binomial).unwrap();
        assert!(r.atende(plano, Distribuicao::Binomial));
        let Plano::Simples { n, c } = plano else {
            panic!()
        };
        // No smaller sample with any acceptance number meets both risks.
        for m in 1..n {
            for k in 0..=m {
                assert!(!r.atende(Plano::Simples { n: m, c: k }, Distribuicao::Binomial));
            }
        }
        assert!(c >= 1);
    }

    #[test]
    fn double_plan_meets_the_risks_and_samples_less_on_average() {
        let r = Riscos {
            p1: 0.01,
            alfa: 0.05,
            p2: 0.06,
            beta: 0.10,
        };
        let s = projetar_simples(&r, Distribuicao::Binomial).unwrap();
        let d = projetar_duplo(&r, Distribuicao::Binomial).unwrap();
        assert!(r.atende(d, Distribuicao::Binomial));
        let asn_d = desempenho(d, r.p1, Distribuicao::Binomial, None).asn;
        let asn_s = desempenho(s, r.p1, Distribuicao::Binomial, None).asn;
        assert!(asn_d <= asn_s);
    }

    #[test]
    fn aoq_and_ati_by_hand() {
        // n = 50, c = 1, N = 1000, p = 0.02: Pa = 0.98^50 + 50 * 0.02 * 0.98^49.
        let pa = 0.98f64.powi(50) + 50.0 * 0.02 * 0.98f64.powi(49);
        let d = desempenho(
            Plano::Simples { n: 50, c: 1 },
            0.02,
            Distribuicao::Binomial,
            Some(1000),
        );
        assert!((d.pa - pa).abs() < 1e-12);
        assert!((d.aoq.unwrap() - pa * 0.02 * 950.0 / 1000.0).abs() < 1e-12);
        assert!((d.ati.unwrap() - (50.0 + (1.0 - pa) * 950.0)).abs() < 1e-9);
        let (limite, onde) = aoql(Plano::Simples { n: 50, c: 1 }, Distribuicao::Binomial, 1000);
        assert!(limite >= d.aoq.unwrap() && onde > 0.0 && onde < 0.2);
    }

    #[test]
    fn oc_curve_falls_and_plans_by_csv() {
        let c = curva(
            Plano::Simples { n: 80, c: 2 },
            Distribuicao::Binomial,
            Some(500),
            0.15,
            31,
        );
        assert_eq!(c[0].pa, 1.0);
        assert!(c.windows(2).all(|w| w[1].pa <= w[0].pa + 1e-12));
        let v = planos_csv("nome;n;c\nempresa A;80;2\nduplo B;50;1;4;50;4\n").unwrap();
        assert_eq!(
            v[1].plano,
            Plano::Duplo {
                n1: 50,
                c1: 1,
                r1: 4,
                n2: 50,
                c2: 4
            }
        );
        assert!(planos_csv("x;1;2;3\n").is_err());
        assert!(planos_csv("x;10;3;2;10;4\n").is_err());
    }
}
