//! Corredores de circulação e faixas de segurança (plano, seção 6): o formato dos corredores
//! e do perfil de marcação, a rede de caminhos gerada pelas linhas centrais, as ligações dos
//! pontos de acesso, os cruzamentos pedestre × veículo, as larguras mínimas por perfil e as
//! faixas no piso (polígonos com cor e padrão) para o desenho.
//!
//! Tudo é função pura dos dados: a mesma entrada dá sempre a mesma rede, os mesmos nomes de
//! nós e os mesmos polígonos.
//!
//! **Normas.** As larguras e cores dos perfis seguem a prática de mercado e as normas
//! citadas no manual como referência; o programa não certifica conformidade. As fórmulas do
//! corredor de empilhadeira (`L + 1,0 m` mão única, `2L + 1,2 m` mão dupla) são prática de
//! mercado, não norma.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use utsim_core::Router;
use utsim_objects::network::{EdgeDef, NetworkDef, NetworkRouter, NodeDef};

use crate::body::{Obstacle, Role};
use crate::geom::{Pt, dist, point_polygon, point_segment, polyline_length, segment_polygon};

/// Tolerância das junções (m): pontas e cruzamentos a menos disso viram o mesmo nó, e uma
/// ponta a menos disso de outro eixo encaixa nele (junção em T).
pub const JOIN_TOL: f64 = 0.01;

/// Diferença de altura (m) a partir da qual dois corredores que se cruzam na planta estão
/// em níveis diferentes (uma passarela sobre um corredor): não se juntam ali.
pub const LEVEL_TOL: f64 = 0.3;

/// Altura livre (m) acima da qual um corredor passa por cima dos obstáculos do piso (uma
/// passarela sobre as máquinas).
pub const OVERHEAD: f64 = 2.2;

/// Quem pode usar um corredor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AisleKind {
    /// Só pessoas.
    Pedestrian,
    /// Empilhadeiras, AGVs e outros veículos.
    Forklift,
    /// Pessoas e veículos.
    Mixed,
}

/// Classe de quem anda: pessoa ou veículo (empilhadeira, AGV, outro).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum User {
    Person,
    Vehicle,
}

impl User {
    /// Pessoa para o papel `Person`; veículo para os outros.
    pub fn of(role: Role) -> User {
        match role {
            Role::Person => User::Person,
            _ => User::Vehicle,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            User::Person => "pessoa",
            User::Vehicle => "veículo",
        }
    }

    /// Corredores que servem a esta classe, para mensagens.
    pub fn aisles_label(self) -> &'static str {
        match self {
            User::Person => "de pedestres ou misto",
            User::Vehicle => "de empilhadeiras ou misto",
        }
    }
}

impl AisleKind {
    /// Pessoas em corredores de pedestres e mistos; veículos nos de empilhadeiras e mistos.
    pub fn allows(self, user: User) -> bool {
        matches!(
            (self, user),
            (AisleKind::Mixed, _)
                | (AisleKind::Pedestrian, User::Person)
                | (AisleKind::Forklift, User::Vehicle)
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            AisleKind::Pedestrian => "pedestres",
            AisleKind::Forklift => "empilhadeiras",
            AisleKind::Mixed => "misto",
        }
    }
}

fn yes() -> bool {
    true
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Corredor desenhado no piso: linha central (polilinha) com largura.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aisle {
    pub name: String,
    pub kind: AisleKind,
    /// Linha central, em metros no piso (ao menos 2 pontos).
    pub points: Vec<Pt>,
    /// Largura total (m), de borda a borda.
    pub width: f64,
    /// Mão dupla (padrão). Em mão única, só no sentido dos pontos.
    #[serde(default = "yes")]
    pub two_way: bool,
    /// Passagem secundária de pedestres: o mínimo é o de passagem secundária (0,90 m no
    /// perfil Brasil) em vez do da via principal (1,20 m).
    #[serde(default, skip_serializing_if = "is_false")]
    pub secondary: bool,
    /// Perfil de marcação só deste corredor (ausente: o do modelo).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    /// Altura de cada ponto (m), na ordem de `points`; vazio: tudo no piso. Um caminho
    /// fica no piso, salvo numa rampa (até um mezanino, por exemplo).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heights: Vec<f64>,
    /// Raio da curva em cada ponto (m), na ordem de `points`; vazio ou 0: quina viva.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub radii: Vec<f64>,
    /// Escada: os trechos com desnível são degraus (só pedestres, mais devagar; ver
    /// `utsim_objects::ramps`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub stair: bool,
}

impl Aisle {
    /// Comprimento real: a linha central com as curvas e as rampas, em 3D.
    pub fn length(&self) -> f64 {
        if self.is_plain() {
            return polyline_length(&self.points);
        }
        crate::path3d::length3(&self.centerline())
    }

    /// Sem alturas nem curvas: a linha central é a dos pontos (os modelos antigos).
    pub fn is_plain(&self) -> bool {
        self.heights.iter().all(|h| *h == 0.0) && self.radii.iter().all(|r| *r <= 0.0)
    }

    /// Os pontos como nós de trajeto (com altura e raio).
    pub fn nodes(&self) -> Vec<crate::path3d::PathNode> {
        self.points
            .iter()
            .enumerate()
            .map(|(i, p)| crate::path3d::PathNode {
                at: [p[0], p[1], self.heights.get(i).copied().unwrap_or(0.0)],
                radius: self.radii.get(i).copied().filter(|r| *r > 0.0),
            })
            .collect()
    }

    /// Linha central real, em 3D: os pontos com as curvas amostradas e as alturas.
    pub fn centerline(&self) -> Vec<crate::path3d::P3> {
        if self.is_plain() {
            return self.points.iter().map(|p| [p[0], p[1], 0.0]).collect();
        }
        crate::path3d::sample(&self.nodes())
    }

    /// A linha central real em planta (as faixas no piso seguem as curvas).
    pub fn plan(&self) -> Vec<Pt> {
        if self.is_plain() {
            return self.points.clone();
        }
        crate::path3d::plan_line(&self.centerline())
    }

    /// Põe o ponto `p` (à altura `z`, sem curva) na posição `i`, mantendo as alturas e os
    /// raios alinhados com os pontos.
    pub fn insert_point(&mut self, i: usize, p: Pt, z: f64) {
        let i = i.min(self.points.len());
        if !self.heights.is_empty() || z != 0.0 {
            self.heights.resize(self.points.len(), 0.0);
            self.heights.insert(i, z);
        }
        if !self.radii.is_empty() {
            self.radii.resize(self.points.len(), 0.0);
            self.radii.insert(i, 0.0);
        }
        self.points.insert(i, p);
    }

    /// Tira o ponto `i` (com a sua altura e o seu raio).
    pub fn remove_point(&mut self, i: usize) {
        if i >= self.points.len() {
            return;
        }
        self.points.remove(i);
        if i < self.heights.len() {
            self.heights.remove(i);
        }
        if i < self.radii.len() {
            self.radii.remove(i);
        }
    }

    /// Muda a altura do ponto `i` (todas zero: a lista some e o arquivo fica como antes).
    pub fn set_height(&mut self, i: usize, z: f64) {
        if i >= self.points.len() {
            return;
        }
        self.heights.resize(self.points.len(), 0.0);
        self.heights[i] = z;
        if self.heights.iter().all(|h| *h == 0.0) {
            self.heights.clear();
        }
    }

    /// Muda o raio da curva no ponto `i` (0: quina viva).
    pub fn set_radius(&mut self, i: usize, r: f64) {
        if i >= self.points.len() {
            return;
        }
        self.radii.resize(self.points.len(), 0.0);
        self.radii[i] = r.max(0.0);
        if self.radii.iter().all(|r| *r <= 0.0) {
            self.radii.clear();
        }
    }

    /// Inverte o sentido (pontos, alturas e raios juntos).
    pub fn reverse(&mut self) {
        let n = self.points.len();
        self.heights
            .resize(if self.heights.is_empty() { 0 } else { n }, 0.0);
        self.radii
            .resize(if self.radii.is_empty() { 0 } else { n }, 0.0);
        self.points.reverse();
        self.heights.reverse();
        self.radii.reverse();
    }

    /// Mensagem de erro, se o corredor não serve.
    pub fn problem(&self) -> Option<String> {
        if self.points.len() < 2 {
            return Some(format!(
                "corredor '{}': a linha central precisa de ao menos 2 pontos",
                self.name
            ));
        }
        if !self.points.iter().flatten().all(|v| v.is_finite()) {
            return Some(format!(
                "corredor '{}': as coordenadas precisam ser finitas",
                self.name
            ));
        }
        if self.heights.len() > self.points.len() || self.radii.len() > self.points.len() {
            return Some(format!(
                "corredor '{}': há mais alturas ou raios que pontos",
                self.name
            ));
        }
        if !self
            .heights
            .iter()
            .chain(&self.radii)
            .all(|v| v.is_finite())
            || self.radii.iter().any(|r| *r < 0.0)
        {
            return Some(format!(
                "corredor '{}': alturas e raios precisam ser finitos (raio >= 0)",
                self.name
            ));
        }
        if self.stair && self.kind != AisleKind::Pedestrian {
            return Some(format!(
                "corredor '{}': escada só serve a pedestres (kind: Pedestrian)",
                self.name
            ));
        }
        if !(self.width.is_finite() && self.width > 0.0) {
            return Some(format!(
                "corredor '{}': width deve ser um número > 0 (metros)",
                self.name
            ));
        }
        if self.length() <= 0.0 {
            return Some(format!("corredor '{}': comprimento zero", self.name));
        }
        if let Some(Profile::Custom(spec)) = &self.profile {
            if let Some(p) = spec.problem() {
                return Some(format!("corredor '{}': {p}", self.name));
            }
        }
        None
    }
}

/// Perfil de cores e larguras da marcação do piso.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Profile {
    /// Faixa amarela contínua, setas amarelas, travessia zebrada amarelo e preto (padrão).
    #[default]
    Brasil,
    /// Como o Brasil, com as faixas e setas brancas (redação original da NR-26).
    Branco,
    /// Valores próprios; os campos ausentes valem os do perfil Brasil.
    Custom(ProfileSpec),
}

impl Profile {
    pub fn spec(&self) -> ProfileSpec {
        match self {
            Profile::Brasil => ProfileSpec::default(),
            Profile::Branco => ProfileSpec {
                line_color: WHITE.into(),
                arrow_color: WHITE.into(),
                ..ProfileSpec::default()
            },
            Profile::Custom(s) => s.clone(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Profile::Brasil => "Brasil",
            Profile::Branco => "Branco",
            Profile::Custom(_) => "personalizado",
        }
    }
}

/// Amarelo de segurança usado nos perfis (aproximação em sRGB).
pub const YELLOW: &str = "#F2C200";
pub const BLACK: &str = "#1A1A1A";
pub const WHITE: &str = "#FFFFFF";

/// Cores (`#RRGGBB`) e larguras de um perfil.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileSpec {
    /// Faixas de delimitação dos corredores.
    pub line_color: String,
    /// Setas de sentido dos corredores de mão única.
    pub arrow_color: String,
    /// As duas cores alternadas da travessia zebrada.
    pub zebra_colors: (String, String),
    /// Largura de cada listra da zebra (m).
    pub stripe: f64,
    /// Distância entre setas de sentido (m).
    pub arrow_spacing: f64,
    /// Largura mínima do corredor de pedestres, via principal (m).
    pub pedestrian_min: f64,
    /// Largura mínima da passagem secundária de pedestres (m).
    pub pedestrian_secondary_min: f64,
    /// Corredor de veículo de mão única: `L + one_way_margin` (prática de mercado).
    pub one_way_margin: f64,
    /// Corredor de veículo de mão dupla: `2L + two_way_margin` (prática de mercado).
    pub two_way_margin: f64,
}

impl Default for ProfileSpec {
    fn default() -> Self {
        ProfileSpec {
            line_color: YELLOW.into(),
            arrow_color: YELLOW.into(),
            zebra_colors: (YELLOW.into(), BLACK.into()),
            stripe: 0.40,
            arrow_spacing: 10.0,
            pedestrian_min: 1.20,
            pedestrian_secondary_min: 0.90,
            one_way_margin: 1.0,
            two_way_margin: 1.2,
        }
    }
}

fn hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit())
}

impl ProfileSpec {
    pub fn problem(&self) -> Option<String> {
        for c in [
            &self.line_color,
            &self.arrow_color,
            &self.zebra_colors.0,
            &self.zebra_colors.1,
        ] {
            if !hex_color(c) {
                return Some(format!("cor '{c}' inválida (use #RRGGBB)"));
            }
        }
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        if !(ok(self.stripe)
            && self.stripe > 0.0
            && ok(self.arrow_spacing)
            && self.arrow_spacing > 0.0)
        {
            return Some("stripe e arrow_spacing devem ser números > 0".into());
        }
        if ![
            self.pedestrian_min,
            self.pedestrian_secondary_min,
            self.one_way_margin,
            self.two_way_margin,
        ]
        .into_iter()
        .all(ok)
        {
            return Some("larguras mínimas e margens devem ser números >= 0".into());
        }
        None
    }

    /// Largura mínima de um corredor, com `vehicle_width` = `L`, a maior largura entre o
    /// veículo e a carga. Misto: a exigência do veículo mais a do pedestre.
    pub fn min_width(
        &self,
        kind: AisleKind,
        two_way: bool,
        secondary: bool,
        vehicle_width: f64,
    ) -> f64 {
        let person = if secondary {
            self.pedestrian_secondary_min
        } else {
            self.pedestrian_min
        };
        let vehicle = if two_way {
            2.0 * vehicle_width + self.two_way_margin
        } else {
            vehicle_width + self.one_way_margin
        };
        match kind {
            AisleKind::Pedestrian => person,
            AisleKind::Forklift => vehicle,
            AisleKind::Mixed => vehicle + person,
        }
    }

    /// A regra da largura, para mensagens: `2L + 1,2 m`.
    pub fn rule(&self, kind: AisleKind, two_way: bool, secondary: bool) -> String {
        let m = |v: f64| format!("{v:.2}").replace('.', ",");
        let vehicle = if two_way {
            format!("2L + {} m", m(self.two_way_margin))
        } else {
            format!("L + {} m", m(self.one_way_margin))
        };
        let person = if secondary {
            self.pedestrian_secondary_min
        } else {
            self.pedestrian_min
        };
        match kind {
            AisleKind::Pedestrian => format!("{} m", m(person)),
            AisleKind::Forklift => vehicle,
            AisleKind::Mixed => format!("{vehicle} + {} m", m(person)),
        }
    }
}

fn default_line() -> f64 {
    0.10
}

fn default_stop() -> f64 {
    2.0
}

/// Marcação do piso do modelo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloorMarking {
    #[serde(default)]
    pub profile: Profile,
    /// Largura da faixa de delimitação (m); a prática usual vai de 0,05 a 0,15.
    #[serde(default = "default_line")]
    pub line_width: f64,
    /// Parada obrigatória do veículo antes de um cruzamento com pedestres (s).
    #[serde(default = "default_stop")]
    pub crossing_stop: f64,
    /// Cor do chão na cena 3D (`"#rrggbb"`; ausente, o cinza de sempre). Nunca lida pela
    /// simulação.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "cor_do_chao"
    )]
    pub floor_color: Option<String>,
}

impl Default for FloorMarking {
    fn default() -> Self {
        FloorMarking {
            profile: Profile::Brasil,
            line_width: default_line(),
            crossing_stop: default_stop(),
            floor_color: None,
        }
    }
}

impl FloorMarking {
    pub fn problem(&self) -> Option<String> {
        if !(self.line_width.is_finite() && self.line_width > 0.0) {
            return Some("floor_marking: line_width deve ser um número > 0 (metros)".into());
        }
        if !(self.crossing_stop.is_finite() && self.crossing_stop >= 0.0) {
            return Some("floor_marking: crossing_stop deve ser um número >= 0 (segundos)".into());
        }
        if let Some(c) = &self.floor_color {
            let h = c.trim().trim_start_matches('#');
            if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(format!(
                    "floor_marking: floor_color deve ser #rrggbb, não {c}"
                ));
            }
        }
        if let Profile::Custom(spec) = &self.profile {
            if let Some(p) = spec.problem() {
                return Some(format!("floor_marking: {p}"));
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------------------
// Rede gerada pelos corredores
// ---------------------------------------------------------------------------------------

/// O que um trecho da rede gerada é.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeUse {
    /// Trecho de corredor, com o tipo dele.
    Aisle(AisleKind),
    /// Ligação de um ponto de acesso ao corredor, só para a classe dada.
    Connector(User),
}

impl EdgeUse {
    pub fn allows(self, user: User) -> bool {
        match self {
            EdgeUse::Aisle(k) => k.allows(user),
            EdgeUse::Connector(u) => u == user,
        }
    }
}

/// Dados de cada trecho, na ordem de `NetworkDef::edges`.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeInfo {
    pub usage: EdgeUse,
    /// Corredor de origem (ligações: o nome do ponto ligado).
    pub aisle: String,
}

/// Cruzamento pedestre × veículo: nó onde se encontram corredores de tipos diferentes.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    /// `corredor_A x passeio_1`.
    pub name: String,
    pub node: String,
    pub position: Pt,
    /// Corredores que passam pelo nó, na ordem do modelo.
    pub aisles: Vec<String>,
    /// Meia extensão da zona de conflito ao longo do caminho (m): metade da maior largura.
    pub radius: f64,
}

/// Ligação de um ponto (de acesso ou posição de um recurso) ao corredor mais próximo.
#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub user: User,
    pub point: Pt,
    /// Nó da rede no ponto; `None` quando não há corredor da classe.
    pub node: Option<String>,
    /// Ponto do eixo onde a ligação chega.
    pub on: Option<Pt>,
    pub aisle: Option<String>,
    /// Obstáculos que a ligação mais curta possível cruza (vazio: ligação livre).
    pub crosses: Vec<String>,
}

/// Trecho de eixo que cruza um obstáculo e ficou fora da rede.
#[derive(Clone, Debug, PartialEq)]
pub struct Blocked {
    pub aisle: String,
    pub from: Pt,
    pub to: Pt,
    pub obstacles: Vec<String>,
}

/// Rede gerada pelos corredores.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AisleNet {
    pub network: NetworkDef,
    pub edges: Vec<EdgeInfo>,
    pub crossings: Vec<Crossing>,
    pub attachments: Vec<Attachment>,
    pub blocked: Vec<Blocked>,
}

fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross2(a: Pt, b: Pt) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Parâmetro `t` em `ab` do ponto de `ab` mais perto de `p`.
fn project(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = sub(b, a);
    let len2 = d[0] * d[0] + d[1] * d[1];
    if len2 <= 0.0 {
        return 0.0;
    }
    (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
}

/// Parâmetro em `ab` do cruzamento próprio com `cd` (retas não paralelas).
fn intersection(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<f64> {
    let (r, s) = (sub(b, a), sub(d, c));
    let den = cross2(r, s);
    if den.abs() < 1e-12 {
        return None;
    }
    let ac = sub(c, a);
    let t = cross2(ac, s) / den;
    let u = cross2(ac, r) / den;
    let (lr, ls) = (dist(a, b), dist(c, d));
    let (tt, tu) = (JOIN_TOL / lr.max(1e-12), JOIN_TOL / ls.max(1e-12));
    ((-tt..=1.0 + tt).contains(&t) && (-tu..=1.0 + tu).contains(&u)).then(|| t.clamp(0.0, 1.0))
}

struct Builder {
    nodes: Vec<NodeDef>,
    edges: Vec<EdgeDef>,
    info: Vec<EdgeInfo>,
}

impl Builder {
    fn pos(&self, i: usize) -> Pt {
        let p = self.nodes[i].position;
        [p[0], p[1]]
    }

    fn index(&self, name: &str) -> usize {
        self.nodes
            .iter()
            .position(|n| n.name == name)
            .expect("node exists")
    }

    /// Nó em `p` (o primeiro a menos de [`JOIN_TOL`], senão um novo `n<k>`).
    fn node(&mut self, p: Pt) -> usize {
        self.node_at(p, 0.0)
    }

    /// Nó em `p` à altura `z` (a de uma rampa): o que já existe ali no mesmo nível (a menos
    /// de [`LEVEL_TOL`]), senão um novo; em outro nível é outro nó (a passarela sobre o
    /// corredor).
    fn node_at(&mut self, p: Pt, z: f64) -> usize {
        if let Some(i) = (0..self.nodes.len()).find(|&i| {
            dist(self.pos(i), p) < JOIN_TOL && (self.nodes[i].position[2] - z).abs() < LEVEL_TOL
        }) {
            return i;
        }
        self.nodes.push(NodeDef {
            name: format!("n{}", self.nodes.len() + 1),
            position: [p[0], p[1], z],
        });
        self.nodes.len() - 1
    }

    fn edge(&mut self, a: usize, b: usize, two_way: bool, info: EdgeInfo) {
        self.edges.push(EdgeDef {
            from: self.nodes[a].name.clone(),
            to: self.nodes[b].name.clone(),
            two_way,
            capacity: None,
        });
        self.info.push(info);
    }
}

fn hits(a: Pt, b: Pt, obstacles: &[Obstacle]) -> Vec<String> {
    obstacles
        .iter()
        .filter(|o| o.polygon.len() >= 3 && segment_polygon(a, b, &o.polygon) <= 0.0)
        .map(|o| o.name.clone())
        .collect()
}

/// Rede dos corredores e ligações dos `points` (nome, posição, classe).
///
/// 1. Cada trecho da linha central é cortado nas pontas, nos cruzamentos com os outros
///    eixos e onde uma ponta de outro eixo encosta nele (junção em T, tolerância
///    [`JOIN_TOL`]); nós `n1`, `n2`... na ordem em que aparecem (corredores na ordem do
///    modelo, trechos na ordem dos pontos).
/// 2. Cada pedaço vira um trecho da rede com o sentido do corredor; os que cruzam um
///    obstáculo pelo eixo ficam de fora (`blocked`).
/// 3. Num nó onde se encontram corredores de tipos diferentes nasce um cruzamento.
/// 4. Cada ponto liga-se ao ponto mais próximo de um trecho compatível com a sua classe
///    cuja ligação reta não cruza obstáculo (senão, ao mais próximo, com os obstáculos
///    anotados em `crosses`); o trecho é cortado ali e a ligação só vale para a classe.
///    Pontos repetidos (mesma classe, a menos de [`JOIN_TOL`]) ligam-se uma vez.
pub fn aisle_network(
    aisles: &[Aisle],
    obstacles: &[Obstacle],
    points: &[(String, Pt, User)],
) -> AisleNet {
    let valid: Vec<&Aisle> = aisles.iter().filter(|a| a.problem().is_none()).collect();
    // The real centre lines (curves sampled, ramps at their heights).
    let lines: Vec<Vec<crate::path3d::P3>> = valid.iter().map(|a| a.centerline()).collect();
    let segs3: Vec<(usize, crate::path3d::P3, crate::path3d::P3)> = lines
        .iter()
        .enumerate()
        .flat_map(|(i, l)| l.windows(2).map(move |w| (i, w[0], w[1])))
        .filter(|(_, a, b)| dist([a[0], a[1]], [b[0], b[1]]) > 0.0)
        .collect();
    let segs: Vec<(usize, Pt, Pt)> = segs3
        .iter()
        .map(|(i, a, b)| (*i, [a[0], a[1]], [b[0], b[1]]))
        .collect();
    let mut b = Builder {
        nodes: Vec::new(),
        edges: Vec::new(),
        info: Vec::new(),
    };
    let mut blocked = Vec::new();
    for (si, &(ai, p, q)) in segs.iter().enumerate() {
        let mut ts = vec![0.0, 1.0];
        let z_here = |t: f64| segs3[si].1[2] + (segs3[si].2[2] - segs3[si].1[2]) * t;
        for (sj, &(_, c, d)) in segs.iter().enumerate() {
            if si == sj {
                continue;
            }
            // Height of the other line at its parameter `u`.
            let z_there = |u: f64| segs3[sj].1[2] + (segs3[sj].2[2] - segs3[sj].1[2]) * u;
            if let Some(t) = intersection(p, q, c, d) {
                let x = lerp(p, q, t);
                // Grade separated: a walkway over an aisle does not meet it.
                if (z_here(t) - z_there(project(x, c, d))).abs() < LEVEL_TOL {
                    ts.push(t);
                }
            }
            for (e, u) in [(c, 0.0), (d, 1.0)] {
                if point_segment(e, p, q) < JOIN_TOL {
                    let t = project(e, p, q);
                    if (z_here(t) - z_there(u)).abs() < LEVEL_TOL {
                        ts.push(t);
                    }
                }
            }
        }
        // High enough, the line passes over the obstacles of the floor.
        let overhead = segs3[si].1[2].min(segs3[si].2[2]) >= OVERHEAD;
        // Cut where the centre line enters and leaves each obstacle: every piece is then
        // wholly inside or wholly outside it.
        for o in obstacles
            .iter()
            .filter(|o| o.polygon.len() >= 3 && !overhead)
        {
            let n = o.polygon.len();
            for k in 0..n {
                if let Some(t) = intersection(p, q, o.polygon[k], o.polygon[(k + 1) % n]) {
                    ts.push(t);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        let aisle = valid[ai];
        let (za, zb) = (segs3[si].1[2], segs3[si].2[2]);
        let mut prev: Option<(usize, Pt)> = None;
        for t in ts {
            let x = lerp(p, q, t);
            let n = b.node_at(x, za + (zb - za) * t);
            if let Some((m, y)) = prev {
                if m != n {
                    let mid = lerp(y, x, 0.5);
                    let crossed: Vec<String> = obstacles
                        .iter()
                        .filter(|o| {
                            !overhead
                                && o.polygon.len() >= 3
                                && point_polygon(mid, &o.polygon) <= 0.0
                        })
                        .map(|o| o.name.clone())
                        .collect();
                    if crossed.is_empty() {
                        b.edge(
                            m,
                            n,
                            aisle.two_way,
                            EdgeInfo {
                                usage: EdgeUse::Aisle(aisle.kind),
                                aisle: aisle.name.clone(),
                            },
                        );
                    } else {
                        blocked.push(Blocked {
                            aisle: aisle.name.clone(),
                            from: y,
                            to: x,
                            obstacles: crossed,
                        });
                    }
                }
            }
            if prev.is_none_or(|(m, _)| m != n) {
                prev = Some((n, x));
            }
        }
    }

    // Crossings: nodes shared by aisles of different kinds.
    let mut crossings = Vec::new();
    for i in 0..b.nodes.len() {
        let name = b.nodes[i].name.clone();
        let mut on: BTreeSet<usize> = BTreeSet::new();
        for (e, info) in b.edges.iter().zip(&b.info) {
            if e.from == name || e.to == name {
                if let Some(k) = valid.iter().position(|a| a.name == info.aisle) {
                    on.insert(k);
                }
            }
        }
        let kinds: BTreeSet<AisleKind> = on.iter().map(|&k| valid[k].kind).collect();
        if kinds.len() < 2 {
            continue;
        }
        let names: Vec<String> = on.iter().map(|&k| valid[k].name.clone()).collect();
        let radius = on.iter().map(|&k| valid[k].width).fold(0.0, f64::max) / 2.0;
        crossings.push(Crossing {
            name: names.join(" x "),
            node: name,
            position: b.pos(i),
            aisles: names,
            radius,
        });
    }

    // Attachments.
    let mut attachments: Vec<Attachment> = Vec::new();
    for (name, p, user) in points {
        if attachments
            .iter()
            .any(|a| a.user == *user && dist(a.point, *p) < JOIN_TOL)
        {
            continue;
        }
        let mut cands: Vec<(f64, usize, f64, bool)> = Vec::new();
        for (e, info) in b.info.iter().enumerate() {
            if !matches!(info.usage, EdgeUse::Aisle(k) if k.allows(*user)) {
                continue;
            }
            let (u, v) = (b.index(&b.edges[e].from), b.index(&b.edges[e].to));
            let (pu, pv) = (b.pos(u), b.pos(v));
            let t = project(*p, pu, pv);
            // Objects stand on the floor: they reach a walkway above them only by its
            // ramp, so the lines at their level come first.
            let z = b.nodes[u].position[2] + (b.nodes[v].position[2] - b.nodes[u].position[2]) * t;
            cands.push((dist(*p, lerp(pu, pv, t)), e, t, z.abs() >= LEVEL_TOL));
        }
        if cands.iter().any(|c| !c.3) {
            cands.retain(|c| !c.3);
        }
        let mut cands: Vec<(f64, usize, f64)> =
            cands.into_iter().map(|c| (c.0, c.1, c.2)).collect();
        cands.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        let Some(&(_, first_e, first_t)) = cands.first() else {
            attachments.push(Attachment {
                name: name.clone(),
                user: *user,
                point: *p,
                node: None,
                on: None,
                aisle: None,
                crosses: Vec::new(),
            });
            continue;
        };
        let pick = |e: usize, t: f64, b: &Builder| {
            let (u, v) = (b.index(&b.edges[e].from), b.index(&b.edges[e].to));
            lerp(b.pos(u), b.pos(v), t)
        };
        let clean = cands
            .iter()
            .find(|(d, e, t)| *d < JOIN_TOL || hits(*p, pick(*e, *t, &b), obstacles).is_empty());
        let (e, t, crosses) = match clean {
            Some(&(_, e, t)) => (e, t, Vec::new()),
            None => (
                first_e,
                first_t,
                hits(*p, pick(first_e, first_t, &b), obstacles),
            ),
        };
        let q = pick(e, t, &b);
        let aisle = b.info[e].aisle.clone();
        let (u, v) = (b.index(&b.edges[e].from), b.index(&b.edges[e].to));
        let on = if dist(q, b.pos(u)) < JOIN_TOL {
            u
        } else if dist(q, b.pos(v)) < JOIN_TOL {
            v
        } else {
            // Split the edge at q: u → q replaces it, q → v goes to the end. On a ramp
            // the new node is at the height the edge has there.
            let (zu, zv) = (b.nodes[u].position[2], b.nodes[v].position[2]);
            let n = b.node_at(q, zu + (zv - zu) * t_of(q, b.pos(u), b.pos(v)));
            let mut tail = b.edges[e].clone();
            tail.from = b.nodes[n].name.clone();
            b.edges[e].to = b.nodes[n].name.clone();
            let info = b.info[e].clone();
            b.edges.push(tail);
            b.info.push(info);
            n
        };
        let at = if dist(*p, b.pos(on)) < JOIN_TOL {
            on
        } else {
            let n = b.node(*p);
            b.edge(
                n,
                on,
                true,
                EdgeInfo {
                    usage: EdgeUse::Connector(*user),
                    aisle: name.clone(),
                },
            );
            n
        };
        attachments.push(Attachment {
            name: name.clone(),
            user: *user,
            point: *p,
            node: Some(b.nodes[at].name.clone()),
            on: Some(b.pos(on)),
            aisle: Some(aisle),
            crosses,
        });
    }

    AisleNet {
        network: NetworkDef {
            nodes: b.nodes,
            edges: b.edges,
            generated: None,
        },
        edges: b.info,
        crossings,
        attachments,
        blocked,
    }
}

/// Fração de `q` no segmento `a`–`b` (em planta).
fn t_of(q: Pt, a: Pt, b: Pt) -> f64 {
    let l = dist(a, b);
    if l <= 0.0 {
        0.0
    } else {
        (dist(a, q) / l).clamp(0.0, 1.0)
    }
}

impl AisleNet {
    /// Sub-rede de uma classe: os trechos que ela pode usar e os nós deles.
    pub fn network_for(&self, user: User) -> NetworkDef {
        let edges: Vec<EdgeDef> = self
            .network
            .edges
            .iter()
            .zip(&self.edges)
            .filter(|(_, i)| i.usage.allows(user))
            .map(|(e, _)| e.clone())
            .collect();
        let used: BTreeSet<&str> = edges
            .iter()
            .flat_map(|e| [e.from.as_str(), e.to.as_str()])
            .collect();
        NetworkDef {
            nodes: self
                .network
                .nodes
                .iter()
                .filter(|n| used.contains(n.name.as_str()))
                .cloned()
                .collect(),
            edges,
            generated: None,
        }
    }

    /// Ligação de um ponto para uma classe.
    pub fn attachment(&self, p: Pt, user: User) -> Option<&Attachment> {
        self.attachments
            .iter()
            .find(|a| a.user == user && dist(a.point, p) < JOIN_TOL)
    }
}

/// Modo `Aisles`: o destino que é um objeto vira o ponto de acesso; o caminho é o menor
/// pela sub-rede da classe (ligações incluídas), entrando e saindo pelo nó mais próximo da
/// origem e do destino (os pontos conhecidos têm nó próprio, ligado ao corredor).
pub struct AislesRouter {
    network: NetworkRouter,
    /// Posição do objeto → ponto de acesso (o primeiro vence).
    targets: Vec<([f64; 3], Pt)>,
}

impl AislesRouter {
    pub fn new(network: &NetworkDef, targets: Vec<([f64; 3], Pt)>) -> Self {
        AislesRouter {
            network: NetworkRouter::new(network),
            targets,
        }
    }

    pub fn target(&self, to: [f64; 3]) -> [f64; 3] {
        self.targets
            .iter()
            .find(|(p, _)| p.map(f64::to_bits) == to.map(f64::to_bits))
            .map_or(to, |(_, a)| [a[0], a[1], to[2]])
    }

    pub fn network(&self) -> &NetworkRouter {
        &self.network
    }
}

impl Router for AislesRouter {
    fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
        self.network.route(from, self.target(to))
    }
}

// ---------------------------------------------------------------------------------------
// Faixas no piso (dados para o desenho)
// ---------------------------------------------------------------------------------------

/// Padrão de uma forma da marcação.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Pattern {
    /// Faixa contínua de delimitação (um quadrilátero por trecho, com junção em esquadria).
    Continuous,
    /// Listra da travessia zebrada.
    Zebra,
    /// Seta de sentido (triângulo).
    Arrow,
}

/// Uma forma convexa da marcação (quadrilátero ou triângulo, sentido anti-horário ou
/// horário conforme o lado), com a cor `#RRGGBB`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MarkShape {
    /// Corredor (ou cruzamento) de origem.
    pub source: String,
    pub pattern: Pattern,
    pub color: String,
    pub polygon: Vec<Pt>,
}

/// Esquadria limitada: a ponta da junção vai no máximo a esta razão da distância.
pub const MITER_LIMIT: f64 = 2.0;

fn unit(a: Pt, b: Pt) -> Pt {
    let d = dist(a, b);
    if d <= 0.0 {
        [1.0, 0.0]
    } else {
        [(b[0] - a[0]) / d, (b[1] - a[1]) / d]
    }
}

fn normal(u: Pt) -> Pt {
    [-u[1], u[0]]
}

/// Polilinha deslocada de `off` para a esquerda (negativo: direita), com esquadria limitada.
pub fn offset_polyline(points: &[Pt], off: f64) -> Vec<Pt> {
    let n = points.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let before = (i > 0).then(|| normal(unit(points[i - 1], points[i])));
        let after = (i + 1 < n).then(|| normal(unit(points[i], points[i + 1])));
        let p = points[i];
        let shifted = match (before, after) {
            (Some(a), Some(b)) => {
                let m = [a[0] + b[0], a[1] + b[1]];
                let ml = (m[0] * m[0] + m[1] * m[1]).sqrt();
                if ml < 1e-9 {
                    [p[0] + a[0] * off, p[1] + a[1] * off]
                } else {
                    let m = [m[0] / ml, m[1] / ml];
                    let cos = m[0] * a[0] + m[1] * a[1];
                    let len =
                        (off / cos.max(1e-9)).abs().min(off.abs() * MITER_LIMIT) * off.signum();
                    [p[0] + m[0] * len, p[1] + m[1] * len]
                }
            }
            (Some(a), None) | (None, Some(a)) => [p[0] + a[0] * off, p[1] + a[1] * off],
            (None, None) => p,
        };
        out.push(shifted);
    }
    out
}

/// Área de um polígono simples (fórmula do laço, sempre positiva).
pub fn polygon_area(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| cross2(poly[i], poly[(i + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.0
}

/// Direção do trecho do corredor que passa mais perto de `p`.
fn direction_at(aisle: &Aisle, p: Pt) -> Pt {
    let mut best = (f64::INFINITY, [1.0, 0.0]);
    for w in aisle.plan().windows(2) {
        let d = point_segment(p, w[0], w[1]);
        if d < best.0 {
            best = (d, unit(w[0], w[1]));
        }
    }
    best.1
}

/// Faixas no piso de todos os corredores: as duas faixas contínuas de delimitação (por
/// dentro da largura), as setas dos corredores de mão única e as listras zebradas de cada
/// cruzamento, nas cores do perfil de cada corredor (ou do modelo).
///
/// A zebra fica sobre o corredor de veículos, alinhada ao de pedestres: listras de
/// `stripe` m paralelas ao corredor de veículos, do comprimento da largura do corredor de
/// pedestres, alternadas ao longo do corredor de pedestres por toda a largura do de
/// veículos. As faixas contínuas não são interrompidas sob a zebra (a zebra vem depois e
/// fica por cima).
pub fn floor_marks(aisles: &[Aisle], net: &AisleNet, marking: &FloorMarking) -> Vec<MarkShape> {
    let mut out = Vec::new();
    let lw = marking.line_width.max(0.0);
    let spec_of = |a: &Aisle| a.profile.as_ref().unwrap_or(&marking.profile).spec();
    for a in aisles.iter().filter(|a| a.problem().is_none()) {
        let spec = spec_of(a);
        let half = a.width / 2.0;
        let line = a.plan();
        for side in [1.0, -1.0] {
            let outer = offset_polyline(&line, side * half);
            let inner = offset_polyline(&line, side * (half - lw).max(0.0));
            for i in 0..line.len() - 1 {
                out.push(MarkShape {
                    source: a.name.clone(),
                    pattern: Pattern::Continuous,
                    color: spec.line_color.clone(),
                    polygon: vec![outer[i], outer[i + 1], inner[i + 1], inner[i]],
                });
            }
        }
        if !a.two_way {
            let len = a.length();
            let spacing = spec.arrow_spacing;
            let (al, aw) = ((a.width * 0.6).min(1.0), (a.width * 0.25).min(0.3));
            let mut s = if len < spacing {
                len / 2.0
            } else {
                spacing / 2.0
            };
            while s < len {
                // Point and direction at arc length s.
                let mut walked = 0.0;
                for w in line.windows(2) {
                    let l = dist(w[0], w[1]);
                    if l > 0.0 && walked + l >= s {
                        let u = unit(w[0], w[1]);
                        let c = lerp(w[0], w[1], (s - walked) / l);
                        let nrm = normal(u);
                        let tip = [c[0] + u[0] * al / 2.0, c[1] + u[1] * al / 2.0];
                        let base = [c[0] - u[0] * al / 2.0, c[1] - u[1] * al / 2.0];
                        out.push(MarkShape {
                            source: a.name.clone(),
                            pattern: Pattern::Arrow,
                            color: spec.arrow_color.clone(),
                            polygon: vec![
                                tip,
                                [base[0] + nrm[0] * aw, base[1] + nrm[1] * aw],
                                [base[0] - nrm[0] * aw, base[1] - nrm[1] * aw],
                            ],
                        });
                        break;
                    }
                    walked += l;
                }
                s += spacing;
            }
        }
    }
    for c in &net.crossings {
        let find = |k: AisleKind| {
            c.aisles
                .iter()
                .filter_map(|n| aisles.iter().find(|a| &a.name == n))
                .find(|a| a.kind == k)
        };
        let (Some(ped), Some(veh)) = (
            find(AisleKind::Pedestrian).or(find(AisleKind::Mixed)),
            find(AisleKind::Forklift).or(find(AisleKind::Mixed)),
        ) else {
            continue;
        };
        if ped.name == veh.name {
            continue;
        }
        let spec = spec_of(ped);
        let (dp, dv) = (direction_at(ped, c.position), direction_at(veh, c.position));
        let (along, across) = (veh.width, ped.width);
        let count = (along / spec.stripe).ceil().max(1.0) as usize;
        for k in 0..count {
            let s0 = -along / 2.0 + k as f64 * spec.stripe;
            let s1 = (s0 + spec.stripe).min(along / 2.0);
            let corner = |s: f64, t: f64| {
                [
                    c.position[0] + dp[0] * s + dv[0] * t,
                    c.position[1] + dp[1] * s + dv[1] * t,
                ]
            };
            out.push(MarkShape {
                source: c.name.clone(),
                pattern: Pattern::Zebra,
                color: if k % 2 == 0 {
                    spec.zebra_colors.0.clone()
                } else {
                    spec.zebra_colors.1.clone()
                },
                polygon: vec![
                    corner(s0, -across / 2.0),
                    corner(s1, -across / 2.0),
                    corner(s1, across / 2.0),
                    corner(s0, across / 2.0),
                ],
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::rectangle;
    use utsim_core::task::path_length;

    fn aisle(name: &str, kind: AisleKind, points: &[Pt], width: f64) -> Aisle {
        Aisle {
            name: name.into(),
            kind,
            points: points.to_vec(),
            width,
            two_way: true,
            secondary: false,
            profile: None,
            heights: Vec::new(),
            radii: Vec::new(),
            stair: false,
        }
    }

    fn cross() -> Vec<Aisle> {
        vec![
            aisle(
                "corredor_A",
                AisleKind::Forklift,
                &[[-10.0, 0.0], [10.0, 0.0]],
                3.6,
            ),
            aisle(
                "passeio_1",
                AisleKind::Pedestrian,
                &[[0.0, -10.0], [0.0, 10.0]],
                1.2,
            ),
        ]
    }

    #[test]
    fn a_walkway_over_an_aisle_does_not_meet_it() {
        let mut aisles = cross();
        // The pedestrian line goes up to 3 m before the forklift aisle and down after it.
        aisles[1].points = vec![[0.0, -10.0], [0.0, -4.0], [0.0, 4.0], [0.0, 10.0]];
        aisles[1].heights = vec![0.0, 3.0, 3.0, 0.0];
        let net = aisle_network(&aisles, &[], &[]);
        assert!(net.crossings.is_empty());
        // Two separate lines: 1 edge + 3 edges, no shared node.
        assert_eq!(net.network.edges.len(), 4);
        assert_eq!(net.network.nodes.len(), 6);
        // A machine under the walkway does not cut it.
        let machine = Obstacle {
            name: "maquina".into(),
            polygon: vec![[-1.0, 1.0], [1.0, 1.0], [1.0, 3.0], [-1.0, 3.0]],
        };
        let net = aisle_network(&aisles, &[machine], &[]);
        assert!(net.blocked.iter().all(|b| b.aisle != "passeio_1"));
    }

    #[test]
    fn a_cross_gives_five_nodes_four_edges_and_one_crossing() {
        let net = aisle_network(&cross(), &[], &[]);
        assert_eq!(net.network.nodes.len(), 5);
        assert_eq!(net.network.edges.len(), 4);
        assert_eq!(net.crossings.len(), 1);
        let c = &net.crossings[0];
        assert_eq!(c.name, "corredor_A x passeio_1");
        assert_eq!(c.position, [0.0, 0.0]);
        assert!((c.radius - 1.8).abs() < 1e-12);
        // Same input, same network.
        assert_eq!(net, aisle_network(&cross(), &[], &[]));
    }

    #[test]
    fn an_l_shaped_aisle_turns_at_its_corner() {
        let a = vec![aisle(
            "L",
            AisleKind::Pedestrian,
            &[[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]],
            1.2,
        )];
        let net = aisle_network(&a, &[], &[]);
        assert_eq!(net.network.nodes.len(), 3);
        assert_eq!(net.network.nodes[1].position, [10.0, 0.0, 0.0]);
        assert_eq!(net.network.edges.len(), 2);
        assert!(net.crossings.is_empty());
        let r = NetworkRouter::new(&net.network);
        let p = r.route([0.0, 0.0, 0.0], [10.0, 5.0, 0.0]);
        assert_eq!(path_length(&p), 15.0);
    }

    #[test]
    fn a_t_junction_within_tolerance_joins_the_aisles() {
        let a = vec![
            aisle("a", AisleKind::Mixed, &[[0.0, 0.0], [10.0, 0.0]], 4.0),
            aisle("b", AisleKind::Mixed, &[[5.0, 0.005], [5.0, 8.0]], 4.0),
        ];
        let net = aisle_network(&a, &[], &[]);
        assert_eq!(net.network.nodes.len(), 4);
        assert_eq!(net.network.edges.len(), 3);
        assert!(net.network.validate().is_empty());
        // Same kind: no crossing.
        assert!(net.crossings.is_empty());
    }

    #[test]
    fn classes_only_use_their_aisles_and_connectors() {
        let pts = vec![
            ("pessoa".to_string(), [-10.0, -3.0], User::Person),
            ("maq".to_string(), [-10.0, -3.0], User::Vehicle),
        ];
        let mut a = cross();
        a.push(aisle(
            "passeio_2",
            AisleKind::Pedestrian,
            &[[-10.0, -3.0], [0.0, -3.0]],
            1.2,
        ));
        let net = aisle_network(&a, &[], &pts);
        let people = net.network_for(User::Person);
        let vehicles = net.network_for(User::Vehicle);
        assert!(people.edges.iter().all(|e| {
            let i = net.network.edges.iter().position(|x| x == e).unwrap();
            net.edges[i].usage.allows(User::Person)
        }));
        // The vehicle attachment goes to the forklift aisle (3 m up), not the walkway.
        let v = net.attachment([-10.0, -3.0], User::Vehicle).unwrap();
        assert_eq!(v.aisle.as_deref(), Some("corredor_A"));
        assert_eq!(v.on, Some([-10.0, 0.0]));
        // The person stands on the walkway end: no connector needed.
        let p = net.attachment([-10.0, -3.0], User::Person).unwrap();
        assert_eq!(p.aisle.as_deref(), Some("passeio_2"));
        assert!(vehicles.validate().is_empty(), "{:?}", vehicles.validate());
    }

    #[test]
    fn an_aisle_through_a_wall_loses_that_stretch() {
        let wall = Obstacle {
            name: "parede".into(),
            polygon: rectangle([5.0, 0.0], [0.1, 3.0], 0.0),
        };
        let a = vec![aisle(
            "a",
            AisleKind::Forklift,
            &[[0.0, 0.0], [10.0, 0.0]],
            3.0,
        )];
        let net = aisle_network(&a, &[wall], &[]);
        // Only the 0.2 m inside the wall is left out; both sides stay (disconnected).
        assert_eq!(net.network.edges.len(), 2);
        assert_eq!(net.blocked.len(), 1);
        assert_eq!(net.blocked[0].obstacles, vec!["parede".to_string()]);
        assert!((dist(net.blocked[0].from, [4.9, 0.0])) < 1e-9);
        assert!((dist(net.blocked[0].to, [5.1, 0.0])) < 1e-9);
        assert!(!net.network.validate().is_empty());
    }

    #[test]
    fn attachment_avoids_obstacles_when_it_can() {
        // Two parallel aisles; the nearer one is behind a wall.
        let a = vec![
            aisle(
                "perto",
                AisleKind::Forklift,
                &[[0.0, 2.0], [10.0, 2.0]],
                3.0,
            ),
            aisle(
                "longe",
                AisleKind::Forklift,
                &[[0.0, -4.0], [10.0, -4.0]],
                3.0,
            ),
        ];
        let wall = Obstacle {
            name: "muro".into(),
            polygon: rectangle([5.0, 1.0], [3.0, 0.1], 0.0),
        };
        let pts = vec![("m".to_string(), [5.0, 0.0], User::Vehicle)];
        let net = aisle_network(&a, &[wall], &pts);
        let at = net.attachment([5.0, 0.0], User::Vehicle).unwrap();
        assert_eq!(at.aisle.as_deref(), Some("longe"));
        assert!(at.crosses.is_empty());
    }

    #[test]
    fn widths_follow_the_profile() {
        let s = Profile::Brasil.spec();
        // Two-way forklift aisle with L = 1.2: 2 × 1.2 + 1.2 = 3.6.
        assert!((s.min_width(AisleKind::Forklift, true, false, 1.2) - 3.6).abs() < 1e-12);
        assert!((s.min_width(AisleKind::Forklift, false, false, 1.2) - 2.2).abs() < 1e-12);
        assert_eq!(s.min_width(AisleKind::Pedestrian, true, false, 0.0), 1.2);
        assert_eq!(s.min_width(AisleKind::Pedestrian, true, true, 0.0), 0.9);
        assert!((s.min_width(AisleKind::Mixed, true, false, 1.2) - 4.8).abs() < 1e-12);
        assert_eq!(s.rule(AisleKind::Forklift, true, false), "2L + 1,20 m");
        assert_eq!(Profile::Branco.spec().line_color, WHITE);
        assert_eq!(Profile::Brasil.spec().line_color, YELLOW);
    }

    #[test]
    fn straight_line_area_is_length_times_width() {
        let a = vec![aisle(
            "a",
            AisleKind::Pedestrian,
            &[[0.0, 0.0], [20.0, 0.0]],
            1.2,
        )];
        let net = aisle_network(&a, &[], &[]);
        let marks = floor_marks(&a, &net, &FloorMarking::default());
        let lines: Vec<&MarkShape> = marks
            .iter()
            .filter(|m| m.pattern == Pattern::Continuous)
            .collect();
        assert_eq!(lines.len(), 2);
        for l in lines {
            assert!((polygon_area(&l.polygon) - 20.0 * 0.10).abs() < 1e-9);
            assert_eq!(l.color, YELLOW);
        }
        // Two-way: no arrows.
        assert!(marks.iter().all(|m| m.pattern != Pattern::Arrow));
    }

    #[test]
    fn miter_is_limited_at_sharp_angles() {
        // Almost a U-turn: the plain miter would shoot far away.
        let pts = [[0.0, 0.0], [10.0, 0.0], [0.0, 0.5]];
        let off = offset_polyline(&pts, 1.0);
        let d = dist(off[1], pts[1]);
        assert!(d <= 1.0 * MITER_LIMIT + 1e-9, "{d}");
        // A right angle keeps the exact miter (√2 < limit).
        let off = offset_polyline(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], 1.0);
        assert!((dist(off[1], [10.0, 0.0]) - 2f64.sqrt()).abs() < 1e-12);
        assert!((off[1][0] - 9.0).abs() < 1e-12 && (off[1][1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn crossings_get_zebra_and_one_way_aisles_get_arrows() {
        let mut a = cross();
        a[0].two_way = false;
        let net = aisle_network(&a, &[], &[]);
        let marks = floor_marks(&a, &net, &FloorMarking::default());
        let zebra: Vec<&MarkShape> = marks
            .iter()
            .filter(|m| m.pattern == Pattern::Zebra)
            .collect();
        // 3.6 m of forklift aisle in 0.40 m stripes: 9 stripes, alternating colours.
        assert_eq!(zebra.len(), 9);
        assert_eq!(zebra[0].color, YELLOW);
        assert_eq!(zebra[1].color, BLACK);
        let total: f64 = zebra.iter().map(|z| polygon_area(&z.polygon)).sum();
        assert!((total - 3.6 * 1.2).abs() < 1e-9);
        // 20 m one-way aisle, arrows every 10 m from 5 m: two arrows.
        assert_eq!(
            marks.iter().filter(|m| m.pattern == Pattern::Arrow).count(),
            2
        );
    }
}
