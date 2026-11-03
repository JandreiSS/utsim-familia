//! Measurement system analysis for attributes: agreement percentages with exact
//! (Clopper-Pearson) intervals, Cohen's kappa (two ratings) and Fleiss' kappa (several).
//!
//! - **Cohen (1960)**: `κ = (p_o − p_e)/(1 − p_e)` from the square table of the two
//!   ratings; standard error under `κ = 0` by Fleiss, Cohen and Everitt (1969):
//!   `√(p_e + p_e² − Σ p_i. p_.i (p_i. + p_.i)) / ((1 − p_e)√n)`.
//! - **Fleiss (1971)**: `n` items, each rated by `m` raters into `k` categories;
//!   `P̄ = Σᵢ Σⱼ nᵢⱼ(nᵢⱼ − 1)/(n m (m − 1))`, `P̄_e = Σ pⱼ²`, `κ = (P̄ − P̄_e)/(1 − P̄_e)`; standard
//!   error under `κ = 0` and per-category kappas by Fleiss, Nee and Landis (1979).

use serde::{Deserialize, Serialize};

use crate::prob::{clopper_pearson, z_two_sided};

/// Kappa with its test of `κ = 0`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kappa {
    pub kappa: f64,
    /// Observed agreement.
    pub p_o: f64,
    /// Agreement expected by chance.
    pub p_e: f64,
    /// Standard error under `κ = 0`.
    pub se0: f64,
    pub z: f64,
    pub p_value: f64,
}

/// Cohen's kappa of two ratings of the same items (any labels).
pub fn cohen<T: PartialEq + Clone>(a: &[T], b: &[T]) -> Result<Kappa, String> {
    if a.len() != b.len() || a.is_empty() {
        return Err("as duas avaliações devem ter o mesmo número de itens (> 0)".into());
    }
    let mut cats: Vec<T> = Vec::new();
    for v in a.iter().chain(b) {
        if !cats.contains(v) {
            cats.push(v.clone());
        }
    }
    let k = cats.len();
    let n = a.len() as f64;
    let mut table = vec![vec![0.0; k]; k];
    for (x, y) in a.iter().zip(b) {
        let i = cats.iter().position(|c| c == x).unwrap();
        let j = cats.iter().position(|c| c == y).unwrap();
        table[i][j] += 1.0 / n;
    }
    let row: Vec<f64> = table.iter().map(|r| r.iter().sum()).collect();
    let col: Vec<f64> = (0..k).map(|j| table.iter().map(|r| r[j]).sum()).collect();
    let p_o: f64 = (0..k).map(|i| table[i][i]).sum();
    let p_e: f64 = (0..k).map(|i| row[i] * col[i]).sum();
    let kappa = if p_e < 1.0 {
        (p_o - p_e) / (1.0 - p_e)
    } else {
        1.0
    };
    let extra: f64 = (0..k).map(|i| row[i] * col[i] * (row[i] + col[i])).sum();
    let se0 = if p_e < 1.0 {
        (p_e + p_e * p_e - extra).max(0.0).sqrt() / ((1.0 - p_e) * n.sqrt())
    } else {
        f64::NAN
    };
    let z = kappa / se0;
    Ok(Kappa {
        kappa,
        p_o,
        p_e,
        se0,
        z,
        p_value: z_two_sided(z),
    })
}

/// Fleiss' kappa with the per-category kappas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fleiss {
    pub overall: Kappa,
    pub items: usize,
    pub raters: usize,
    /// `(category index, proportion pⱼ, κⱼ, z, p-value)`.
    pub per_category: Vec<(usize, f64, f64, f64, f64)>,
}

/// Fleiss' kappa from `counts[i][j]`: how many of the `m` raters put item `i` in
/// category `j` (every row must add to the same `m ≥ 2`).
pub fn fleiss(counts: &[Vec<usize>]) -> Result<Fleiss, String> {
    let n = counts.len();
    if n == 0 {
        return Err("nenhum item".into());
    }
    let k = counts[0].len();
    let m: usize = counts[0].iter().sum();
    if m < 2 {
        return Err("o kappa de Fleiss exige ao menos 2 avaliações por item".into());
    }
    if counts
        .iter()
        .any(|r| r.len() != k || r.iter().sum::<usize>() != m)
    {
        return Err(format!(
            "todos os itens devem ter {m} avaliações nas mesmas {k} categorias"
        ));
    }
    let (nf, mf) = (n as f64, m as f64);
    let p: Vec<f64> = (0..k)
        .map(|j| counts.iter().map(|r| r[j] as f64).sum::<f64>() / (nf * mf))
        .collect();
    let pbar = counts
        .iter()
        .map(|r| {
            r.iter()
                .map(|&c| (c * c.saturating_sub(1)) as f64)
                .sum::<f64>()
        })
        .sum::<f64>()
        / (nf * mf * (mf - 1.0));
    let pe: f64 = p.iter().map(|x| x * x).sum();
    let kappa = if pe < 1.0 {
        (pbar - pe) / (1.0 - pe)
    } else {
        1.0
    };
    let pq: Vec<f64> = p.iter().map(|x| x * (1.0 - x)).collect();
    let spq: f64 = pq.iter().sum();
    let inner: f64 = spq * spq - (0..k).map(|j| pq[j] * ((1.0 - p[j]) - p[j])).sum::<f64>();
    let se0 = if spq > 0.0 {
        2f64.sqrt() / (spq * (nf * mf * (mf - 1.0)).sqrt()) * inner.max(0.0).sqrt()
    } else {
        f64::NAN
    };
    let z = kappa / se0;
    let se_j = (2.0 / (nf * mf * (mf - 1.0))).sqrt();
    let per_category = (0..k)
        .filter(|&j| pq[j] > 0.0)
        .map(|j| {
            let dis: f64 = counts.iter().map(|r| (r[j] * (m - r[j])) as f64).sum();
            let kj = 1.0 - dis / (nf * mf * (mf - 1.0) * pq[j]);
            let zj = kj / se_j;
            (j, p[j], kj, zj, z_two_sided(zj))
        })
        .collect();
    Ok(Fleiss {
        overall: Kappa {
            kappa,
            p_o: pbar,
            p_e: pe,
            se0,
            z,
            p_value: z_two_sided(z),
        },
        items: n,
        raters: m,
        per_category,
    })
}

/// One rating of an attribute agreement study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    pub item: String,
    pub appraiser: String,
    pub response: String,
    /// Known correct answer for the item, when there is one.
    pub standard: Option<String>,
}

/// Agreement percentage with its exact interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agreement {
    pub matched: usize,
    pub inspected: usize,
    pub percent: f64,
    pub ci: (f64, f64),
}

fn agreement(matched: usize, inspected: usize, level: f64) -> Agreement {
    let (lo, hi) = clopper_pearson(matched, inspected, level);
    Agreement {
        matched,
        inspected,
        percent: 100.0 * matched as f64 / inspected.max(1) as f64,
        ci: (100.0 * lo, 100.0 * hi),
    }
}

/// Results of one appraiser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppraiserResult {
    pub appraiser: String,
    /// Items where all the appraiser's trials agree (needs 2+ trials).
    pub within: Option<Agreement>,
    /// Items where all trials agree with the standard.
    pub vs_standard: Option<Agreement>,
    /// Fleiss' kappa of the appraiser's trials.
    pub kappa_within: Option<f64>,
    /// Cohen's kappa of all the appraiser's ratings against the standard.
    pub kappa_vs_standard: Option<f64>,
}

/// Attribute agreement study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttributeStudy {
    pub items: usize,
    pub appraisers: Vec<AppraiserResult>,
    pub trials: usize,
    pub categories: Vec<String>,
    /// Items where every rating of every appraiser agrees.
    pub between: Agreement,
    /// Items where every rating agrees with the standard.
    pub all_vs_standard: Option<Agreement>,
    /// Fleiss' kappa over all ratings (every appraiser and trial as a rater).
    pub fleiss_all: Option<Fleiss>,
    pub warnings: Vec<String>,
}

/// Attribute agreement analysis. The trial number is the order of the ratings of the
/// same item by the same appraiser; every appraiser must rate every item the same number
/// of times.
pub fn attribute_agreement(ratings: &[Rating], level: f64) -> Result<AttributeStudy, String> {
    let mut items: Vec<String> = Vec::new();
    let mut apps: Vec<String> = Vec::new();
    let mut cats: Vec<String> = Vec::new();
    for r in ratings {
        if !items.contains(&r.item) {
            items.push(r.item.clone());
        }
        if !apps.contains(&r.appraiser) {
            apps.push(r.appraiser.clone());
        }
        for c in std::iter::once(&r.response).chain(r.standard.iter()) {
            if !cats.contains(c) {
                cats.push(c.clone());
            }
        }
    }
    if items.len() < 2 || apps.is_empty() {
        return Err("o estudo exige ao menos 2 itens e 1 avaliador".into());
    }
    // grid[a][i] = responses (category index) in order.
    let mut grid = vec![vec![Vec::new(); items.len()]; apps.len()];
    let mut standard: Vec<Option<usize>> = vec![None; items.len()];
    let mut warnings = Vec::new();
    for r in ratings {
        let a = apps.iter().position(|x| *x == r.appraiser).unwrap();
        let i = items.iter().position(|x| *x == r.item).unwrap();
        let c = cats.iter().position(|x| *x == r.response).unwrap();
        grid[a][i].push(c);
        if let Some(s) = &r.standard {
            let sc = cats.iter().position(|x| x == s).unwrap();
            if standard[i].is_some_and(|old| old != sc) {
                return Err(format!("item {}: padrão informado de dois modos", r.item));
            }
            standard[i] = Some(sc);
        }
    }
    let trials = grid[0][0].len();
    if trials == 0 || grid.iter().flatten().any(|v| v.len() != trials) {
        return Err(
            "estudo desbalanceado: cada avaliador deve avaliar cada item o mesmo número de vezes"
                .into(),
        );
    }
    let has_standard = standard.iter().all(Option::is_some);
    if !has_standard && standard.iter().any(Option::is_some) {
        warnings.push(
            "padrão informado só para alguns itens: comparações com o padrão omitidas".into(),
        );
    }
    let k = cats.len();
    let n_items = items.len();
    let counts_of = |rows: &dyn Fn(usize) -> Vec<usize>| -> Vec<Vec<usize>> {
        (0..n_items)
            .map(|i| {
                let mut c = vec![0; k];
                for v in rows(i) {
                    c[v] += 1;
                }
                c
            })
            .collect()
    };
    let mut appraisers = Vec::new();
    for (a, name) in apps.iter().enumerate() {
        let within = (trials >= 2).then(|| {
            let m = grid[a]
                .iter()
                .filter(|v| v.iter().all(|&x| x == v[0]))
                .count();
            agreement(m, n_items, level)
        });
        let kappa_within = if trials >= 2 {
            fleiss(&counts_of(&|i| grid[a][i].clone()))
                .ok()
                .map(|f| f.overall.kappa)
        } else {
            None
        };
        let (vs_standard, kappa_vs_standard) = if has_standard {
            let m = (0..n_items)
                .filter(|&i| grid[a][i].iter().all(|&x| Some(x) == standard[i]))
                .count();
            let ratings: Vec<usize> = (0..n_items).flat_map(|i| grid[a][i].clone()).collect();
            let truth: Vec<usize> = (0..n_items)
                .flat_map(|i| std::iter::repeat_n(standard[i].unwrap(), trials))
                .collect();
            (
                Some(agreement(m, n_items, level)),
                cohen(&ratings, &truth).ok().map(|k| k.kappa),
            )
        } else {
            (None, None)
        };
        appraisers.push(AppraiserResult {
            appraiser: name.clone(),
            within,
            vs_standard,
            kappa_within,
            kappa_vs_standard,
        });
    }
    let all_of =
        |i: usize| -> Vec<usize> { (0..apps.len()).flat_map(|a| grid[a][i].clone()).collect() };
    let between_m = (0..n_items)
        .filter(|&i| {
            let v = all_of(i);
            v.iter().all(|&x| x == v[0])
        })
        .count();
    let all_vs_standard = has_standard.then(|| {
        let m = (0..n_items)
            .filter(|&i| all_of(i).iter().all(|&x| Some(x) == standard[i]))
            .count();
        agreement(m, n_items, level)
    });
    let fleiss_all = if apps.len() * trials >= 2 {
        fleiss(&counts_of(&all_of)).ok()
    } else {
        None
    };
    Ok(AttributeStudy {
        items: n_items,
        appraisers,
        trials,
        categories: cats,
        between: agreement(between_m, n_items, level),
        all_vs_standard,
        fleiss_all,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cohen_by_hand() {
        // 50 items: yes/yes 20, yes/no 5, no/yes 10, no/no 15.
        // p_o = 35/50 = 0.7; rows 0.5/0.5, columns 0.6/0.4: p_e = 0.3 + 0.2 = 0.5; κ = 0.4.
        let mut a = Vec::new();
        let mut b = Vec::new();
        for (x, y, n) in [
            ("s", "s", 20),
            ("s", "n", 5),
            ("n", "s", 10),
            ("n", "n", 15),
        ] {
            for _ in 0..n {
                a.push(x);
                b.push(y);
            }
        }
        let k = cohen(&a, &b).unwrap();
        assert!((k.p_o - 0.7).abs() < 1e-12);
        assert!((k.p_e - 0.5).abs() < 1e-12);
        assert!((k.kappa - 0.4).abs() < 1e-12);
        // se0 = √(0.5 + 0.25 − [0.5·0.6·1.1 + 0.5·0.4·0.9]) / (0.5·√50) = √0.24/(0.5√50).
        assert!((k.se0 - 0.24f64.sqrt() / (0.5 * 50f64.sqrt())).abs() < 1e-12);
        assert!(k.p_value < 0.01);
    }

    #[test]
    fn perfect_and_chance_agreement() {
        let a = ["x", "y", "z", "x", "y"];
        assert!((cohen(&a, &a).unwrap().kappa - 1.0).abs() < 1e-12);
        let f = fleiss(&[vec![3, 0], vec![0, 3], vec![3, 0]]).unwrap();
        assert!((f.overall.kappa - 1.0).abs() < 1e-12);
        // Two raters always disagreeing on two balanced categories: κ = −1.
        let b = ["y", "x", "y", "x"];
        let c = ["x", "y", "x", "y"];
        assert!((cohen(&b, &c).unwrap().kappa + 1.0).abs() < 1e-12);
    }

    #[test]
    fn fleiss_by_hand() {
        // 4 items, 3 raters, 2 categories:
        //   item 1: 3, 0   item 2: 2, 1   item 3: 1, 2   item 4: 0, 3
        // pⱼ = 6/12 = 0.5 each; Pᵢ = 1, 1/3, 1/3, 1 → P̄ = 2/3; P̄e = 0.5; κ = 1/3.
        let f = fleiss(&[vec![3, 0], vec![2, 1], vec![1, 2], vec![0, 3]]).unwrap();
        assert!((f.overall.p_o - 2.0 / 3.0).abs() < 1e-12);
        assert!((f.overall.kappa - 1.0 / 3.0).abs() < 1e-12);
        // With two categories both per-category kappas equal the overall one.
        for &(_, _, kj, _, _) in &f.per_category {
            assert!((kj - 1.0 / 3.0).abs() < 1e-12);
        }
        // se0: Σpq = 0.5, inner = 0.25 − Σ pq(q − p) = 0.25, √2/(0.5√(4·3·2))·0.5.
        assert!((f.overall.se0 - 2f64.sqrt() / (0.5 * 24f64.sqrt()) * 0.5).abs() < 1e-12);
    }

    #[test]
    fn fleiss_with_two_raters_and_equal_marginals_matches_cohen() {
        // Fleiss with m = 2 is Scott's pi; with equal marginals it equals Cohen's kappa.
        let a = ["s", "s", "n", "n", "s", "n"];
        let b = ["s", "n", "n", "s", "s", "n"];
        let counts: Vec<Vec<usize>> = a
            .iter()
            .zip(&b)
            .map(|(x, y)| {
                let mut c = vec![0, 0];
                for v in [x, y] {
                    c[if *v == "s" { 0 } else { 1 }] += 1;
                }
                c
            })
            .collect();
        let f = fleiss(&counts).unwrap();
        let k = cohen(&a, &b).unwrap();
        assert!((f.overall.kappa - k.kappa).abs() < 1e-12);
    }

    #[test]
    fn agreement_study() {
        let mut ratings = Vec::new();
        let truth = ["ok", "ok", "nok", "nok", "ok"];
        for (app, answers) in [
            (
                "Ana",
                [
                    ["ok", "ok"],
                    ["ok", "ok"],
                    ["nok", "nok"],
                    ["nok", "nok"],
                    ["ok", "ok"],
                ],
            ),
            (
                "Bia",
                [
                    ["ok", "ok"],
                    ["ok", "nok"],
                    ["nok", "nok"],
                    ["nok", "nok"],
                    ["ok", "ok"],
                ],
            ),
        ] {
            for (i, item) in answers.iter().enumerate() {
                for r in item {
                    ratings.push(Rating {
                        item: format!("i{i}"),
                        appraiser: app.into(),
                        response: r.to_string(),
                        standard: Some(truth[i].into()),
                    });
                }
            }
        }
        let s = attribute_agreement(&ratings, 0.95).unwrap();
        assert_eq!(s.trials, 2);
        assert_eq!(s.appraisers[0].within.as_ref().unwrap().matched, 5);
        assert_eq!(s.appraisers[1].within.as_ref().unwrap().matched, 4);
        assert_eq!(s.appraisers[0].vs_standard.as_ref().unwrap().matched, 5);
        assert!((s.appraisers[0].kappa_vs_standard.unwrap() - 1.0).abs() < 1e-12);
        assert_eq!(s.between.matched, 4);
        assert_eq!(s.all_vs_standard.as_ref().unwrap().matched, 4);
        let ci = s.between.ci;
        assert!(ci.0 < 80.0 && 80.0 < ci.1);
    }
}
