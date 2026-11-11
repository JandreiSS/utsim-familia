//! Corredores de circulação no modelo (plano, seção 6): rede gerada pelos corredores para o
//! modo `Aisles`, cruzamentos com prioridade ao pedestre, validação, avisos, resumo, faixas
//! no piso para o desenho e a planta em SVG.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;

use utsim_core::Router;
use utsim_nav::aisles::{EdgeUse, offset_polyline};
use utsim_nav::{
    AisleKind, AisleNet, AislesRouter, FloorMarking, MarkShape, Pattern, Pt, User, half_width,
};
use utsim_objects::traffic::CrossingSpec;

use crate::movement::{fmt_m, role_name};
use crate::{Model, Movement, NavSet, ObjectDef, ObjectKind};

impl Model {
    /// Classe do recurso nos corredores: pessoa pelo papel `Person`, senão veículo.
    pub fn aisle_user(&self, obj: &ObjectDef) -> User {
        User::of(self.role_of(obj))
    }

    /// `L` do recurso (m): a maior largura entre o veículo e a carga. Pela largura do papel
    /// (empilhadeira 1,20, AGV e outro veículo 1,00, pessoa 0,60) ou, se maior, pela folga
    /// dada: `2 × (clearance − 0,3)`.
    pub fn vehicle_width(&self, obj: &ObjectDef) -> f64 {
        let role = self.role_of(obj);
        let by_role = 2.0 * half_width(role);
        match &obj.kind {
            ObjectKind::Transporter {
                clearance: Some(c), ..
            } => by_role.max(2.0 * (c - utsim_nav::body::MARGIN)),
            _ => by_role,
        }
    }

    /// Marcação do piso (a do modelo, senão o padrão: perfil Brasil, faixa de 10 cm,
    /// parada de 2 s).
    pub fn marking(&self) -> FloorMarking {
        self.floor_marking.as_deref().cloned().unwrap_or_default()
    }

    /// Transportadores que andam pelos corredores (modo `Aisles`, fora do controle de
    /// tráfego dos AGVs com rede).
    fn aisle_walkers(&self) -> Vec<&ObjectDef> {
        self.objects
            .iter()
            .filter(|o| self.movement_of(o) == Some(Movement::Aisles) && !self.traffic_agv(o))
            .collect()
    }

    /// Pontos ligados aos corredores: a posição de cada recurso em `Aisles` e os pontos de
    /// acesso (com a folga dele) dos objetos que ele atende.
    fn aisle_points(&self) -> Vec<(String, Pt, User)> {
        let mut out = Vec::new();
        for t in self.aisle_walkers() {
            let user = self.aisle_user(t);
            out.push((t.name.clone(), [t.position[0], t.position[1]], user));
            let c = self.clearance_of(t);
            let bodies = self.nav_bodies();
            for o in self.served_by(&t.name) {
                let Some(b) = bodies.iter().find(|b| b.name == o.name) else {
                    continue;
                };
                out.push((o.name.clone(), b.access_point(c), user));
                // A belt is also reached at its ends.
                if let Some((i, e)) = b.end_stands(c) {
                    out.push((o.name.clone(), i, user));
                    out.push((o.name.clone(), e, user));
                }
            }
        }
        out
    }

    /// Rede gerada pelos corredores, com as ligações dos recursos em `Aisles` e dos pontos
    /// de acesso dos objetos que eles atendem.
    pub fn aisle_net(&self) -> AisleNet {
        utsim_nav::aisle_network(&self.aisles, &self.nav_obstacles(), &self.aisle_points())
    }

    /// Roteadores do modo `Aisles` (um por classe e folga) e cruzamentos, em `set`.
    pub(crate) fn aisle_routers(&self, set: &mut NavSet) {
        let walkers = self.aisle_walkers();
        if walkers.is_empty() || self.aisles.is_empty() {
            return;
        }
        let net = self.aisle_net();
        let mut nets: BTreeMap<User, utsim_objects::network::NetworkDef> = BTreeMap::new();
        let mut routers: BTreeMap<(User, u64), Arc<AislesRouter>> = BTreeMap::new();
        for t in walkers {
            let user = self.aisle_user(t);
            let c = self.clearance_of(t);
            let def = nets
                .entry(user)
                .or_insert_with(|| net.network_for(user))
                .clone();
            let router = routers
                .entry((user, c.to_bits()))
                .or_insert_with(|| Arc::new(AislesRouter::new(&def, self.stops(c))))
                .clone();
            set.routers
                .insert(t.name.clone(), router as Arc<dyn Router>);
            set.crossing_users
                .insert(t.name.clone(), user == User::Vehicle);
        }
        set.crossings = net
            .crossings
            .iter()
            .map(|c| CrossingSpec {
                name: c.name.clone(),
                position: [c.position[0], c.position[1], 0.0],
                radius: c.radius,
            })
            .collect();
        set.crossing_stop = self.marking().crossing_stop;
    }

    pub(crate) fn validate_aisles(&self, errors: &mut Vec<String>) {
        let mut names = BTreeSet::new();
        for a in &self.aisles {
            if let Some(p) = a.problem() {
                errors.push(p);
            }
            if !names.insert(a.name.as_str()) {
                errors.push(format!("corredor repetido: '{}'", a.name));
            }
        }
        if let Some(p) = self
            .floor_marking
            .as_deref()
            .and_then(FloorMarking::problem)
        {
            errors.push(p);
        }
    }

    /// Avisos dos corredores (ver `Model::warnings`).
    pub(crate) fn aisle_warnings(&self, out: &mut Vec<String>) {
        if self.aisles.is_empty() {
            return;
        }
        let marking = self.marking();
        if !(0.05..=0.15).contains(&marking.line_width) {
            out.push(format!(
                "a faixa de delimitação tem {} m; a prática usual vai de 0,05 a 0,15 m",
                fmt_m(marking.line_width)
            ));
        }
        let transporters: Vec<&ObjectDef> = self
            .objects
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Transporter { .. }))
            .collect();
        let walkers = self.aisle_walkers();
        // Vehicles that use the aisles: those in `Aisles`, else every vehicle.
        let mut vehicles: Vec<&ObjectDef> = walkers
            .iter()
            .copied()
            .filter(|t| self.aisle_user(t) == User::Vehicle)
            .collect();
        if vehicles.is_empty() {
            vehicles = transporters
                .iter()
                .copied()
                .filter(|t| self.aisle_user(t) == User::Vehicle)
                .collect();
        }
        let widest = vehicles
            .iter()
            .map(|t| (self.vehicle_width(t), t.name.clone()))
            .fold(None::<(f64, String)>, |acc, x| match acc {
                Some(a) if a.0 >= x.0 => Some(a),
                _ => Some(x),
            })
            .unwrap_or((
                2.0 * half_width(utsim_nav::Role::Forklift),
                "empilhadeira padrão".to_string(),
            ));
        for a in self.aisles.iter().filter(|a| a.problem().is_none()) {
            let profile = a.profile.as_ref().unwrap_or(&marking.profile);
            let spec = profile.spec();
            let l = if a.kind == AisleKind::Pedestrian {
                0.0
            } else {
                widest.0
            };
            let min = spec.min_width(a.kind, a.two_way, a.secondary, l);
            if a.width + 1e-9 < min {
                let sentido = if a.two_way {
                    "mão dupla"
                } else {
                    "mão única"
                };
                let base = format!(
                    "o corredor '{}' ({}, {sentido}) tem {} m; o mínimo do perfil {} é {} m",
                    a.name,
                    a.kind.label(),
                    fmt_m(a.width),
                    profile.label(),
                    fmt_m(min)
                );
                let why = match a.kind {
                    AisleKind::Pedestrian if a.secondary => {
                        "passagem secundária, prática usual".to_string()
                    }
                    AisleKind::Pedestrian if a.width + 1e-9 >= spec.pedestrian_secondary_min => {
                        format!(
                            "via principal; aceito como passagem secundária a partir de {} m com `secondary: true`",
                            fmt_m(spec.pedestrian_secondary_min)
                        )
                    }
                    AisleKind::Pedestrian => "via principal".to_string(),
                    _ => format!(
                        "{} com L = {} m de '{}'; prática de mercado, não norma",
                        spec.rule(a.kind, a.two_way, a.secondary),
                        fmt_m(widest.0),
                        widest.1
                    ),
                };
                out.push(format!("{base} ({why})"));
            }
            let obstacles = self.nav_obstacles();
            let mut hit: BTreeMap<String, bool> = BTreeMap::new();
            for w in a.points.windows(2) {
                for o in &obstacles {
                    let d = utsim_nav::geom::segment_polygon(w[0], w[1], &o.polygon);
                    if d <= 0.0 {
                        hit.insert(o.name.clone(), true);
                    } else if d + 1e-9 < a.width / 2.0 {
                        hit.entry(o.name.clone()).or_insert(false);
                    }
                }
            }
            for (name, center) in hit {
                if center {
                    out.push(format!(
                        "o eixo do corredor '{}' cruza '{name}': o trecho fica fora da rede até ser corrigido",
                        a.name
                    ));
                } else {
                    out.push(format!(
                        "a faixa do corredor '{}' ({} m de largura) invade '{name}': afaste o eixo ou estreite o corredor",
                        a.name,
                        fmt_m(a.width)
                    ));
                }
            }
        }
        // Resources whose class has no aisle.
        for t in &transporters {
            let user = self.aisle_user(t);
            if self.aisles.iter().any(|a| a.kind.allows(user)) {
                continue;
            }
            let walking = walkers.iter().any(|w| w.name == t.name);
            out.push(format!(
                "'{}' ({}) não tem corredor {}{}",
                t.name,
                role_name(self.role_of(t)),
                user.aisles_label(),
                if walking {
                    ": em movimentação pelos corredores, as viagens sairão em linha reta"
                } else {
                    ""
                }
            ));
        }
        if walkers.is_empty() {
            return;
        }
        // Access points: attachment and reach within the class.
        let net = self.aisle_net();
        let mut by_user: HashMap<User, utsim_objects::network::NetworkRouter> = HashMap::new();
        for t in &walkers {
            let user = self.aisle_user(t);
            if !self.aisles.iter().any(|a| a.kind.allows(user)) {
                continue;
            }
            let router = by_user.entry(user).or_insert_with(|| {
                utsim_objects::network::NetworkRouter::new(&net.network_for(user))
            });
            let c = self.clearance_of(t);
            let start = [t.position[0], t.position[1], 0.0];
            for o in self.served_by(&t.name) {
                let Some(p) = self.access_point(&o.name, c) else {
                    continue;
                };
                let Some(at) = net.attachment(p, user) else {
                    continue;
                };
                if at.node.is_none() {
                    out.push(format!(
                        "o ponto de acesso de '{}' não tem corredor {} que o alcance para '{}'",
                        o.name,
                        user.aisles_label(),
                        t.name
                    ));
                    continue;
                }
                if !at.crosses.is_empty() {
                    out.push(format!(
                        "o ponto de acesso de '{}' ({:.2}, {:.2}) não tem corredor {} que o alcance sem cruzar '{}' (para '{}'): \
                         a ligação ao corredor mais próximo passa por cima",
                        o.name,
                        p[0],
                        p[1],
                        user.aisles_label(),
                        at.crosses.join("', '"),
                        t.name
                    ));
                }
                if router.node_route(start, [p[0], p[1], 0.0]).is_none() {
                    out.push(format!(
                        "'{}' não alcança o ponto de acesso de '{}' pelos corredores {}: \
                         ligue os corredores ou mude o modo",
                        t.name,
                        o.name,
                        user.aisles_label()
                    ));
                }
            }
        }
    }

    /// Resumo dos corredores e da rede gerada, uma linha por item.
    pub fn aisle_lines(&self) -> Vec<String> {
        let marking = self.marking();
        let mut out = vec![format!(
            "perfil {}, faixa de {} m, parada de {} s antes dos cruzamentos",
            marking.profile.label(),
            fmt_m(marking.line_width),
            fmt_m(marking.crossing_stop)
        )];
        for a in &self.aisles {
            out.push(format!(
                "corredor '{}': {}, {} m de largura, {}, {} m",
                a.name,
                a.kind.label(),
                fmt_m(a.width),
                if a.two_way {
                    "mão dupla"
                } else {
                    "mão única"
                },
                fmt_m(a.length())
            ));
        }
        let net = self.aisle_net();
        let connectors = net
            .edges
            .iter()
            .filter(|e| matches!(e.usage, EdgeUse::Connector(_)))
            .count();
        out.push(format!(
            "rede gerada: {} nós, {} trechos de corredor, {} ligações a pontos de acesso, {} cruzamentos",
            net.network.nodes.len(),
            net.edges.len() - connectors,
            connectors,
            net.crossings.len()
        ));
        for c in &net.crossings {
            out.push(format!(
                "cruzamento {} ({}) em ({:.2}, {:.2}): prioridade ao pedestre, zona de {} m",
                c.name,
                c.node,
                c.position[0],
                c.position[1],
                fmt_m(2.0 * c.radius)
            ));
        }
        out
    }

    /// Faixas no piso para o desenho (polígonos com cor e padrão), no perfil do modelo.
    pub fn floor_marks(&self) -> Vec<MarkShape> {
        utsim_nav::floor_marks(&self.aisles, &self.aisle_net(), &self.marking())
    }

    /// Planta em SVG com os objetos, os obstáculos, os corredores com as faixas, a rede
    /// gerada, as ligações e os cruzamentos.
    pub fn aisles_svg(&self) -> String {
        let net = self.aisle_net();
        let marks = self.floor_marks();
        let bodies = self.nav_bodies();
        let mut pts: Vec<Pt> = Vec::new();
        for a in &self.aisles {
            for p in &a.points {
                pts.push([p[0] - a.width, p[1] - a.width]);
                pts.push([p[0] + a.width, p[1] + a.width]);
            }
        }
        pts.extend(bodies.iter().flat_map(|b| b.polygon()));
        pts.extend(self.obstacles.iter().flat_map(|o| o.polygon.clone()));
        pts.extend(self.objects.iter().map(|o| [o.position[0], o.position[1]]));
        let bb = utsim_nav::geom::bbox(&pts).unwrap_or([[0.0, 0.0], [10.0, 10.0]]);
        let margin = 2.0;
        let (x0, y0) = (bb[0][0] - margin, bb[0][1] - margin);
        let (w, h) = (
            bb[1][0] - bb[0][0] + 2.0 * margin,
            bb[1][1] - bb[0][1] + 2.0 * margin,
        );
        let scale = (1600.0 / w.max(h)).clamp(4.0, 40.0);
        let tx = |p: Pt| ((p[0] - x0) * scale, (y0 + h - p[1]) * scale);
        let poly = |ps: &[Pt]| {
            ps.iter()
                .map(|p| {
                    let (x, y) = tx(*p);
                    format!("{x:.1},{y:.1}")
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        };
        let mut s = String::new();
        let _ = writeln!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" viewBox="0 0 {:.0} {:.0}" font-family="sans-serif" font-size="11">"#,
            w * scale,
            h * scale + 24.0,
            w * scale,
            h * scale + 24.0
        );
        let _ = writeln!(
            s,
            r##"<rect width="100%" height="100%" fill="#8a8f94"/><title>{}: corredores</title>"##,
            esc(&self.name)
        );
        for a in self.aisles.iter().filter(|a| a.problem().is_none()) {
            let fill = match a.kind {
                AisleKind::Pedestrian => "#9fb4c7",
                AisleKind::Forklift => "#a39a8a",
                AisleKind::Mixed => "#9aab95",
            };
            let left = offset_polyline(&a.points, a.width / 2.0);
            let mut right = offset_polyline(&a.points, -a.width / 2.0);
            right.reverse();
            let mut band = left;
            band.extend(right);
            let _ = writeln!(
                s,
                r#"<polygon points="{}" fill="{fill}"><title>{} ({})</title></polygon>"#,
                poly(&band),
                esc(&a.name),
                a.kind.label()
            );
        }
        for m in &marks {
            let stroke = if m.pattern == Pattern::Zebra {
                ""
            } else {
                r##" stroke="#00000033" stroke-width="0.5""##
            };
            let _ = writeln!(
                s,
                r#"<polygon points="{}" fill="{}"{stroke}/>"#,
                poly(&m.polygon),
                m.color
            );
        }
        for o in &self.obstacles {
            let _ = writeln!(
                s,
                r##"<polygon points="{}" fill="#3b3b3b"><title>{}</title></polygon>"##,
                poly(&o.polygon),
                esc(&o.name)
            );
        }
        for b in &bodies {
            let (cx, cy) = tx(b.center);
            let _ = writeln!(
                s,
                r##"<polygon points="{}" fill="#d9d9d9" stroke="#555" stroke-width="1"/><text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" fill="#222">{}</text>"##,
                poly(&b.polygon()),
                esc(&b.name)
            );
        }
        let pos: HashMap<&str, [f64; 3]> = net
            .network
            .nodes
            .iter()
            .map(|n| (n.name.as_str(), n.position))
            .collect();
        for (e, info) in net.network.edges.iter().zip(&net.edges) {
            let (a, b) = (pos[e.from.as_str()], pos[e.to.as_str()]);
            let ((x1, y1), (x2, y2)) = (tx([a[0], a[1]]), tx([b[0], b[1]]));
            let style = match info.usage {
                EdgeUse::Aisle(_) => r##"stroke="#ffffff99" stroke-width="1""##,
                EdgeUse::Connector(User::Person) => {
                    r##"stroke="#1f5fbf" stroke-width="1.5" stroke-dasharray="4 3""##
                }
                EdgeUse::Connector(User::Vehicle) => {
                    r##"stroke="#b35a00" stroke-width="1.5" stroke-dasharray="4 3""##
                }
            };
            let _ = writeln!(
                s,
                r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" {style}/>"#
            );
        }
        for b in &net.blocked {
            let ((x1, y1), (x2, y2)) = (tx(b.from), tx(b.to));
            let _ = writeln!(
                s,
                r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#d00" stroke-width="2"><title>{} cruza {}</title></line>"##,
                esc(&b.aisle),
                esc(&b.obstacles.join(", "))
            );
        }
        for c in &net.crossings {
            let (x, y) = tx(c.position);
            let _ = writeln!(
                s,
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="{:.1}" fill="none" stroke="#d00" stroke-width="1.5"><title>cruzamento {}</title></circle>"##,
                c.radius * scale,
                esc(&c.name)
            );
        }
        for o in self
            .objects
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Transporter { .. }))
        {
            let (x, y) = tx([o.position[0], o.position[1]]);
            let color = if self.aisle_user(o) == User::Person {
                "#1f5fbf"
            } else {
                "#b35a00"
            };
            let _ = writeln!(
                s,
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="4" fill="{color}"/><text x="{:.1}" y="{:.1}" fill="#111">{}</text>"##,
                x + 6.0,
                y - 6.0,
                esc(&o.name)
            );
        }
        let _ = writeln!(
            s,
            r##"<text x="8" y="{:.0}" fill="#111">perfil {}: azul = pedestres, marrom = empilhadeiras, verde = misto; círculos vermelhos = cruzamentos; tracejado = ligações</text>"##,
            h * scale + 16.0,
            esc(self.marking().profile.label())
        );
        s.push_str("</svg>\n");
        s
    }
}
