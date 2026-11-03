//! Critical path method: forward and backward passes, early and late dates, total and
//! free float, and every critical path.

use serde::Serialize;

use crate::network::ActivityNetwork;

/// Relative tolerance of "zero float" (times the project duration, at least 1).
pub const TOLERANCE: f64 = 1e-9;

/// Most critical paths listed (ties can multiply them).
pub const MAX_PATHS: usize = 1000;

/// Dates and floats of every activity for one set of durations.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Schedule {
    pub durations: Vec<f64>,
    /// Early start and finish.
    pub es: Vec<f64>,
    pub ef: Vec<f64>,
    /// Late start and finish.
    pub ls: Vec<f64>,
    pub lf: Vec<f64>,
    /// `LS - ES`.
    pub total_float: Vec<f64>,
    /// `min(ES of the successors) - EF`.
    pub free_float: Vec<f64>,
    /// Project duration (early finish of the end milestone).
    pub project: f64,
    pub critical: Vec<bool>,
    /// Every critical path, from the start to the end milestone (milestones included).
    pub critical_paths: Vec<Vec<usize>>,
    /// More than [`MAX_PATHS`] critical paths: only the first ones are listed.
    pub paths_truncated: bool,
}

/// A network prepared for repeated passes (Monte Carlo): order and adjacency once.
#[derive(Clone, Debug)]
pub struct Cpm {
    order: Vec<usize>,
    preds: Vec<Vec<usize>>,
    succs: Vec<Vec<usize>>,
    start: usize,
    end: usize,
}

impl Cpm {
    pub fn new(net: &ActivityNetwork) -> Self {
        let (preds, succs) = net.adjacency();
        Cpm {
            order: net.order(),
            preds,
            succs,
            start: net.start,
            end: net.end,
        }
    }

    pub fn len(&self) -> usize {
        self.preds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.preds.is_empty()
    }

    /// Forward pass: fills `es` and `ef`, returns the project duration.
    pub fn forward(&self, d: &[f64], es: &mut [f64], ef: &mut [f64]) -> f64 {
        for &v in &self.order {
            let s = self.preds[v].iter().map(|&p| ef[p]).fold(0.0, f64::max);
            es[v] = s;
            ef[v] = s + d[v];
        }
        ef[self.end]
    }

    /// Backward pass from the project duration: fills `ls` and `lf`.
    pub fn backward(&self, d: &[f64], project: f64, ls: &mut [f64], lf: &mut [f64]) {
        for &v in self.order.iter().rev() {
            let f = self.succs[v].iter().map(|&s| ls[s]).fold(project, f64::min);
            lf[v] = f;
            ls[v] = f - d[v];
        }
    }

    /// Tolerance for a project of this duration.
    pub fn tolerance(project: f64) -> f64 {
        TOLERANCE * project.abs().max(1.0)
    }

    /// Critical flags from the dates (zero total float within the tolerance).
    pub fn critical_flags(&self, es: &[f64], ls: &[f64], project: f64, out: &mut [bool]) {
        let tol = Cpm::tolerance(project);
        for v in 0..self.len() {
            out[v] = (ls[v] - es[v]).abs() <= tol;
        }
    }

    /// Paths from the start to the end milestone through critical activities linked by
    /// tight precedences (`EF` of one = `ES` of the next), at most `max`.
    pub fn critical_paths(
        &self,
        critical: &[bool],
        es: &[f64],
        ef: &[f64],
        project: f64,
        max: usize,
    ) -> (Vec<Vec<usize>>, bool) {
        let tol = Cpm::tolerance(project);
        let mut paths = Vec::new();
        let mut truncated = false;
        if !critical[self.start] {
            return (paths, false);
        }
        let mut stack: Vec<(usize, usize)> = vec![(self.start, 0)];
        let mut path = vec![self.start];
        while let Some(top) = stack.last_mut() {
            let v = top.0;
            if v == self.end {
                if paths.len() >= max {
                    truncated = true;
                    break;
                }
                paths.push(path.clone());
                stack.pop();
                path.pop();
                continue;
            }
            if top.1 < self.succs[v].len() {
                let w = self.succs[v][top.1];
                top.1 += 1;
                if critical[w] && (ef[v] - es[w]).abs() <= tol {
                    stack.push((w, 0));
                    path.push(w);
                }
            } else {
                stack.pop();
                path.pop();
            }
        }
        (paths, truncated)
    }

    /// Full schedule for these durations.
    pub fn schedule(&self, durations: &[f64]) -> Schedule {
        let n = self.len();
        let (mut es, mut ef, mut ls, mut lf) =
            (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
        let project = self.forward(durations, &mut es, &mut ef);
        self.backward(durations, project, &mut ls, &mut lf);
        let tol = Cpm::tolerance(project);
        let clean = |x: f64| if x.abs() <= tol { 0.0 } else { x };
        let total_float: Vec<f64> = (0..n).map(|v| clean(ls[v] - es[v])).collect();
        let free_float: Vec<f64> = (0..n)
            .map(|v| {
                let next = self.succs[v].iter().map(|&s| es[s]).fold(project, f64::min);
                clean(next - ef[v])
            })
            .collect();
        let mut critical = vec![false; n];
        self.critical_flags(&es, &ls, project, &mut critical);
        let (critical_paths, paths_truncated) =
            self.critical_paths(&critical, &es, &ef, project, MAX_PATHS);
        Schedule {
            durations: durations.to_vec(),
            es,
            ef,
            ls,
            lf,
            total_float,
            free_float,
            project,
            critical,
            critical_paths,
            paths_truncated,
        }
    }
}

/// CPM of the network with these durations (one per activity).
pub fn schedule(net: &ActivityNetwork, durations: &[f64]) -> Schedule {
    assert_eq!(durations.len(), net.len(), "uma duração por atividade");
    Cpm::new(net).schedule(durations)
}

/// CPM with the mean duration of every activity.
pub fn schedule_means(net: &ActivityNetwork) -> Schedule {
    schedule(net, &net.mean_durations())
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::Dist;

    fn c(x: f64) -> Dist {
        Dist::Const(x)
    }

    #[test]
    fn series_network() {
        let net =
            ActivityNetwork::from_table(&[("a", c(2.0), &[]), ("b", c(3.0), &["a"])]).unwrap();
        let s = schedule_means(&net);
        assert_eq!(s.project, 5.0);
        assert_eq!(s.critical_paths, vec![vec![net.start, 0, 1, net.end]]);
        assert_eq!(s.total_float, vec![0.0; 4]);
    }
}
