//! Work cycles in a sequence of elements: where the cycle starts (an anchor element), which
//! cycles are whole and regular, and the representative one.
//!
//! The rule is the one of the stopwatch study on the shop floor: a cycle is the time
//! between two successive starts of the same element (the "anchor", e.g. the operator
//! starting to load machine 1). Cycles that start before the window (warm-up) or end after
//! it (partial cycles) are left out. A cycle whose sequence of work elements differs from
//! the most frequent sequence (a machine served out of turn, a breakdown, a pause) is
//! kept in the list but marked irregular and not used in the statistics; it is the
//! "foreign element" of the time study.

/// One stretch of an actor doing one element.
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub t0: f64,
    pub t1: f64,
    pub elemento: String,
    /// Counts in the signature of the cycle (work elements; idle and waiting do not, so a
    /// little waiting more or less does not break the pattern).
    pub trabalho: bool,
    /// The cycle is irregular if it contains this element (breakdown, pause, off shift).
    pub interrupcao: bool,
}

/// One cycle found.
#[derive(Clone, Debug, PartialEq)]
pub struct Ciclo {
    pub inicio: f64,
    pub fim: f64,
    /// Work elements in order, joined by " > ".
    pub assinatura: String,
    pub regular: bool,
}

impl Ciclo {
    pub fn duracao(&self) -> f64 {
        self.fim - self.inicio
    }
}

/// Joins consecutive stretches of the same element and drops the ones shorter than `eps`
/// (the zero-length flickers of a state change, e.g. blocked → idle → waiting at the same
/// instant).
pub fn limpar(segs: &[Seg], eps: f64) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    for s in segs.iter().filter(|s| s.t1 - s.t0 > eps) {
        match out.last_mut() {
            Some(last) if last.elemento == s.elemento && (s.t0 - last.t1).abs() <= eps => {
                last.t1 = s.t1;
            }
            _ => out.push(s.clone()),
        }
    }
    out
}

/// The cycles of `segs` (already cleaned, in time order) anchored on the starts of
/// `ancora`, inside `[w0, w1]`.
pub fn detectar(segs: &[Seg], ancora: &str, w0: f64, w1: f64) -> Vec<Ciclo> {
    let starts: Vec<f64> = segs
        .iter()
        .filter(|s| s.elemento == ancora && s.t0 >= w0 - 1e-9)
        .map(|s| s.t0)
        .collect();
    let mut ciclos: Vec<Ciclo> = starts
        .windows(2)
        .filter(|w| w[1] <= w1 + 1e-9)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            // The stretches are in time order and do not overlap.
            let i = segs.partition_point(|s| s.t1 <= a);
            let j = segs.partition_point(|s| s.t0 < b).max(i);
            let dentro = segs[i..j].iter();
            let mut assinatura: Vec<&str> = Vec::new();
            let mut interrompido = false;
            for s in dentro {
                interrompido |= s.interrupcao;
                if s.trabalho && assinatura.last() != Some(&s.elemento.as_str()) {
                    assinatura.push(&s.elemento);
                }
            }
            Ciclo {
                inicio: a,
                fim: b,
                assinatura: assinatura.join(" > "),
                regular: !interrompido,
            }
        })
        .collect();
    // The most frequent signature among the uninterrupted cycles is the method; the
    // others are irregular. Ties: the one seen first.
    let mut contagem: Vec<(&str, usize)> = Vec::new();
    for c in ciclos.iter().filter(|c| c.regular) {
        match contagem.iter_mut().find(|(s, _)| *s == c.assinatura) {
            Some(e) => e.1 += 1,
            None => contagem.push((&c.assinatura, 1)),
        }
    }
    let modal = contagem
        .iter()
        .fold(None::<(&str, usize)>, |best, &(s, n)| match best {
            Some((_, m)) if m >= n => best,
            _ => Some((s, n)),
        })
        .map(|(s, _)| s.to_string());
    if let Some(m) = modal {
        for c in &mut ciclos {
            c.regular &= c.assinatura == m;
        }
    }
    ciclos
}

/// Index of the regular cycle whose duration is the median of the regular ones (the
/// lower median for an even count). `None` without regular cycles.
pub fn mediano(ciclos: &[Ciclo]) -> Option<usize> {
    let mut reg: Vec<(f64, usize)> = ciclos
        .iter()
        .enumerate()
        .filter(|(_, c)| c.regular)
        .map(|(i, c)| (c.duracao(), i))
        .collect();
    if reg.is_empty() {
        return None;
    }
    reg.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    Some(reg[(reg.len() - 1) / 2].1)
}

/// Time an actor spends in each element inside `[a, b)`, in order of first appearance.
pub fn tempos_no_intervalo(segs: &[Seg], a: f64, b: f64) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    for s in segs {
        let d = s.t1.min(b) - s.t0.max(a);
        if d <= 0.0 {
            continue;
        }
        match out.iter_mut().find(|(e, _)| *e == s.elemento) {
            Some(e) => e.1 += d,
            None => out.push((s.elemento.clone(), d)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t0: f64, t1: f64, e: &str) -> Seg {
        Seg {
            t0,
            t1,
            elemento: e.into(),
            trabalho: !e.starts_with("ocioso"),
            interrupcao: e == "pausa",
        }
    }

    /// One operator, two machines: load m1, walk, load m2, walk back, wait. `jitter`
    /// changes the durations of cycle k so no two cycles are equal.
    fn sequencia(n: usize, t_start: f64, jitter: impl Fn(usize) -> f64) -> Vec<Seg> {
        let mut v = Vec::new();
        let mut t = t_start;
        for k in 0..n {
            let j = jitter(k);
            for (e, d) in [
                ("carga m1", 10.0 + j),
                ("andar até m2", 4.0),
                ("carga m2", 10.0 - j / 2.0),
                ("andar até m1", 4.0),
                ("ocioso", 12.0 + j / 3.0),
            ] {
                v.push(seg(t, t + d, e));
                t += d;
            }
        }
        v
    }

    #[test]
    fn noisy_cycles_are_found_and_the_median_is_representative() {
        let segs = sequencia(9, 0.0, |k| ((k * 7) % 5) as f64 * 0.4 - 0.8);
        let c = detectar(&limpar(&segs, 1e-9), "carga m1", 0.0, 1e9);
        assert_eq!(c.len(), 8, "9 starts make 8 whole cycles");
        assert!(c.iter().all(|c| c.regular));
        assert_eq!(
            c[0].assinatura,
            "carga m1 > andar até m2 > carga m2 > andar até m1"
        );
        for c in &c {
            assert!((c.duracao() - 40.0).abs() < 1.0, "{}", c.duracao());
        }
        let m = mediano(&c).unwrap();
        let mut d: Vec<f64> = c.iter().map(Ciclo::duracao).collect();
        d.sort_by(f64::total_cmp);
        assert_eq!(c[m].duracao(), d[3]);
    }

    #[test]
    fn warm_up_and_the_partial_cycle_at_the_end_are_left_out() {
        let segs = sequencia(6, 0.0, |_| 0.0);
        // Cycles start at 0, 40, 80, ... The window [50, 215] keeps only 80→120,
        // 120→160 and 160→200; the one from 200 ends after the window.
        let c = detectar(&segs, "carga m1", 50.0, 215.0);
        let starts: Vec<f64> = c.iter().map(|c| c.inicio).collect();
        assert_eq!(starts, vec![80.0, 120.0, 160.0]);
    }

    #[test]
    fn a_cycle_out_of_order_or_interrupted_is_irregular() {
        let mut segs = sequencia(6, 0.0, |_| 0.0);
        // Cycle 2 (80 to 120): the operator takes another way to m2 (a foreign element).
        segs[11] = seg(segs[11].t0, segs[11].t1, "andar até m1 (desvio)");
        // Cycle 4 (160 to 200): a pause instead of the idle time.
        let p = segs.iter().position(|s| s.t0 == 188.0).unwrap();
        segs[p] = seg(segs[p].t0, segs[p].t1, "pausa");
        let c = detectar(&segs, "carga m1", 0.0, 1e9);
        let reg: Vec<bool> = c.iter().map(|c| c.regular).collect();
        assert_eq!(reg, vec![true, true, false, true, false]);
    }

    #[test]
    fn flickers_and_repeated_stretches_are_merged() {
        let segs = vec![
            seg(0.0, 5.0, "carga m1"),
            seg(5.0, 5.0, "ocioso"),
            seg(5.0, 8.0, "carga m1"),
            seg(8.0, 9.0, "ocioso"),
        ];
        let l = limpar(&segs, 1e-9);
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].t0, l[0].t1), (0.0, 8.0));
        let t = tempos_no_intervalo(&l, 2.0, 8.5);
        assert_eq!(
            t,
            vec![("carga m1".to_string(), 6.0), ("ocioso".to_string(), 0.5)]
        );
    }

    #[test]
    fn no_regular_cycle_without_two_anchors() {
        let segs = vec![seg(0.0, 5.0, "carga m1"), seg(5.0, 9.0, "ocioso")];
        assert!(detectar(&segs, "carga m1", 0.0, 100.0).is_empty());
        assert_eq!(mediano(&[]), None);
    }
}
