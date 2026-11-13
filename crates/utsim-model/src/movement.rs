//! Modos de movimentação dos recursos móveis (formato 2): obstáculos, rotas por recurso,
//! geração da rede a partir do caminho livre, validação e avisos.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use utsim_core::Router;
use utsim_nav::{
    Body, FreeRouter, Generated, GridOptions, NavGrid, NetworkModeRouter, Obstacle, Pt, Role, Shape,
};
use utsim_objects::network::NetworkRouter;
use utsim_objects::traffic::{CrossingSpec, Crossings};

use crate::{Model, ObjectDef, ObjectKind, is_mobile};

/// Como um recurso móvel anda.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Movement {
    /// Como no formato 1: linha reta sem rede; com rede, nó mais próximo em linha reta e
    /// menor caminho na rede, até o centro do objeto. Modelos antigos migram para ele.
    Legacy,
    /// Menor caminho no piso desviando dos objetos, das paredes e dos obstáculos, até o
    /// ponto de acesso do objeto. Padrão do formato 2.
    Free,
    /// Pela rede de caminhos, com a ligação livre (desviando) até o ponto de acesso.
    Network,
    /// Só pelos corredores da classe do recurso (`aisles`): pessoas nos de pedestres e
    /// mistos, veículos nos de empilhadeiras e mistos; cruzamentos com prioridade ao pedestre.
    Aisles,
}

impl Movement {
    pub fn label(self) -> &'static str {
        match self {
            Movement::Legacy => "legado (linha reta ou rede, até o centro do objeto)",
            Movement::Free => "livre (menor caminho no piso, desviando dos obstáculos)",
            Movement::Network => "pela rede de caminhos (ligação livre até o ponto de acesso)",
            Movement::Aisles => "pelos corredores (só os da sua classe e os mistos)",
        }
    }
}

/// Roteadores dos recursos que não usam o modo legado, por nome. Montados uma vez e
/// compartilhados pelas replicações (os caches não mudam resultados).
#[derive(Clone, Default)]
pub struct NavSet {
    pub(crate) routers: HashMap<String, Arc<dyn Router>>,
    /// Recursos em `Aisles` que entram nas reservas de cruzamento: nome → é veículo.
    pub(crate) crossing_users: HashMap<String, bool>,
    /// Cruzamentos pedestre × veículo e a parada obrigatória (s).
    pub(crate) crossings: Vec<CrossingSpec>,
    pub(crate) crossing_stop: f64,
    /// Rampas da rede de corredores de cada recurso em `Aisles` (só quando há rampa).
    pub(crate) ramps: HashMap<String, Arc<utsim_objects::ramps::Ramps>>,
}

impl NavSet {
    pub fn router(&self, name: &str) -> Option<Arc<dyn Router>> {
        self.routers.get(name).cloned()
    }

    /// Reservas de cruzamento novas (uma por simulação), se algum recurso as usa.
    pub fn crossing_board(&self) -> Option<Arc<Mutex<Crossings>>> {
        (!self.crossing_users.is_empty() && !self.crossings.is_empty()).then(|| {
            Arc::new(Mutex::new(Crossings::new(
                self.crossings.clone(),
                self.crossing_stop,
            )))
        })
    }

    /// `(board, é veículo)` para o transportador `name`, se ele entra nas reservas.
    /// As rampas por onde o recurso anda (ele vai mais devagar nelas).
    pub fn ramps_for(&self, name: &str) -> Option<Arc<utsim_objects::ramps::Ramps>> {
        self.ramps.get(name).cloned()
    }

    pub fn crossings_for(
        &self,
        name: &str,
        board: &Option<Arc<Mutex<Crossings>>>,
    ) -> Option<(Arc<Mutex<Crossings>>, bool)> {
        let vehicle = *self.crossing_users.get(name)?;
        board.clone().map(|b| (b, vehicle))
    }
}

pub(crate) fn role_name(role: Role) -> &'static str {
    match role {
        Role::Person => "pessoa",
        Role::Forklift => "empilhadeira",
        Role::Agv => "AGV",
        Role::Vehicle => "veículo",
    }
}

pub(crate) fn fmt_m(v: f64) -> String {
    format!("{v:.2}").replace('.', ",")
}

impl Model {
    /// Modo de um transportador: o dele, senão o do modelo, senão `Free`. `None` para os
    /// outros objetos.
    pub fn movement_of(&self, obj: &ObjectDef) -> Option<Movement> {
        match &obj.kind {
            ObjectKind::Transporter { movement, .. } => {
                Some(movement.or(self.run.movement).unwrap_or(Movement::Free))
            }
            _ => None,
        }
    }

    /// O transportador e os despachantes cuja equipe o inclui.
    fn carriers<'a>(&'a self, name: &'a str) -> BTreeSet<&'a str> {
        let mut out = BTreeSet::from([name]);
        for o in &self.objects {
            if let ObjectKind::Dispatcher { team, .. } = &o.kind {
                if team.iter().any(|m| m == name) {
                    out.insert(o.name.as_str());
                }
            }
        }
        out
    }

    /// Objetos que o recurso atende (por `transport`, pelos destinos dessas saídas e por
    /// `operators`), na ordem do modelo.
    pub fn served_by(&self, name: &str) -> Vec<&ObjectDef> {
        let carriers = self.carriers(name);
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for o in &self.objects {
            if o.transport.as_deref().is_some_and(|t| carriers.contains(t)) {
                names.insert(o.name.as_str());
                for (_, to) in self.connections.iter().filter(|(f, _)| *f == o.name) {
                    names.insert(to.as_str());
                }
            }
            if o.kind
                .operator_uses()
                .any(|op| carriers.contains(op.team.as_str()))
            {
                names.insert(o.name.as_str());
            }
        }
        self.objects
            .iter()
            .filter(|o| names.contains(o.name.as_str()) && !is_mobile(&o.kind))
            .collect()
    }

    /// Papel do recurso, como no desenho 3D: pelo ativo, `agv`, porta-paletes atendido,
    /// operador; senão veículo.
    pub fn role_of(&self, obj: &ObjectDef) -> Role {
        if let Some(r) = obj
            .visual
            .as_ref()
            .and_then(|v| v.asset.as_deref())
            .and_then(utsim_nav::role_of_asset)
        {
            return r;
        }
        if matches!(obj.kind, ObjectKind::Transporter { agv: true, .. }) {
            return Role::Agv;
        }
        let carriers = self.carriers(&obj.name);
        let is_rack = |n: &str| {
            self.objects
                .iter()
                .any(|o| o.name == n && matches!(o.kind, ObjectKind::Rack { .. }))
        };
        let serves_rack = self.objects.iter().any(|o| {
            o.transport.as_deref().is_some_and(|t| carriers.contains(t))
                && (is_rack(&o.name)
                    || self
                        .connections
                        .iter()
                        .any(|(f, t)| *f == o.name && is_rack(t)))
        });
        if serves_rack {
            return Role::Forklift;
        }
        let operator = self.objects.iter().any(|o| {
            o.kind
                .operator_uses()
                .any(|op| carriers.contains(op.team.as_str()))
        });
        if operator {
            Role::Person
        } else {
            Role::Vehicle
        }
    }

    /// Folga do recurso (m): a dele (`clearance`), senão meia largura do papel + 0,3 m.
    pub fn clearance_of(&self, obj: &ObjectDef) -> f64 {
        match &obj.kind {
            ObjectKind::Transporter {
                clearance: Some(c), ..
            } => *c,
            _ => utsim_nav::default_clearance(self.role_of(obj)),
        }
    }

    /// Área ocupada de cada objeto fixo (os recursos móveis não são obstáculos).
    pub fn nav_bodies(&self) -> Vec<Body> {
        let pos: HashMap<&str, [f64; 3]> = self
            .objects
            .iter()
            .map(|o| (o.name.as_str(), o.position))
            .collect();
        self.objects
            .iter()
            .filter_map(|o| {
                let shape = match &o.kind {
                    ObjectKind::Source { .. } => Shape::Source,
                    ObjectKind::Queue { .. } => Shape::Queue,
                    ObjectKind::Processor { .. } => Shape::Processor,
                    ObjectKind::Combiner { .. } => Shape::Combiner,
                    ObjectKind::Separator { .. } => Shape::Separator,
                    ObjectKind::Sink => Shape::Sink,
                    ObjectKind::Conveyor { length, .. } => Shape::Conveyor { length: *length },
                    ObjectKind::Rack {
                        bays, bay_width, ..
                    } => Shape::Rack {
                        bays: *bays,
                        bay_width: *bay_width,
                    },
                    ObjectKind::Transporter { .. } | ObjectKind::Dispatcher { .. } => {
                        return None;
                    }
                };
                // A belt points to its first outgoing connection (as drawn in 3D).
                let belt_yaw = matches!(shape, Shape::Conveyor { .. })
                    .then(|| {
                        let (_, to) = self.connections.iter().find(|(f, _)| *f == o.name)?;
                        let q = pos.get(to.as_str())?;
                        let (dx, dy) = (q[0] - o.position[0], q[1] - o.position[1]);
                        (dx.abs() + dy.abs() > 1e-9).then(|| dy.atan2(dx))
                    })
                    .flatten();
                let visual = o.visual.as_ref();
                let yaw = visual
                    .and_then(|v| v.yaw)
                    .map(f64::to_radians)
                    .or(belt_yaw)
                    .unwrap_or(0.0);
                Some(Body {
                    name: o.name.clone(),
                    shape,
                    center: [o.position[0], o.position[1]],
                    yaw,
                    scale: visual.and_then(|v| v.scale).unwrap_or(1.0),
                    access: o.access,
                    size: o.size.map(|d| [d[0], d[1]]),
                    route: match &o.kind {
                        ObjectKind::Conveyor { route: Some(r), .. }
                            if utsim_nav::path3d::problem(&r.nodes, "").is_none() =>
                        {
                            Some((utsim_nav::path3d::plan_line(&r.line(o.position)), r.width()))
                        }
                        _ => None,
                    },
                })
            })
            .collect()
    }

    /// Obstáculos da navegação: áreas dos objetos fixos e `obstacles` do modelo (paredes e
    /// colunas da planta ou desenhados à mão).
    pub fn nav_obstacles(&self) -> Vec<Obstacle> {
        let mut out: Vec<Obstacle> = self.nav_bodies().iter().map(Body::obstacle).collect();
        out.extend(self.obstacles.iter().cloned());
        out
    }

    /// Grade de navegação para uma folga, cobrindo todos os objetos e os pontos de acesso.
    pub fn nav_grid(&self, clearance: f64) -> NavGrid {
        let bodies = self.nav_bodies();
        let mut points: Vec<Pt> = self
            .objects
            .iter()
            .map(|o| [o.position[0], o.position[1]])
            .collect();
        points.extend(bodies.iter().map(|b| b.access_point(clearance)));
        for (i, o) in bodies.iter().filter_map(|b| b.end_stands(clearance)) {
            points.extend([i, o]);
        }
        if let Some(net) = &self.network {
            points.extend(net.nodes.iter().map(|n| [n.position[0], n.position[1]]));
        }
        NavGrid::new(
            &self.nav_obstacles(),
            clearance,
            &points,
            &GridOptions::default(),
        )
    }

    /// Ponto de acesso do objeto `name` para um recurso com essa folga.
    pub fn access_point(&self, name: &str, clearance: f64) -> Option<Pt> {
        self.nav_bodies()
            .into_iter()
            .find(|b| b.name == name)
            .map(|b| b.access_point(clearance))
    }

    fn free_router(&self, clearance: f64) -> FreeRouter {
        FreeRouter::new(Arc::new(self.nav_grid(clearance)), self.stops(clearance))
    }

    /// Where a resource with this clearance stops for each point it may be sent to: the
    /// position of a fixed object goes to its access point and the ends of a belt
    /// ([`Model::belt_ends`]) to the places beside them.
    pub(crate) fn stops(&self, clearance: f64) -> Vec<([f64; 3], Pt)> {
        let bodies = self.nav_bodies();
        let fixed = self.objects.iter().filter(|o| !is_mobile(&o.kind));
        let mut out: Vec<([f64; 3], Pt)> = bodies
            .iter()
            .zip(fixed)
            .map(|(b, o)| (o.position, b.access_point(clearance)))
            .collect();
        for (name, i, e) in self.belt_ends() {
            let stands = bodies
                .iter()
                .find(|b| b.name == name)
                .and_then(|b| b.end_stands(clearance));
            if let Some((si, se)) = stands {
                out.push((i, si));
                out.push((e, se));
            }
        }
        out
    }

    /// Input and output ends of each belt, by name, at its height: where the mobile
    /// resources in `Free`, `Network` and `Aisles` movement drop and pick up its items.
    ///
    /// They are also the keys the routers look up (bit for bit), so each one differs from
    /// every object's position and every other end: one that falls exactly on another
    /// (two belts end to end, a belt ending at a machine's centre) moves by the smallest
    /// step in `x` (one epsilon), far below anything a distance shows.
    pub fn belt_ends(&self) -> Vec<(String, [f64; 3], [f64; 3])> {
        let bits = |p: [f64; 3]| p.map(f64::to_bits);
        let mut taken: Vec<[u64; 3]> = self
            .objects
            .iter()
            .filter(|o| !is_mobile(&o.kind))
            .map(|o| bits(o.position))
            .collect();
        let mut unique = |mut p: [f64; 3]| {
            while taken.contains(&bits(p)) {
                p[0] += p[0].abs().max(1.0) * f64::EPSILON;
            }
            taken.push(bits(p));
            p
        };
        let fixed = self.objects.iter().filter(|o| !is_mobile(&o.kind));
        self.nav_bodies()
            .iter()
            .zip(fixed)
            .filter_map(|(b, o)| {
                let (i, e) = b.belt_ends()?;
                // A drawn belt's ends are at the heights of its first and last nodes.
                if let ObjectKind::Conveyor { route: Some(r), .. } = &o.kind {
                    if let Some((ri, re)) = r.ends(o.position).filter(|_| b.route.is_some()) {
                        return Some((o.name.clone(), ri, re));
                    }
                }
                let z = o.position[2];
                Some((o.name.clone(), [i[0], i[1], z], [e[0], e[1], z]))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(n, i, e)| {
                let i = unique(i);
                let e = unique(e);
                (n, i, e)
            })
            .collect()
    }

    /// Roteadores dos transportadores em `Free` e `Network`, um por folga (e modo).
    pub fn nav_set(&self) -> NavSet {
        let mut free: BTreeMap<u64, Arc<FreeRouter>> = BTreeMap::new();
        let network = self
            .network
            .as_ref()
            .map(|def| Arc::new(NetworkRouter::new(def)));
        let mut set = NavSet::default();
        self.aisle_routers(&mut set);
        for o in &self.objects {
            let Some(mode) = self.routed_mode(o) else {
                continue;
            };
            if mode == Movement::Aisles {
                continue;
            }
            let c = self.clearance_of(o);
            let fr = free
                .entry(c.to_bits())
                .or_insert_with(|| Arc::new(self.free_router(c)))
                .clone();
            let router: Arc<dyn Router> = match (mode, &network) {
                (Movement::Network, Some(net)) => Arc::new(NetworkModeRouter::new(net.clone(), fr)),
                _ => fr,
            };
            set.routers.insert(o.name.clone(), router);
        }
        set
    }

    /// Rede gerada pelos caminhos livres entre os pontos de acesso dos objetos atendidos
    /// por recursos móveis e as posições dos recursos, com a folga dada ou a maior entre
    /// os recursos (a rede serve a todos). Sem recursos, liga os pontos de acesso de todos
    /// os objetos.
    pub fn generate_network(&self, clearance: Option<f64>) -> Generated {
        let transporters: Vec<&ObjectDef> = self
            .objects
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Transporter { .. }))
            .collect();
        let c = clearance.unwrap_or_else(|| {
            transporters
                .iter()
                .map(|t| self.clearance_of(t))
                .fold(0.0, f64::max)
        });
        let bodies = self.nav_bodies();
        let mut wanted: BTreeSet<&str> = BTreeSet::new();
        for t in &transporters {
            wanted.extend(self.served_by(&t.name).iter().map(|o| o.name.as_str()));
        }
        let mut points: Vec<(String, Pt)> = transporters
            .iter()
            .map(|t| (t.name.clone(), [t.position[0], t.position[1]]))
            .collect();
        points.extend(
            bodies
                .iter()
                .filter(|b| wanted.is_empty() || wanted.contains(b.name.as_str()))
                .map(|b| (b.name.clone(), b.access_point(c))),
        );
        utsim_nav::generate_network(&self.nav_grid(c), &points)
    }

    /// Um AGV com rede segue a rede aresta por aresta sob controle de tráfego, qualquer
    /// que seja o modo.
    pub(crate) fn traffic_agv(&self, obj: &ObjectDef) -> bool {
        matches!(obj.kind, ObjectKind::Transporter { agv: true, .. }) && self.network.is_some()
    }

    /// Modo do recurso quando ele usa um roteador próprio (`Free`, `Network` ou `Aisles`).
    pub(crate) fn routed_mode(&self, obj: &ObjectDef) -> Option<Movement> {
        self.movement_of(obj)
            .filter(|m| matches!(m, Movement::Free | Movement::Network | Movement::Aisles))
            .filter(|_| !self.traffic_agv(obj))
    }

    /// Uma linha por recurso móvel: `nome: modo (papel, folga)`.
    pub fn movement_lines(&self) -> Vec<String> {
        self.objects
            .iter()
            .filter_map(|o| {
                let mode = self.movement_of(o)?;
                if self.traffic_agv(o) {
                    return Some(format!(
                        "{}: AGV pela rede, aresta por aresta, com controle de tráfego \
                         (o modo de movimentação não se aplica)",
                        o.name
                    ));
                }
                let mut line = format!("{}: {}", o.name, mode.label());
                if mode == Movement::Aisles && !self.traffic_agv(o) {
                    let user = self.aisle_user(o);
                    line.push_str(&format!(
                        " ({}, corredores {})",
                        role_name(self.role_of(o)),
                        user.aisles_label()
                    ));
                } else if self.routed_mode(o).is_some() {
                    line.push_str(&format!(
                        " ({}, folga {} m)",
                        role_name(self.role_of(o)),
                        fmt_m(self.clearance_of(o))
                    ));
                }
                Some(line)
            })
            .collect()
    }

    pub(crate) fn validate_movement(&self, errors: &mut Vec<String>) {
        let check = |who: &str, m: Option<Movement>, errors: &mut Vec<String>| {
            match m {
            Some(Movement::Network) if self.network.is_none() => errors.push(format!(
                "{who}: movement = Network exige `network` no modelo (desenhe a rede ou gere com `utsim rede gerar`)"
            )),
            Some(Movement::Aisles) if self.aisles.is_empty() => errors.push(format!(
                "{who}: movement = Aisles exige `aisles` no modelo (desenhe os corredores)"
            )),
            _ => {}
        }
        };
        check("run", self.run.movement, errors);
        for o in &self.objects {
            if let ObjectKind::Transporter {
                movement,
                clearance,
                ..
            } = &o.kind
            {
                check(&o.name, *movement, errors);
                if clearance.is_some_and(|c| !(c.is_finite() && c >= 0.0)) {
                    errors.push(format!("{}: clearance deve ser finita e >= 0", o.name));
                }
            }
            if let Some(a) = o.access {
                if !a.iter().all(|v| v.is_finite()) {
                    errors.push(format!("{}: access precisa de coordenadas finitas", o.name));
                }
                if is_mobile(&o.kind) {
                    errors.push(format!(
                        "{}: recursos móveis não têm ponto de acesso",
                        o.name
                    ));
                }
            }
        }
        let mut names = BTreeSet::new();
        for ob in &self.obstacles {
            if let Some(p) = ob.problem() {
                errors.push(p);
            }
            if !names.insert(ob.name.as_str()) {
                errors.push(format!("obstáculo repetido: '{}'", ob.name));
            }
        }
    }

    /// Avisos da movimentação (ver `Model::warnings`).
    pub(crate) fn movement_warnings(&self, out: &mut Vec<String>) {
        let mut grids: BTreeMap<u64, NavGrid> = BTreeMap::new();
        let mut uses_network = false;
        let bodies = self.nav_bodies();
        for t in &self.objects {
            let Some(mode) = self.routed_mode(t) else {
                continue;
            };
            if mode == Movement::Aisles {
                continue; // checked by the aisle warnings
            }
            uses_network |= mode == Movement::Network;
            let c = self.clearance_of(t);
            let grid = grids.entry(c.to_bits()).or_insert_with(|| self.nav_grid(c));
            let served = self.served_by(&t.name);
            if served.is_empty() {
                continue;
            }
            let start = [t.position[0], t.position[1]];
            let mut unreachable = Vec::new();
            for o in &served {
                let Some(a) = self.access_point(&o.name, c) else {
                    continue;
                };
                if !grid.is_free(a) {
                    let inside = grid.containing(a);
                    let why = if inside.is_empty() {
                        format!("a menos da folga de {} m de um obstáculo", fmt_m(c))
                    } else {
                        format!("dentro de '{}'", inside.join("', '"))
                    };
                    out.push(format!(
                        "o ponto de acesso de '{}' ({:.2}, {:.2}) fica {why} para '{}': \
                         afaste os objetos ou defina `access`",
                        o.name, a[0], a[1], t.name
                    ));
                }
                if !grid.connected(start, a) {
                    unreachable.push(o.name.as_str());
                }
                // A belt is served at its ends: those places must be free too.
                let stands = bodies
                    .iter()
                    .find(|b| b.name == o.name)
                    .and_then(|b| b.end_stands(c));
                for (end, p) in stands
                    .into_iter()
                    .flat_map(|(i, e)| [("entrada", i), ("saída", e)])
                {
                    if !grid.is_free(p) {
                        out.push(format!(
                            "a ponta de {end} de '{}' ({:.2}, {:.2}) fica dentro de um obstáculo ou a menos da folga de {} m dele para '{}': afaste a esteira ou defina `access`",
                            o.name,
                            p[0],
                            p[1],
                            fmt_m(c),
                            t.name
                        ));
                    }
                }
            }
            if unreachable.len() == served.len() {
                out.push(format!(
                    "'{}' não tem caminho livre até nenhum dos objetos que atende ({}): está cercado \
                     ou as passagens são mais estreitas que o dobro da folga ({} m); as viagens sairão em linha reta",
                    t.name,
                    unreachable.join(", "),
                    fmt_m(c)
                ));
            } else {
                for name in unreachable {
                    out.push(format!(
                        "'{}' não alcança o ponto de acesso de '{name}' sem cruzar obstáculos \
                         (folga {} m): a viagem sairá em linha reta; abra uma passagem ou mova o objeto",
                        t.name,
                        fmt_m(c)
                    ));
                }
            }
        }
        let Some(net) = &self.network else { return };
        if !(uses_network || net.generated.is_some()) {
            return;
        }
        let bare = NavGrid::new(&self.nav_obstacles(), 0.0, &[], &GridOptions::default());
        let pos: HashMap<&str, [f64; 3]> = net
            .nodes
            .iter()
            .map(|n| (n.name.as_str(), n.position))
            .collect();
        for e in &net.edges {
            let (Some(a), Some(b)) = (pos.get(e.from.as_str()), pos.get(e.to.as_str())) else {
                continue;
            };
            let hit = bare.crossed([a[0], a[1]], [b[0], b[1]]);
            if !hit.is_empty() {
                out.push(format!(
                    "o trecho {} → {} da rede cruza '{}': mova os nós ou gere a rede de novo",
                    e.from,
                    e.to,
                    hit.join("', '")
                ));
            }
        }
        if let Some(info) = &net.generated {
            if self.nav_grid(info.clearance).fingerprint() != info.fingerprint {
                out.push(
                    "a rede foi gerada com outros obstáculos (objetos, planta ou folga mudaram): \
                     gere de novo com `utsim rede gerar`"
                        .to_string(),
                );
            }
        }
    }
}
