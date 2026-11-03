//! Modelo interno da planta: camadas com polilinhas e rótulos, em metros (ou na unidade
//! do desenho, quando lida sem conversão).

use serde::{Deserialize, Serialize};

use crate::geom::{Bbox, Pt};
use crate::simplify::simplify_polyline;

/// Polilinha (uma linha é uma polilinha de 2 pontos). `closed` liga o último ao primeiro.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline {
    pub points: Vec<Pt>,
    pub closed: bool,
}

impl Polyline {
    /// Segmentos da polilinha (inclui o de fechamento quando `closed`).
    pub fn segments(&self) -> Vec<[Pt; 2]> {
        let n = self.points.len();
        let mut v = Vec::with_capacity(n);
        for i in 0..n.saturating_sub(1) {
            v.push([self.points[i], self.points[i + 1]]);
        }
        if self.closed && n > 2 {
            v.push([self.points[n - 1], self.points[0]]);
        }
        v
    }

    /// Comprimento total.
    pub fn length(&self) -> f64 {
        self.segments()
            .iter()
            .map(|s| crate::geom::dist(s[0], s[1]))
            .sum()
    }
}

/// Texto (TEXT/MTEXT) usado como rótulo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    pub pos: Pt,
    pub height: f64,
    pub rotation_deg: f64,
}

/// Uma camada do desenho.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub polylines: Vec<Polyline>,
    pub labels: Vec<Label>,
}

impl Layer {
    pub(crate) fn new(name: &str) -> Self {
        Layer {
            name: name.to_string(),
            polylines: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// Caixa envolvente das polilinhas e dos rótulos (None se a camada está vazia).
    pub fn bbox(&self) -> Option<Bbox> {
        let mut b: Option<Bbox> = None;
        let pts = self
            .polylines
            .iter()
            .flat_map(|p| p.points.iter().copied())
            .chain(self.labels.iter().map(|l| l.pos));
        for p in pts {
            match &mut b {
                Some(bb) => bb.include(p),
                None => b = Some(Bbox::from_point(p)),
            }
        }
        b
    }

    /// Número de segmentos.
    pub fn segment_count(&self) -> usize {
        self.polylines.iter().map(|p| p.segments().len()).sum()
    }
}

/// Resumo de uma camada (para listas e diálogos).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerSummary {
    pub name: String,
    /// Entidades achatadas (polilinhas mais rótulos).
    pub entities: usize,
    pub polylines: usize,
    pub segments: usize,
    pub labels: usize,
    pub bbox: Option<Bbox>,
}

/// A planta lida: camadas ordenadas por nome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// Camadas, em ordem alfabética do nome (determinístico).
    pub layers: Vec<Layer>,
    /// Fator aplicado às coordenadas do arquivo (metros por unidade do desenho); 1 se lida
    /// sem conversão.
    pub meters_per_unit: f64,
    /// `true` se o cabeçalho não trazia unidade e a padrão foi assumida.
    pub units_assumed: bool,
}

impl Plan {
    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// Caixa envolvente da planta inteira.
    pub fn bbox(&self) -> Option<Bbox> {
        self.layers
            .iter()
            .filter_map(|l| l.bbox())
            .reduce(|a, b| a.union(&b))
    }

    /// Contagem por camada.
    pub fn summaries(&self) -> Vec<LayerSummary> {
        self.layers
            .iter()
            .map(|l| LayerSummary {
                name: l.name.clone(),
                entities: l.polylines.len() + l.labels.len(),
                polylines: l.polylines.len(),
                segments: l.segment_count(),
                labels: l.labels.len(),
                bbox: l.bbox(),
            })
            .collect()
    }

    /// Cópia com todos os pontos transformados (ver [`crate::Transform`]).
    pub fn transformed(&self, t: &crate::Transform) -> Plan {
        let mut p = self.clone();
        for l in &mut p.layers {
            for pl in &mut l.polylines {
                for pt in &mut pl.points {
                    *pt = t.apply(*pt);
                }
            }
            for lb in &mut l.labels {
                lb.pos = t.apply(lb.pos);
                lb.height *= t.scale;
                lb.rotation_deg += t.rotation_deg;
            }
        }
        p.meters_per_unit *= t.scale;
        p
    }

    /// Cópia simplificada (Douglas-Peucker) com a tolerância dada, na unidade da planta.
    /// Polilinhas fechadas continuam com pelo menos 3 pontos.
    pub fn simplified(&self, tolerance: f64) -> Plan {
        let mut p = self.clone();
        for l in &mut p.layers {
            for pl in &mut l.polylines {
                pl.points = simplify_polyline(&pl.points, pl.closed, tolerance);
            }
        }
        p
    }

    /// Número total de segmentos (útil para medir o efeito da simplificação).
    pub fn segment_count(&self) -> usize {
        self.layers.iter().map(|l| l.segment_count()).sum()
    }
}
