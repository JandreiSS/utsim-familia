//! Geometria 2D mínima.

use serde::{Deserialize, Serialize};

/// Ponto 2D `[x, y]`.
pub type Pt = [f64; 2];

/// Caixa envolvente.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bbox {
    pub min: Pt,
    pub max: Pt,
}

impl Bbox {
    /// Caixa que contém só o ponto.
    pub fn from_point(p: Pt) -> Self {
        Bbox { min: p, max: p }
    }

    /// Amplia a caixa para conter o ponto.
    pub fn include(&mut self, p: Pt) {
        self.min[0] = self.min[0].min(p[0]);
        self.min[1] = self.min[1].min(p[1]);
        self.max[0] = self.max[0].max(p[0]);
        self.max[1] = self.max[1].max(p[1]);
    }

    /// União de duas caixas.
    pub fn union(&self, o: &Bbox) -> Bbox {
        let mut b = *self;
        b.include(o.min);
        b.include(o.max);
        b
    }

    pub fn width(&self) -> f64 {
        self.max[0] - self.min[0]
    }

    pub fn height(&self) -> f64 {
        self.max[1] - self.min[1]
    }
}

/// Distância euclidiana.
pub fn dist(a: Pt, b: Pt) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Área com sinal de um anel (positiva no sentido anti-horário; o anel não repete o 1º ponto).
pub fn signed_area(ring: &[Pt]) -> f64 {
    let n = ring.len();
    let mut s = 0.0;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        s += a[0] * b[1] - b[0] * a[1];
    }
    s / 2.0
}
