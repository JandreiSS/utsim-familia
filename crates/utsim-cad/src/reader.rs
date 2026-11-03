//! Leitor mínimo de DXF 2D em texto: LINE, LWPOLYLINE, POLYLINE/VERTEX (com bulge), CIRCLE,
//! ARC, TEXT, MTEXT e INSERT (blocos aninhados, com escala, rotação e espelhamento).
//!
//! Fora do escopo (ignorados em silêncio): ELLIPSE, SPLINE, HATCH, DIMENSION, entidades 3D,
//! atributos de bloco, OCS espelhado por extrusão (código 210/230) e DXF binário.

use std::collections::HashMap;

use crate::CadError;
use crate::geom::Pt;
use crate::plan::{Label, Layer, Plan, Polyline};

/// Unidade do desenho (valores de `$INSUNITS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    /// Sem conversão: as coordenadas ficam como no arquivo (fator 1). Serve à calibração.
    Drawing,
    Inches,
    Feet,
    Miles,
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
    Yards,
    Decimeters,
}

impl Units {
    /// Metros por unidade.
    pub fn meters(self) -> f64 {
        match self {
            Units::Drawing | Units::Meters => 1.0,
            Units::Inches => 0.0254,
            Units::Feet => 0.3048,
            Units::Miles => 1609.344,
            Units::Millimeters => 0.001,
            Units::Centimeters => 0.01,
            Units::Kilometers => 1000.0,
            Units::Yards => 0.9144,
            Units::Decimeters => 0.1,
        }
    }

    /// Código `$INSUNITS` do DXF para a unidade (None se não suportado ou "sem unidade").
    pub fn from_insunits(code: i32) -> Option<Units> {
        Some(match code {
            1 => Units::Inches,
            2 => Units::Feet,
            3 => Units::Miles,
            4 => Units::Millimeters,
            5 => Units::Centimeters,
            6 => Units::Meters,
            7 => Units::Kilometers,
            10 => Units::Yards,
            14 => Units::Decimeters,
            _ => return None,
        })
    }
}

/// Opções de leitura.
#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    /// Força a unidade (ignora `$INSUNITS`). `Some(Units::Drawing)` lê sem converter.
    pub units: Option<Units>,
    /// Unidade assumida quando o cabeçalho não traz uma (padrão: milímetros, o mais comum
    /// em plantas). O resultado marca `units_assumed`.
    pub default_units: Units,
    /// Lê TEXT/MTEXT como rótulos.
    pub labels: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions {
            units: None,
            default_units: Units::Millimeters,
            labels: true,
        }
    }
}

type Pairs = Vec<(i32, String)>;

#[derive(Debug, Default)]
struct RawEnt {
    kind: String,
    pairs: Pairs,
}

impl RawEnt {
    fn f(&self, code: i32) -> Option<f64> {
        self.pairs
            .iter()
            .find(|(c, _)| *c == code)
            .and_then(|(_, v)| v.trim().parse().ok())
    }
    fn fd(&self, code: i32, default: f64) -> f64 {
        self.f(code).unwrap_or(default)
    }
    fn s(&self, code: i32) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.as_str())
    }
    fn layer(&self) -> &str {
        self.s(8).unwrap_or("0")
    }
}

#[derive(Debug, Default)]
struct Block {
    base: Pt,
    ents: Vec<RawEnt>,
}

#[derive(Debug, Default)]
struct Dxf {
    insunits: Option<i32>,
    blocks: HashMap<String, Block>,
    entities: Vec<RawEnt>,
}

fn parse_pairs(text: &str) -> Result<Vec<(i32, &str)>, CadError> {
    if text.starts_with("AutoCAD Binary DXF") {
        return Err(CadError::Invalid(
            "DXF binário não é suportado; salve como DXF de texto (ASCII)".into(),
        ));
    }
    let mut lines = text.lines();
    let mut out = Vec::new();
    while let Some(c) = lines.next() {
        let c = c.trim();
        if c.is_empty() {
            continue;
        }
        let code: i32 = c
            .parse()
            .map_err(|_| CadError::Invalid(format!("código de grupo inesperado: {c:?}")))?;
        let v = lines
            .next()
            .ok_or_else(|| CadError::Invalid("arquivo termina no meio de um par".into()))?;
        out.push((code, v.trim_end_matches('\r')));
    }
    Ok(out)
}

fn parse_dxf(text: &str) -> Result<Dxf, CadError> {
    let pairs = parse_pairs(text)?;
    let mut dxf = Dxf::default();
    let mut i = 0;
    let n = pairs.len();
    let mut section = String::new();
    let mut seen_eof = false;
    let mut cur_block: Option<(String, Block)> = None;
    let mut cur: Option<RawEnt> = None;
    let mut header_var: Option<String> = None;

    fn flush(
        cur: &mut Option<RawEnt>,
        section: &str,
        cur_block: &mut Option<(String, Block)>,
        dxf: &mut Dxf,
    ) {
        if let Some(e) = cur.take() {
            match section {
                "ENTITIES" => dxf.entities.push(e),
                "BLOCKS" => match e.kind.as_str() {
                    "BLOCK" => {
                        let name = e.s(2).unwrap_or("").to_string();
                        let base = [e.fd(10, 0.0), e.fd(20, 0.0)];
                        *cur_block = Some((
                            name,
                            Block {
                                base,
                                ents: Vec::new(),
                            },
                        ));
                    }
                    "ENDBLK" => {
                        if let Some((name, b)) = cur_block.take() {
                            dxf.blocks.insert(name, b);
                        }
                    }
                    _ => {
                        if let Some((_, b)) = cur_block.as_mut() {
                            b.ents.push(e);
                        }
                    }
                },
                _ => {}
            }
        }
    }

    while i < n {
        let (code, val) = pairs[i];
        i += 1;
        if code == 0 {
            flush(&mut cur, &section, &mut cur_block, &mut dxf);
            match val.trim() {
                "SECTION" => {
                    // o próximo par é o nome da seção
                    if i < n && pairs[i].0 == 2 {
                        section = pairs[i].1.trim().to_string();
                        i += 1;
                    }
                }
                "ENDSEC" => section.clear(),
                "EOF" => {
                    seen_eof = true;
                    break;
                }
                k => {
                    if section == "ENTITIES" || section == "BLOCKS" {
                        cur = Some(RawEnt {
                            kind: k.to_string(),
                            pairs: Vec::new(),
                        });
                    }
                }
            }
            continue;
        }
        if section == "HEADER" {
            if code == 9 {
                header_var = Some(val.trim().to_string());
            } else if let Some(v) = header_var.take()
                && v == "$INSUNITS"
                && code == 70
            {
                dxf.insunits = val.trim().parse().ok();
            }
        } else if let Some(e) = cur.as_mut() {
            e.pairs.push((code, val.to_string()));
        }
    }
    flush(&mut cur, &section, &mut cur_block, &mut dxf);
    if !seen_eof && dxf.entities.is_empty() && dxf.blocks.is_empty() {
        return Err(CadError::Invalid(
            "não há seções ENTITIES/BLOCKS (não parece um DXF)".into(),
        ));
    }
    Ok(dxf)
}

/// Matriz afim 2D `[a, b, c, d, e, f]`: `x' = a x + c y + e`, `y' = b x + d y + f`.
type Mat = [f64; 6];
const ID: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(m: &Mat, n: &Mat) -> Mat {
    // m aplicada depois de n
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

fn ap(m: &Mat, p: Pt) -> Pt {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

const MAX_DEPTH: usize = 16;
/// Passo angular máximo ao aproximar arcos e círculos.
const ARC_STEP_DEG: f64 = 5.0;

struct Acc {
    layers: HashMap<String, Layer>,
    labels: bool,
}

impl Acc {
    fn layer(&mut self, name: &str) -> &mut Layer {
        self.layers
            .entry(name.to_string())
            .or_insert_with(|| Layer::new(name))
    }
    fn poly(&mut self, layer: &str, pts: Vec<Pt>, closed: bool) {
        if pts.len() >= 2 {
            self.layer(layer).polylines.push(Polyline {
                points: pts,
                closed,
            });
        }
    }
}

/// Pontos de um arco do centro `c`, raio `r`, de `a0` a `a1` (graus, sentido anti-horário).
fn arc_points(c: Pt, r: f64, a0: f64, a1: f64) -> Vec<Pt> {
    let mut sweep = a1 - a0;
    while sweep <= 0.0 {
        sweep += 360.0;
    }
    let steps = ((sweep / ARC_STEP_DEG).ceil() as usize).max(2);
    (0..=steps)
        .map(|k| {
            let a = (a0 + sweep * k as f64 / steps as f64).to_radians();
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// Insere os pontos intermediários do arco de bulge entre `p0` e `p1` (sem repetir `p0`).
fn bulge_points(p0: Pt, p1: Pt, bulge: f64, out: &mut Vec<Pt>) {
    let chord = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2)).sqrt();
    if bulge.abs() < 1e-12 || chord == 0.0 {
        out.push(p1);
        return;
    }
    let theta = 4.0 * bulge.atan();
    let u = [(p1[0] - p0[0]) / chord, (p1[1] - p0[1]) / chord];
    let nrm = [-u[1], u[0]];
    let h = (chord / 2.0) / (theta / 2.0).tan();
    let mid = [(p0[0] + p1[0]) / 2.0, (p0[1] + p1[1]) / 2.0];
    let c = [mid[0] + nrm[0] * h, mid[1] + nrm[1] * h];
    let steps = ((theta.abs().to_degrees() / ARC_STEP_DEG).ceil() as usize).max(2);
    let (rx, ry) = (p0[0] - c[0], p0[1] - c[1]);
    for k in 1..=steps {
        if k == steps {
            out.push(p1);
            break;
        }
        let a = theta * k as f64 / steps as f64;
        let (s, co) = a.sin_cos();
        out.push([c[0] + rx * co - ry * s, c[1] + rx * s + ry * co]);
    }
}

fn vertices_with_bulge(verts: &[(Pt, f64)], closed: bool) -> Vec<Pt> {
    let mut out = Vec::new();
    if verts.is_empty() {
        return out;
    }
    out.push(verts[0].0);
    for i in 0..verts.len() - 1 {
        bulge_points(verts[i].0, verts[i + 1].0, verts[i].1, &mut out);
    }
    if closed && verts.len() > 2 {
        let last = verts[verts.len() - 1];
        let mut tmp = Vec::new();
        bulge_points(last.0, verts[0].0, last.1, &mut tmp);
        // o último ponto é o inicial: não repete
        tmp.pop();
        out.extend(tmp);
    }
    out
}

fn clean_mtext(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '\\' => match it.next() {
                Some('P') | Some('p') => out.push('\n'),
                Some('~') => out.push(' '),
                Some('\\') => out.push('\\'),
                Some('{') => out.push('{'),
                Some('}') => out.push('}'),
                Some(c) if "ACFHQTWfhqtw".contains(c) => {
                    // código de formato terminado em ';'
                    for x in it.by_ref() {
                        if x == ';' {
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            },
            '{' | '}' => {}
            c => out.push(c),
        }
    }
    out
}

fn flatten(
    ents: &[RawEnt],
    blocks: &HashMap<String, Block>,
    m: &Mat,
    inherit: Option<&str>,
    depth: usize,
    acc: &mut Acc,
) {
    let mut i = 0;
    while i < ents.len() {
        let e = &ents[i];
        i += 1;
        let layer_raw = e.layer();
        let layer = match inherit {
            Some(l) if layer_raw == "0" => l,
            _ => layer_raw,
        };
        match e.kind.as_str() {
            "LINE" => {
                let a = ap(m, [e.fd(10, 0.0), e.fd(20, 0.0)]);
                let b = ap(m, [e.fd(11, 0.0), e.fd(21, 0.0)]);
                acc.poly(layer, vec![a, b], false);
            }
            "LWPOLYLINE" => {
                let closed = (e.fd(70, 0.0) as i64) & 1 == 1;
                let mut verts: Vec<(Pt, f64)> = Vec::new();
                for (c, v) in &e.pairs {
                    let x: f64 = v.trim().parse().unwrap_or(0.0);
                    match c {
                        10 => verts.push(([x, 0.0], 0.0)),
                        20 => {
                            if let Some(l) = verts.last_mut() {
                                l.0[1] = x;
                            }
                        }
                        42 => {
                            if let Some(l) = verts.last_mut() {
                                l.1 = x;
                            }
                        }
                        _ => {}
                    }
                }
                let pts = vertices_with_bulge(&verts, closed);
                acc.poly(layer, pts.into_iter().map(|p| ap(m, p)).collect(), closed);
            }
            "POLYLINE" => {
                let flags = e.fd(70, 0.0) as i64;
                let closed = flags & 1 == 1;
                let mut verts: Vec<(Pt, f64)> = Vec::new();
                while i < ents.len() && ents[i].kind != "SEQEND" {
                    let v = &ents[i];
                    i += 1;
                    if v.kind == "VERTEX" {
                        let vf = v.fd(70, 0.0) as i64;
                        // 16 = ponto de controle de spline; 128 = vértice de malha/faces
                        if vf & 16 == 0 && vf & 128 == 0 {
                            verts.push(([v.fd(10, 0.0), v.fd(20, 0.0)], v.fd(42, 0.0)));
                        }
                    }
                }
                i += 1; // SEQEND
                let pts = vertices_with_bulge(&verts, closed);
                acc.poly(layer, pts.into_iter().map(|p| ap(m, p)).collect(), closed);
            }
            "CIRCLE" => {
                let c = [e.fd(10, 0.0), e.fd(20, 0.0)];
                let r = e.fd(40, 0.0);
                if r > 0.0 {
                    let mut pts = arc_points(c, r, 0.0, 360.0);
                    pts.pop();
                    acc.poly(layer, pts.into_iter().map(|p| ap(m, p)).collect(), true);
                }
            }
            "ARC" => {
                let c = [e.fd(10, 0.0), e.fd(20, 0.0)];
                let r = e.fd(40, 0.0);
                if r > 0.0 {
                    let pts = arc_points(c, r, e.fd(50, 0.0), e.fd(51, 360.0));
                    acc.poly(layer, pts.into_iter().map(|p| ap(m, p)).collect(), false);
                }
            }
            "TEXT" | "MTEXT" if acc.labels => {
                let raw = e.s(1).unwrap_or("");
                let text = if e.kind == "MTEXT" {
                    clean_mtext(raw)
                } else {
                    raw.to_string()
                };
                if !text.trim().is_empty() {
                    let pos = ap(m, [e.fd(10, 0.0), e.fd(20, 0.0)]);
                    let sc = (m[0] * m[0] + m[1] * m[1]).sqrt();
                    let rot = e.fd(50, 0.0) + m[1].atan2(m[0]).to_degrees();
                    acc.layer(layer).labels.push(Label {
                        text,
                        pos,
                        height: e.fd(40, 0.0) * sc,
                        rotation_deg: rot,
                    });
                }
            }
            "INSERT" if depth < MAX_DEPTH => {
                let Some(name) = e.s(2) else { continue };
                let Some(b) = blocks.get(name) else { continue };
                let sx = e.fd(41, 1.0);
                let sy = e.fd(42, 1.0);
                let (s, c) = e.fd(50, 0.0).to_radians().sin_cos();
                let pos = [e.fd(10, 0.0), e.fd(20, 0.0)];
                // M = T(pos) * R(rot) * S(sx, sy) * T(-base)
                let local: Mat = [
                    c * sx,
                    s * sx,
                    -s * sy,
                    c * sy,
                    pos[0] - (c * sx * b.base[0] - s * sy * b.base[1]),
                    pos[1] - (s * sx * b.base[0] + c * sy * b.base[1]),
                ];
                let nm = mul(m, &local);
                flatten(&b.ents, blocks, &nm, Some(layer), depth + 1, acc);
            }
            _ => {}
        }
    }
}

/// Lê um DXF 2D em texto e devolve a planta achatada. As coordenadas saem em metros
/// (conforme `$INSUNITS`, a opção `units` ou `default_units`).
pub fn read_dxf(text: &str, opts: &ReadOptions) -> Result<Plan, CadError> {
    let dxf = parse_dxf(text.trim_start_matches('\u{feff}'))?;
    let (units, assumed) = match opts.units {
        Some(u) => (u, false),
        None => match dxf.insunits.and_then(Units::from_insunits) {
            Some(u) => (u, false),
            None => (opts.default_units, true),
        },
    };
    let mut acc = Acc {
        layers: HashMap::new(),
        labels: opts.labels,
    };
    flatten(&dxf.entities, &dxf.blocks, &ID, None, 0, &mut acc);
    let k = units.meters();
    let mut layers: Vec<Layer> = acc.layers.into_values().collect();
    layers.sort_by(|a, b| a.name.cmp(&b.name));
    if k != 1.0 {
        for l in &mut layers {
            for pl in &mut l.polylines {
                for p in &mut pl.points {
                    *p = [p[0] * k, p[1] * k];
                }
            }
            for lb in &mut l.labels {
                lb.pos = [lb.pos[0] * k, lb.pos[1] * k];
                lb.height *= k;
            }
        }
    }
    Ok(Plan {
        layers,
        meters_per_unit: k,
        units_assumed: assumed,
    })
}
