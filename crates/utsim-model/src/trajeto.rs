//! Esteira desenhada por nós (`route` da esteira): a geometria entra no resultado.
//!
//! - Comprimento da esteira = comprimento 3D do trajeto (com as curvas e as subidas). Ele
//!   substitui `length`: tempo de passagem = comprimento ÷ velocidade e capacidade =
//!   comprimento ÷ `item_length`.
//! - Inclinação: acima do limite do tipo ([`BeltStyle::max_slope`] ou `max_slope` dado) há
//!   aviso; a peça escorrega ou tomba.
//! - Esteira por gravidade (roletes livres): a velocidade de cada trecho sai da inclinação
//!   ([`gravity_speed`]), e a velocidade da esteira é a que dá o mesmo tempo de passagem
//!   do trajeto inteiro: `L ÷ Σ (Lᵢ ÷ vᵢ)`.
//!
//! As pontas de entrada e de saída são o primeiro e o último nó; o corpo no piso é a faixa
//! da largura da esteira em volta do trajeto.

use serde::{Deserialize, Serialize};
use utsim_nav::path3d::{self, P3, PathNode};

use crate::{Model, ObjectDef, ObjectKind};

pub use utsim_nav::path3d::{legs, max_grade, path_length, problem, sample};

/// Tipo construtivo da esteira desenhada.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeltStyle {
    /// Esteira simples (correia lisa ou roletes, sem guias).
    #[default]
    Simple,
    /// Esteira com canaletas (guias laterais altas que seguram a peça).
    Channeled,
}

impl BeltStyle {
    /// Inclinação máxima padrão (graus). Correia lisa: a peça escorrega acima de ~15°;
    /// com as canaletas laterais (e taliscas) ela sobe até ~25°.
    pub fn max_slope(self) -> f64 {
        match self {
            BeltStyle::Simple => 15.0,
            BeltStyle::Channeled => 25.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BeltStyle::Simple => "esteira simples",
            BeltStyle::Channeled => "esteira com canaletas",
        }
    }
}

/// O que move a peça.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeltDrive {
    /// Motorizada: a peça anda na velocidade da esteira, suba ou desça.
    #[default]
    Powered,
    /// Roletes livres por gravidade: a velocidade sai da inclinação ([`gravity_speed`]).
    Gravity,
}

/// Inclinação (fração, 0,015 = 1,5 %) abaixo da qual a peça não anda sozinha nos roletes
/// livres: a resistência ao rolamento.
pub const ROLLING: f64 = 0.015;
/// Inclinação a partir da qual a peça já chega à velocidade-limite (os roletes de freio
/// seguram nela).
pub const FULL_GRADE: f64 = 0.05;
/// Velocidade mínima num trecho plano ou em subida por gravidade, como fração da
/// velocidade-limite: a peça só anda empurrada pelas de trás.
pub const PUSHED: f64 = 0.1;

/// Velocidade num trecho por gravidade com essa inclinação (descendo é positiva), com
/// `limit` a velocidade-limite (o `speed` da esteira, segurada pelos roletes de freio):
/// `limit × clamp((g − ROLLING) ÷ (FULL_GRADE − ROLLING), PUSHED, 1)`.
pub fn gravity_speed(grade_down: f64, limit: f64) -> f64 {
    let f = (grade_down - ROLLING) / (FULL_GRADE - ROLLING);
    limit * f.clamp(PUSHED, 1.0)
}

/// Trajeto desenhado de uma esteira.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeltRoute {
    /// Nós, relativos à posição da esteira (o primeiro costuma ser `[0, 0, 0]`).
    pub nodes: Vec<PathNode>,
    #[serde(default)]
    pub style: BeltStyle,
    #[serde(default)]
    pub drive: BeltDrive,
    /// Inclinação máxima (graus); ausente: a do tipo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_slope: Option<f64>,
    /// Largura (m); ausente: [`DEFAULT_WIDTH`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
}

/// Largura padrão de uma esteira desenhada (m).
pub const DEFAULT_WIDTH: f64 = 0.6;

impl BeltRoute {
    pub fn width(&self) -> f64 {
        self.width
            .filter(|w| w.is_finite() && *w > 0.0)
            .unwrap_or(DEFAULT_WIDTH)
    }

    pub fn max_slope(&self) -> f64 {
        self.max_slope.unwrap_or(self.style.max_slope())
    }

    /// A linha real em coordenadas absolutas (a esteira está em `origin`).
    pub fn line(&self, origin: [f64; 3]) -> Vec<P3> {
        path3d::sample(&self.nodes)
            .into_iter()
            .map(|p| [p[0] + origin[0], p[1] + origin[1], p[2] + origin[2]])
            .collect()
    }

    pub fn length(&self) -> f64 {
        path3d::path_length(&self.nodes)
    }

    /// Velocidade equivalente (mesmo tempo de passagem) com `speed` a da esteira.
    pub fn speed(&self, speed: f64) -> f64 {
        match self.drive {
            BeltDrive::Powered => speed,
            BeltDrive::Gravity => {
                let pts = path3d::sample(&self.nodes);
                let (mut len, mut time) = (0.0, 0.0);
                for w in pts.windows(2) {
                    let d = path3d::dist3(w[0], w[1]);
                    if d <= 0.0 {
                        continue;
                    }
                    let down = -path3d::grade(w[0], w[1]);
                    let v = gravity_speed(if down.is_finite() { down } else { 1.0 }, speed);
                    len += d;
                    time += d / v;
                }
                if time > 0.0 { len / time } else { speed }
            }
        }
    }

    /// Pontas de entrada e de saída (primeiro e último nó), absolutas.
    pub fn ends(&self, origin: [f64; 3]) -> Option<(P3, P3)> {
        let line = self.line(origin);
        Some((*line.first()?, *line.last()?))
    }
}

/// Comprimento, velocidade e item da esteira que a simulação usa: com trajeto, o
/// comprimento é o do trajeto e a velocidade a equivalente; sem, os campos.
pub fn belt_params(obj: &ObjectDef) -> Option<(f64, f64)> {
    belt_params_of(&obj.kind)
}

/// O mesmo que [`belt_params`], só pelo tipo.
pub fn belt_params_of(kind: &ObjectKind) -> Option<(f64, f64)> {
    let ObjectKind::Conveyor { length, speed, .. } = kind else {
        return None;
    };
    match valid_route(kind) {
        Some(r) => Some((r.length(), r.speed(*speed))),
        None => Some((*length, *speed)),
    }
}

/// O trajeto da esteira, se ela tem um que serve.
pub fn valid_route(kind: &ObjectKind) -> Option<&BeltRoute> {
    match kind {
        ObjectKind::Conveyor { route: Some(r), .. } if path3d::problem(&r.nodes, "").is_none() => {
            Some(r)
        }
        _ => None,
    }
}

/// Altura livre (m) sob a esteira para se passar por baixo dela: a parte de baixo da
/// estrutura ([`FRAME_DEPTH`] abaixo da superfície) a pelo menos esta altura do piso.
pub const HEADROOM: f64 = 2.2;
/// Profundidade da estrutura sob a superfície da esteira (m), a mesma do desenho 3D.
pub const FRAME_DEPTH: f64 = 0.14;
/// Lado de uma perna no piso (m), com folga para a sapata.
pub const LEG: f64 = 0.12;

/// Obstáculos no piso de uma esteira desenhada que passa no alto: os trechos baixos
/// (a faixa da largura) e as pernas sob cada nó alto. `None` quando nenhum trecho dá
/// passagem por baixo (a faixa inteira continua sendo o corpo dela).
pub fn elevated_obstacles(
    name: &str,
    origin: [f64; 3],
    r: &BeltRoute,
) -> Option<Vec<utsim_nav::Obstacle>> {
    let line = r.line(origin);
    let high = |p: P3| p[2] - FRAME_DEPTH >= HEADROOM;
    if !line.iter().any(|p| high(*p)) {
        return None;
    }
    let w = r.width() + 0.09;
    let mut out = Vec::new();
    let mut k = 0;
    for seg in line.windows(2) {
        if high(seg[0]) && high(seg[1]) {
            continue;
        }
        let plan = path3d::plan_line(seg);
        if path3d::dist3([plan[0][0], plan[0][1], 0.0], [plan[1][0], plan[1][1], 0.0]) <= 1e-9 {
            continue;
        }
        k += 1;
        out.push(utsim_nav::Obstacle {
            name: format!("{name}_trecho_{k}"),
            polygon: path3d::band(&plan, w),
        });
    }
    let nodes = r.line(origin);
    for (i, n) in r.nodes.iter().enumerate() {
        let node = [
            n.at[0] + origin[0],
            n.at[1] + origin[1],
            n.at[2] + origin[2],
        ];
        // The two legs, beside the line under this node: at a rounded corner the node is
        // off the arc, so they stand under the point of the arc nearest to it (as drawn).
        let j = nodes
            .iter()
            .enumerate()
            .min_by(|a, b| path3d::dist3(*a.1, node).total_cmp(&path3d::dist3(*b.1, node)))
            .map_or(0, |(j, _)| j);
        let at = nodes[j];
        if !high(at) {
            continue;
        }
        let a = nodes[j.min(nodes.len() - 2)];
        let b = nodes[j.min(nodes.len() - 2) + 1];
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
        let side = [-d[1] / len, d[0] / len];
        for s in [-1.0, 1.0] {
            let off = s * (r.width() / 2.0 + 0.0225);
            let c = [at[0] + side[0] * off, at[1] + side[1] * off];
            out.push(utsim_nav::Obstacle {
                name: format!("{name}_perna_{}_{}", i + 1, if s < 0.0 { "d" } else { "e" }),
                polygon: utsim_nav::geom::rectangle(c, [LEG / 2.0, LEG / 2.0], 0.0),
            });
        }
    }
    Some(out)
}

/// Aceleração da gravidade (m/s²).
pub const GRAVITY: f64 = 9.81;
/// Queda (m) abaixo da qual a passagem entre esteiras é imediata.
pub const DROP_MIN: f64 = 0.05;
/// Queda (m) acima da qual há aviso: a peça bate ao cair (use calha ou desça a esteira).
pub const DROP_WARN: f64 = 0.5;
/// Vão horizontal (m) acima do qual a peça cai fora entre as duas pontas.
pub const GAP_WARN: f64 = 0.3;

/// Faixa de alcance da mão de uma pessoa em pé (m do piso) para pegar ou deixar uma peça
/// na ponta de uma esteira: abaixo, ela se agacha; acima, não alcança sem escada ou
/// plataforma.
pub const REACH_LOW: f64 = 0.3;
pub const REACH_HIGH: f64 = 1.75;

/// Tempo de queda livre de uma altura `h` (m): `√(2h/g)`.
pub fn fall_time(h: f64) -> f64 {
    (2.0 * h.max(0.0) / GRAVITY).sqrt()
}

/// Uma passagem de esteira desenhada para outra: `(destino, queda em m (positiva desce),
/// vão horizontal em m)`.
pub(crate) type Handoff = (String, f64, f64);

impl Model {
    /// As passagens da esteira desenhada `obj` para as esteiras desenhadas que ela
    /// alimenta (ponta de saída dela → ponta de entrada da outra).
    pub(crate) fn handoffs(&self, obj: &ObjectDef) -> Vec<Handoff> {
        let Some(r) = valid_route(&obj.kind) else {
            return Vec::new();
        };
        let Some((_, out)) = r.ends(obj.position) else {
            return Vec::new();
        };
        self.connections
            .iter()
            .filter(|(f, _)| *f == obj.name)
            .filter_map(|(_, to)| {
                let o = self.objects.iter().find(|o| o.name == *to)?;
                let (inp, _) = valid_route(&o.kind)?.ends(o.position)?;
                let gap = ((out[0] - inp[0]).powi(2) + (out[1] - inp[1]).powi(2)).sqrt();
                Some((to.clone(), out[2] - inp[2], gap))
            })
            .collect()
    }

    /// A passagem com queda da esteira desenhada `obj` para a primeira esteira desenhada
    /// mais baixa que ela alimenta: o tempo de queda livre, como atraso da ligação.
    pub(crate) fn belt_drop(&self, obj: &ObjectDef) -> Option<utsim_core::TransferDef> {
        if obj.transfer.is_some() {
            return None;
        }
        let (_, h, _) = self
            .handoffs(obj)
            .into_iter()
            .find(|(_, h, _)| *h >= DROP_MIN)?;
        Some(utsim_core::TransferDef {
            speed: f64::INFINITY,
            delay: fall_time(h),
            path: Default::default(),
        })
    }

    pub(crate) fn validate_routes(&self, errors: &mut Vec<String>) {
        for o in &self.objects {
            let ObjectKind::Conveyor {
                route: Some(r),
                item_length,
                ..
            } = &o.kind
            else {
                continue;
            };
            if let Some(p) = path3d::problem(&r.nodes, &o.name) {
                errors.push(p);
                continue;
            }
            if r.width.is_some_and(|w| !(w.is_finite() && w > 0.0)) {
                errors.push(format!("{}: a largura da esteira deve ser > 0 (m)", o.name));
            }
            if r.max_slope
                .is_some_and(|s| !(s.is_finite() && s > 0.0 && s < 90.0))
            {
                errors.push(format!(
                    "{}: max_slope deve ficar entre 0 e 90 graus",
                    o.name
                ));
            }
            if *item_length > r.length() {
                errors.push(format!(
                    "{}: item_length maior que o trajeto desenhado ({:.2} m): nenhum item cabe",
                    o.name,
                    r.length()
                ));
            }
        }
    }

    pub(crate) fn route_warnings(&self, out: &mut Vec<String>) {
        for o in &self.objects {
            let ObjectKind::Conveyor { route: Some(r), .. } = &o.kind else {
                continue;
            };
            if path3d::problem(&r.nodes, &o.name).is_some() {
                continue;
            }
            let line = path3d::sample(&r.nodes);
            let worst = path3d::degrees(path3d::max_grade(&line));
            if worst > r.max_slope() + 1e-9 {
                out.push(format!(
                    "{}: trecho com {:.1}° de inclinação, acima do máximo de {:.0}° da {}; a \
                     peça escorrega ou tomba",
                    o.name,
                    worst,
                    r.max_slope(),
                    r.style.label()
                ));
            }
            if r.drive == BeltDrive::Gravity {
                let slow = line.windows(2).any(|w| {
                    path3d::dist3(w[0], w[1]) > 1e-9 && -path3d::grade(w[0], w[1]) < ROLLING
                });
                if slow {
                    out.push(format!(
                        "{}: esteira por gravidade com trecho plano ou em subida (menos de \
                         {:.1} % de descida); ali a peça só anda empurrada, a {:.0} % da \
                         velocidade",
                        o.name,
                        ROLLING * 100.0,
                        PUSHED * 100.0
                    ));
                }
            }
            for (to, h, gap) in self.handoffs(o) {
                if h > DROP_WARN {
                    out.push(format!(
                        "{}: a peça cai {:.2} m até '{to}'; acima de {:.1} m ela bate (use calha ou desça a esteira)",
                        o.name, h, DROP_WARN
                    ));
                } else if h < -0.02 {
                    out.push(format!(
                        "{}: a ponta de entrada de '{to}' está {:.2} m acima da saída; a peça não sobe sozinha",
                        o.name, -h
                    ));
                }
                if gap > GAP_WARN {
                    out.push(format!(
                        "{}: as pontas com '{to}' estão a {:.2} m uma da outra; a peça cai fora",
                        o.name, gap
                    ));
                }
            }
            // Who picks at the output end (the belt's own `transport`) and who drops at the
            // input end (the `transport` of what feeds it): a person must reach the end.
            if let Some((inp, outp)) = r.ends(o.position) {
                let person = |t: &str| {
                    self.objects
                        .iter()
                        .find(|x| x.name == t)
                        .is_some_and(|x| self.role_of(x) == utsim_nav::Role::Person)
                };
                let mut ends: Vec<(&str, f64, String)> = Vec::new();
                if let Some(t) = o.transport.as_deref().filter(|t| person(t)) {
                    ends.push(("saída", outp[2], t.to_string()));
                }
                for (from, _) in self.connections.iter().filter(|(_, to)| *to == o.name) {
                    let feeder = self.objects.iter().find(|x| x.name == *from);
                    if let Some(t) = feeder
                        .and_then(|x| x.transport.as_deref())
                        .filter(|t| person(t))
                    {
                        ends.push(("entrada", inp[2], t.to_string()));
                    }
                }
                for (end, z, who) in ends {
                    if z > REACH_HIGH {
                        out.push(format!(
                            "{}: a ponta de {end} está a {:.2} m do piso; '{who}' não alcança acima de {:.2} m (ponha uma plataforma ou desça a ponta)",
                            o.name, z, REACH_HIGH
                        ));
                    } else if z < REACH_LOW {
                        out.push(format!(
                            "{}: a ponta de {end} está a {:.2} m do piso; '{who}' trabalha agachado abaixo de {:.2} m",
                            o.name, z, REACH_LOW
                        ));
                    }
                }
            }
            if line.iter().any(|p| p[2] + o.position[2] < -1e-9) {
                out.push(format!("{}: nó abaixo do piso", o.name));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(nodes: Vec<PathNode>, drive: BeltDrive) -> BeltRoute {
        BeltRoute {
            nodes,
            style: BeltStyle::Simple,
            drive,
            max_slope: None,
            width: None,
        }
    }

    #[test]
    fn gravity_speed_follows_the_slope() {
        assert_eq!(gravity_speed(0.0, 1.0), PUSHED);
        assert_eq!(gravity_speed(0.10, 1.0), 1.0);
        let mid = gravity_speed((ROLLING + FULL_GRADE) / 2.0, 1.0);
        assert!((mid - 0.5).abs() < 1e-12);
    }

    #[test]
    fn a_powered_belt_keeps_its_speed_and_a_gravity_one_slows_when_flatter() {
        let steep = route(
            vec![
                PathNode::new([0.0, 0.0, 1.0]),
                PathNode::new([10.0, 0.0, 0.0]),
            ],
            BeltDrive::Gravity,
        );
        let gentle = route(
            vec![
                PathNode::new([0.0, 0.0, 0.3]),
                PathNode::new([10.0, 0.0, 0.0]),
            ],
            BeltDrive::Gravity,
        );
        assert!((steep.speed(0.5) - 0.5).abs() < 1e-12);
        assert!(gentle.speed(0.5) < 0.5);
        let mut powered = gentle.clone();
        powered.drive = BeltDrive::Powered;
        assert_eq!(powered.speed(0.5), 0.5);
    }
}
