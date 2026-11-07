//! Two-way ANOVA with interaction (`docs/plano_lean_seis_sigma.md`, 2.3, P2): factors A and
//! B, every combination with the same number `r ≥ 1` of replicates (balanced). With
//! `r ≥ 2` the interaction is tested against the pure error; with `r = 1` the interaction
//! is the error (additive model).

use crate::special::f_sf;

/// One line of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub source: &'static str,
    pub df: f64,
    pub ss: f64,
    pub ms: f64,
    pub f: Option<f64>,
    pub p_value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TwoWay {
    pub levels_a: Vec<String>,
    pub levels_b: Vec<String>,
    pub replicates: usize,
    /// A, B, A×B (with replicates), error and total.
    pub lines: Vec<Line>,
    /// Cell means `[a][b]`, for the interaction plot.
    pub cells: Vec<Vec<f64>>,
}

impl TwoWay {
    pub fn line(&self, source: &str) -> Option<&Line> {
        self.lines.iter().find(|l| l.source == source)
    }
}

/// The two-way ANOVA of `(a, b, y)` observations. `Err` with fewer than two levels of a
/// factor, a missing combination or unequal replicates.
pub fn two_way(obs: &[(String, String, f64)]) -> Result<TwoWay, String> {
    let mut la: Vec<String> = Vec::new();
    let mut lb: Vec<String> = Vec::new();
    for (a, b, _) in obs {
        if !la.contains(a) {
            la.push(a.clone());
        }
        if !lb.contains(b) {
            lb.push(b.clone());
        }
    }
    if la.len() < 2 || lb.len() < 2 {
        return Err("a ANOVA de dois fatores precisa de ao menos dois níveis em cada fator".into());
    }
    let (na, nb) = (la.len(), lb.len());
    let mut cell: Vec<Vec<Vec<f64>>> = vec![vec![Vec::new(); nb]; na];
    for (a, b, y) in obs {
        let i = la.iter().position(|x| x == a).expect("level");
        let j = lb.iter().position(|x| x == b).expect("level");
        cell[i][j].push(*y);
    }
    let r = cell[0][0].len();
    if r == 0 || cell.iter().flatten().any(|c| c.len() != r) {
        return Err(
            "todas as combinações dos níveis precisam do mesmo número de repetições".into(),
        );
    }
    let n = (na * nb * r) as f64;
    let grand = obs.iter().map(|o| o.2).sum::<f64>() / n;
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let cells: Vec<Vec<f64>> = cell
        .iter()
        .map(|row| row.iter().map(|c| mean(c)).collect())
        .collect();
    let ma: Vec<f64> = cells.iter().map(|row| mean(row)).collect();
    let mb: Vec<f64> = (0..nb)
        .map(|j| cells.iter().map(|row| row[j]).sum::<f64>() / na as f64)
        .collect();
    let rf = r as f64;
    let ss_a = nb as f64 * rf * ma.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
    let ss_b = na as f64 * rf * mb.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
    let mut ss_ab = 0.0;
    for i in 0..na {
        for j in 0..nb {
            ss_ab += rf * (cells[i][j] - ma[i] - mb[j] + grand).powi(2);
        }
    }
    let ss_t: f64 = obs.iter().map(|o| (o.2 - grand).powi(2)).sum();
    let df_a = na as f64 - 1.0;
    let df_b = nb as f64 - 1.0;
    let df_ab = df_a * df_b;
    let (err_ss, err_df, inter) = if r >= 2 {
        (ss_t - ss_a - ss_b - ss_ab, n - (na * nb) as f64, true)
    } else {
        (ss_ab, df_ab, false)
    };
    let mse = err_ss.max(0.0) / err_df;
    let test = |source, ss: f64, df: f64| {
        let ms = ss / df;
        let f = (mse > 0.0).then(|| ms / mse);
        Line {
            source,
            df,
            ss,
            ms,
            f,
            p_value: f.map(|f| f_sf(f, df, err_df)),
        }
    };
    let mut lines = vec![test("A", ss_a, df_a), test("B", ss_b, df_b)];
    if inter {
        lines.push(test("A×B", ss_ab, df_ab));
    }
    lines.push(Line {
        source: "erro",
        df: err_df,
        ss: err_ss.max(0.0),
        ms: mse,
        f: None,
        p_value: None,
    });
    lines.push(Line {
        source: "total",
        df: n - 1.0,
        ss: ss_t,
        ms: f64::NAN,
        f: None,
        p_value: None,
    });
    Ok(TwoWay {
        levels_a: la,
        levels_b: lb,
        replicates: r,
        lines,
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(v: &[(&str, &str, f64)]) -> Vec<(String, String, f64)> {
        v.iter()
            .map(|(a, b, y)| (a.to_string(), b.to_string(), *y))
            .collect()
    }

    #[test]
    fn replicated_by_hand() {
        // Same numbers as the 2² of the DOE test: A 12, B 6, AB 2 in effects; SS A 288,
        // B 72, AB 8, error 8 on 4 df.
        let o = obs(&[
            ("a1", "b1", 10.0),
            ("a1", "b1", 12.0),
            ("a2", "b1", 20.0),
            ("a2", "b1", 22.0),
            ("a1", "b2", 14.0),
            ("a1", "b2", 16.0),
            ("a2", "b2", 30.0),
            ("a2", "b2", 28.0),
        ]);
        let t = two_way(&o).unwrap();
        assert_eq!(t.replicates, 2);
        let l = |s| t.line(s).unwrap();
        assert!((l("A").ss - 288.0).abs() < 1e-9);
        assert!((l("B").ss - 72.0).abs() < 1e-9);
        assert!((l("A×B").ss - 8.0).abs() < 1e-9);
        assert!((l("erro").ss - 8.0).abs() < 1e-9 && l("erro").df == 4.0);
        assert!((l("A").f.unwrap() - 144.0).abs() < 1e-9);
        assert_eq!(t.cells, vec![vec![11.0, 15.0], vec![21.0, 29.0]]);
    }

    #[test]
    fn unreplicated_and_refusals() {
        let o = obs(&[
            ("a1", "b1", 1.0),
            ("a1", "b2", 2.0),
            ("a2", "b1", 3.0),
            ("a2", "b2", 5.0),
            ("a3", "b1", 4.0),
            ("a3", "b2", 7.0),
        ]);
        let t = two_way(&o).unwrap();
        assert!(t.line("A×B").is_none());
        assert_eq!(t.line("erro").unwrap().df, 2.0);
        assert!(two_way(&o[..5]).is_err(), "missing combination");
        assert!(two_way(&obs(&[("a", "b", 1.0), ("a", "c", 2.0)])).is_err());
    }
}
