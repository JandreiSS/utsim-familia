//! Classic man-machine and work-measurement formulas.
//!
//! - Synchronous servicing (an operator tending identical machines in a fixed rotation):
//!   with `a` the operator time concurrent with each machine (loading, unloading, the
//!   machine stopped waiting for the operator), `b` the operator time independent of the
//!   machines per machine served (walking to the next one, inspecting, packing) and `t`
//!   the automatic time of the machine, the number of machines one operator can tend
//!   without idle machines is `n' = (a + t)/(a + b)`. With `n ≤ n'` machines the cycle is
//!   `a + t` (the machines set the pace and the operator waits); with `n > n'` it is
//!   `n·(a + b)` (the operator sets the pace and the machines wait). See A. Freivalds and
//!   B. W. Niebel, *Niebel's Methods, Standards, and Work Design*, ch. 2 ("Synchronous
//!   servicing", `n' = (l + m)/(l + w)`, with `l` = loading, `m` = machine time,
//!   `w` = walking); M. P. Groover, *Work Systems and the Methods, Measurement, and
//!   Management of Work*, ch. 4 ("Worker-machine systems").
//! - Cost per unit with `n` machines: `TEC(n) = (K1 + n·K2)·cycle(n)/n`, `K1` the hourly
//!   cost of the operator and `K2` of one machine; the optimum is the cheaper of
//!   `⌊n'⌋` and `⌊n'⌋ + 1` (Freivalds and Niebel, same section).
//! - Standard time: normal time = observed time × rating, standard time = normal time ×
//!   (1 + allowances), the allowances (personal, fatigue and delay, PF&D) as a fraction of
//!   the normal time. See R. M. Barnes, *Motion and Time Study: Design and Measurement of
//!   Work*, chs. on performance rating and allowances; Freivalds and Niebel, chs. 9 and 11;
//!   International Labour Office, *Introduction to Work Study* (G. Kanawaty, ed.), part 3.

use serde::Serialize;

/// The three times of the synchronous servicing model, per machine and per cycle.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Abt {
    /// Concurrent operator-machine time (load, unload, setup with the operator).
    pub a: f64,
    /// Independent operator time per machine (walking included).
    pub b: f64,
    /// Automatic machine time.
    pub t: f64,
}

/// One number of machines in the synchronous servicing model.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Arranjo {
    pub maquinas: usize,
    pub ciclo: f64,
    pub utilizacao_operador: f64,
    pub utilizacao_maquina: f64,
    /// Operator idle time per cycle (`n ≤ n'`).
    pub operador_ocioso: f64,
    /// Idle time of each machine per cycle (`n > n'`).
    pub maquina_ociosa: f64,
    /// Cost per unit, when the hourly costs are known.
    pub custo_por_unidade: Option<f64>,
}

impl Abt {
    /// `n' = (a + t)/(a + b)`; infinite when the operator has no time per machine.
    pub fn n_linha(&self) -> f64 {
        let den = self.a + self.b;
        if den > 0.0 {
            (self.a + self.t) / den
        } else {
            f64::INFINITY
        }
    }

    /// Cycle, utilisations and idle times with `n` machines; `custos = (K1, K2)` per hour
    /// and `horas` the length of one model time unit in hours.
    pub fn arranjo(&self, n: usize, custos: Option<(f64, f64)>, horas: f64) -> Arranjo {
        let n = n.max(1);
        let nf = n as f64;
        let maquina = self.a + self.t;
        let operador = nf * (self.a + self.b);
        let ciclo = maquina.max(operador);
        let (uo, um) = if ciclo > 0.0 {
            (operador / ciclo, maquina / ciclo)
        } else {
            (0.0, 0.0)
        };
        Arranjo {
            maquinas: n,
            ciclo,
            utilizacao_operador: uo,
            utilizacao_maquina: um,
            operador_ocioso: ciclo - operador,
            maquina_ociosa: ciclo - maquina,
            custo_por_unidade: custos.map(|(k1, k2)| (k1 + nf * k2) * ciclo * horas / nf),
        }
    }

    /// The arrangements worth showing: `⌊n'⌋`, `⌊n'⌋ + 1` and the current number, in order,
    /// without repeats.
    pub fn arranjos(&self, atual: usize, custos: Option<(f64, f64)>, horas: f64) -> Vec<Arranjo> {
        let nl = self.n_linha();
        let n1 = if nl.is_finite() {
            (nl.floor() as usize).max(1)
        } else {
            atual.max(1)
        };
        let mut ns = vec![n1, n1 + 1, atual.max(1)];
        ns.sort_unstable();
        ns.dedup();
        ns.into_iter()
            .map(|n| self.arranjo(n, custos, horas))
            .collect()
    }

    /// The cheaper of `⌊n'⌋` and `⌊n'⌋ + 1` (the textbook comparison), when costs are known.
    pub fn otimo(&self, custos: (f64, f64), horas: f64) -> Option<Arranjo> {
        let nl = self.n_linha();
        if !nl.is_finite() {
            return None;
        }
        let n1 = (nl.floor() as usize).max(1);
        let a = self.arranjo(n1, Some(custos), horas);
        let b = self.arranjo(n1 + 1, Some(custos), horas);
        Some(if b.custo_por_unidade < a.custo_por_unidade {
            b
        } else {
            a
        })
    }
}

/// Normal time: observed × rating (`1.0` = 100 %).
pub fn tempo_normal(observado: f64, ritmo: f64) -> f64 {
    observado * ritmo
}

/// Standard time: normal × (1 + allowance), the allowance as a fraction of the normal
/// time (`0.15` = 15 %).
pub fn tempo_padrao(normal: f64, tolerancia: f64) -> f64 {
    normal * (1.0 + tolerancia)
}

/// Length of one model time unit in hours: `s`, `min` or `h`.
pub fn horas_por_unidade(unidade: &str) -> Option<f64> {
    match unidade.trim().to_lowercase().as_str() {
        "s" | "seg" | "segundo" | "segundos" => Some(1.0 / 3600.0),
        "min" | "minuto" | "minutos" => Some(1.0 / 60.0),
        "h" | "hora" | "horas" => Some(1.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// Synchronous servicing with the textbook formula `n' = (l + m)/(l + w)` (Freivalds
    /// and Niebel, ch. 2), worked by hand: loading l = 1 min, machine time m = 4 min,
    /// walking w = 0.25 min give n' = 5/1.25 = 4 machines exactly; with 3 machines the
    /// cycle is l + m = 5 min and the operator works 3 × 1.25 = 3.75 min of it (75 %);
    /// with 5 machines the cycle is 5 × 1.25 = 6.25 min and each machine works 5 of it
    /// (80 %).
    #[test]
    fn synchronous_servicing_by_hand() {
        let m = Abt {
            a: 1.0,
            b: 0.25,
            t: 4.0,
        };
        assert!(close(m.n_linha(), 4.0));
        let three = m.arranjo(3, None, 1.0 / 60.0);
        assert!(close(three.ciclo, 5.0));
        assert!(close(three.utilizacao_operador, 0.75));
        assert!(close(three.utilizacao_maquina, 1.0));
        assert!(close(three.operador_ocioso, 1.25));
        let five = m.arranjo(5, None, 1.0 / 60.0);
        assert!(close(five.ciclo, 6.25));
        assert!(close(five.utilizacao_operador, 1.0));
        assert!(close(five.utilizacao_maquina, 0.8));
        assert!(close(five.maquina_ociosa, 1.25));
        let four = m.arranjo(4, None, 1.0);
        assert!(close(four.utilizacao_operador, 1.0) && close(four.utilizacao_maquina, 1.0));
    }

    /// Cost per unit `TEC = (K1 + n·K2)·cycle/n` (Freivalds and Niebel, ch. 2), worked by
    /// hand in minutes: l = 1, w = 0.5, m = 4 give n' = 5/1.5 = 3.33. With K1 = 12 $/h
    /// and K2 = 30 $/h: n = 3, cycle 5 min, TEC = (12 + 90)·(5/60)/3 = 2.8333 $/unit;
    /// n = 4, cycle 4 × 1.5 = 6 min, TEC = (12 + 120)·(6/60)/4 = 3.30 $/unit: 3 machines.
    /// With machines cheap (K2 = 3 $/h) the operator is the expensive part: n = 3 gives
    /// (12 + 9)·(5/60)/3 = 0.5833, n = 4 gives (12 + 12)·(6/60)/4 = 0.60: still 3, by a
    /// little; with K2 = 1 $/h, n = 3 gives 0.4167 and n = 4 gives 0.40: 4 machines.
    #[test]
    fn cost_based_optimum_by_hand() {
        let m = Abt {
            a: 1.0,
            b: 0.5,
            t: 4.0,
        };
        let h = horas_por_unidade("min").unwrap();
        assert!((m.n_linha() - 10.0 / 3.0).abs() < 1e-12);
        let three = m.arranjo(3, Some((12.0, 30.0)), h);
        assert!((three.custo_por_unidade.unwrap() - 2.833_333_333).abs() < 1e-6);
        let four = m.arranjo(4, Some((12.0, 30.0)), h);
        assert!((four.custo_por_unidade.unwrap() - 3.30).abs() < 1e-9);
        assert_eq!(m.otimo((12.0, 30.0), h).unwrap().maquinas, 3);
        assert_eq!(m.otimo((12.0, 3.0), h).unwrap().maquinas, 3);
        let cheap = m.otimo((12.0, 1.0), h).unwrap();
        assert_eq!(cheap.maquinas, 4);
        assert!((cheap.custo_por_unidade.unwrap() - 0.40).abs() < 1e-9);
        let ns: Vec<usize> = m.arranjos(2, None, h).iter().map(|a| a.maquinas).collect();
        assert_eq!(ns, vec![2, 3, 4]);
    }

    /// Normal and standard time (Barnes; Freivalds and Niebel, ch. 11), by hand: observed
    /// 0.80 min at a rating of 110 % gives a normal time of 0.88 min; with 15 % PF&D
    /// allowances the standard time is 0.88 × 1.15 = 1.012 min.
    #[test]
    fn normal_and_standard_time_by_hand() {
        let n = tempo_normal(0.80, 1.10);
        assert!(close(n, 0.88));
        assert!(close(tempo_padrao(n, 0.15), 1.012));
        assert!(close(tempo_padrao(2.0, 0.0), 2.0));
    }

    #[test]
    fn units_and_degenerate_cases() {
        assert_eq!(horas_por_unidade("h"), Some(1.0));
        assert_eq!(horas_por_unidade("semana"), None);
        let no_operator = Abt {
            a: 0.0,
            b: 0.0,
            t: 3.0,
        };
        assert!(no_operator.n_linha().is_infinite());
        assert!(no_operator.otimo((1.0, 1.0), 1.0).is_none());
        // Without n' the current number and one more are shown.
        assert_eq!(no_operator.arranjos(2, None, 1.0).len(), 2);
    }
}
