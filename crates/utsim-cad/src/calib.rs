//! Calibração por dois pontos e deslocamento da origem.

use serde::{Deserialize, Serialize};

use crate::CadError;
use crate::geom::{Pt, dist};

/// Transformação de similaridade: `p' = escala * R(rotação) * p + deslocamento`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub scale: f64,
    pub rotation_deg: f64,
    pub offset: Pt,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        scale: 1.0,
        rotation_deg: 0.0,
        offset: [0.0, 0.0],
    };

    pub fn apply(&self, p: Pt) -> Pt {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        [
            self.scale * (c * p[0] - s * p[1]) + self.offset[0],
            self.scale * (s * p[0] + c * p[1]) + self.offset[1],
        ]
    }

    /// Transformação que apenas desloca para que `origin` vire (0, 0).
    pub fn origin_at(origin: Pt) -> Transform {
        Transform {
            scale: 1.0,
            rotation_deg: 0.0,
            offset: [-origin[0], -origin[1]],
        }
    }

    /// Composição: aplica `self` e depois `then`.
    pub fn then(&self, then: &Transform) -> Transform {
        let rot = self.rotation_deg + then.rotation_deg;
        let moved = then.apply(self.offset);
        // then(self(p)) = s2 R2 (s1 R1 p + o1) + o2
        Transform {
            scale: self.scale * then.scale,
            rotation_deg: rot,
            offset: moved,
        }
    }
}

/// Calibração: dois pontos do desenho e a distância real entre eles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    pub p1: Pt,
    pub p2: Pt,
    /// Distância real, em metros.
    pub distance: f64,
}

impl Calibration {
    pub fn new(p1: Pt, p2: Pt, distance: f64) -> Result<Self, CadError> {
        if !(distance.is_finite() && distance > 0.0) {
            return Err(CadError::Param("a distância real deve ser positiva".into()));
        }
        if dist(p1, p2) <= 0.0 {
            return Err(CadError::Param("os dois pontos são iguais".into()));
        }
        Ok(Calibration { p1, p2, distance })
    }

    /// Escala (metros por unidade do desenho): `distância / |p2 - p1|`.
    pub fn scale(&self) -> f64 {
        self.distance / dist(self.p1, self.p2)
    }

    /// Só escala (sem rotação nem deslocamento).
    pub fn transform(&self) -> Transform {
        Transform {
            scale: self.scale(),
            rotation_deg: 0.0,
            offset: [0.0, 0.0],
        }
    }

    /// Escala mais o deslocamento que leva `p1` (já em metros) para `target1`.
    pub fn transform_anchored(&self, target1: Pt) -> Transform {
        let t = self.transform();
        let q = t.apply(self.p1);
        Transform {
            offset: [target1[0] - q[0], target1[1] - q[1]],
            ..t
        }
    }

    /// Ajuste de similaridade com dois pares de pontos: `p1 -> q1` e `p2 -> q2` (os `q` são
    /// os pontos reais no piso, em metros). Dá escala, rotação e translação.
    pub fn similarity(p1: Pt, p2: Pt, q1: Pt, q2: Pt) -> Result<Transform, CadError> {
        let dp = [p2[0] - p1[0], p2[1] - p1[1]];
        let dq = [q2[0] - q1[0], q2[1] - q1[1]];
        let lp = dist(p1, p2);
        let lq = dist(q1, q2);
        if lp <= 0.0 || lq <= 0.0 {
            return Err(CadError::Param("os dois pontos são iguais".into()));
        }
        let rot = dq[1].atan2(dq[0]) - dp[1].atan2(dp[0]);
        let t = Transform {
            scale: lq / lp,
            rotation_deg: rot.to_degrees(),
            offset: [0.0, 0.0],
        };
        let m = t.apply(p1);
        Ok(Transform {
            offset: [q1[0] - m[0], q1[1] - m[1]],
            ..t
        })
    }
}
