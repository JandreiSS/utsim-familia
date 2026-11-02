//! Which asset each entity is drawn with and the pure rules that drive its parts: the
//! realistic look of the Bevy viewer (`crates/utsim-viewer/src/visual.rs`), ported here
//! because that crate pulls in Bevy. The numbers are the same; the Bevy file documents
//! where they come from (clip strides, fork reach, light colours). Keep both in step
//! until the rules move to a crate of their own (see `docs/plano_godot.md`).
//!
//! - `visual.asset` wins: a library name, a third-party catalogue name or a `.glb` file
//!   relative to the model.
//! - Mobiles by role: forklift `empilhadeira`, AGV `agv`, person the skeletal mannequin
//!   (`pessoa_operador`) when the interface has it, else `operador_simples`; a generic
//!   vehicle is a `paleteira` in a scene with pallet loads, else an `agv`.
//! - Objects by kind: `Rack` `porta_paletes`, `Conveyor` `esteira` and `Queue` floor
//!   `area`, sized from the block geometry; `Processor` `maquina`; `Combiner` `bancada`.
//!   Other kinds keep a simple box.

use std::f32::consts::PI;
use std::path::{Path, PathBuf};

use utsim_assets::{AreaSpec, ConveyorSpec, RackSpec, Spec, dims};
use utsim_model::VisualDef;
use utsim_scene::geometry::Geom;
use utsim_scene::handling::{BEAM, BELT_THICKNESS, BELT_WIDTH, RACK_DEPTH};
use utsim_scene::style::{self, Shape};
use utsim_scene::{MobileRole, Motion};

use crate::catalogue;

/// Catalogue entry of the default skeletal person, and the one with the carrying clips.
pub const PERSON: &str = "pessoa_operador";
pub const PERSON_CLIPS: &str = "pessoa_operador_carga";

/// Where an asset comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Library(Spec),
    /// A catalogue entry: its name and its file on disk.
    Catalogue {
        name: String,
        file: PathBuf,
    },
    /// A user `.glb` file.
    File(PathBuf),
}

/// How items ride on a mobile drawn with this asset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Carry {
    Forks { forward: f32, height: Option<f32> },
    Hands,
    Deck(f32),
    Stack,
}

impl Carry {
    pub fn name(self) -> &'static str {
        match self {
            Carry::Forks { .. } => "forks",
            Carry::Hands => "hands",
            Carry::Deck(_) => "deck",
            Carry::Stack => "stack",
        }
    }
}

/// How one block is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub source: Source,
    pub scale: f32,
    /// Extra rotation of the asset under the root, radians.
    pub yaw: f32,
    /// Colour of the `pintura` material, sRGB.
    pub color: Option<[f32; 3]>,
    pub carry: Carry,
    /// Translation of the asset under the root (Godot space).
    pub offset: [f32; 3],
    pub skeletal: bool,
}

/// Scene-wide choices.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Context {
    pub pallets: bool,
    pub skeletal_people: bool,
}

pub fn is_skeletal(name: &str) -> bool {
    name.eq_ignore_ascii_case(PERSON) || name.eq_ignore_ascii_case(PERSON_CLIPS)
}

pub fn parse_color(text: &str) -> Option<[f32; 3]> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(utsim_assets::builder::hex(v))
}

fn default_spec(kind: &str, role: Option<MobileRole>, geom: &Geom, ctx: Context) -> Option<Spec> {
    if let Some(role) = role {
        return Some(match role {
            MobileRole::Forklift => Spec::Empilhadeira,
            MobileRole::Agv => Spec::Agv,
            MobileRole::Person => Spec::OperadorSimples,
            MobileRole::Vehicle if ctx.pallets => Spec::Paleteira,
            MobileRole::Vehicle => Spec::Agv,
        });
    }
    match kind {
        "Rack" => sized(Spec::PortaPaletes(RackSpec::default()), geom),
        "Conveyor" => sized(Spec::Esteira(ConveyorSpec::default()), geom),
        "Queue" => sized(Spec::Area(AreaSpec::default()), geom),
        "Processor" => Some(Spec::Maquina),
        "Combiner" => Some(Spec::Bancada),
        _ => None,
    }
}

/// A parametric library asset sized by the block's geometry.
fn sized(spec: Spec, geom: &Geom) -> Option<Spec> {
    Some(match spec {
        Spec::PortaPaletes(_) => {
            let p = geom.rack_params()?;
            Spec::PortaPaletes(RackSpec {
                bays: p.bays,
                levels: p.levels,
                bay_width: p.bay_width as f32,
                level_height: p.level_height as f32,
                depth: RACK_DEPTH,
                beam: BEAM,
            })
        }
        Spec::Esteira(_) => {
            let p = geom.belt_params()?;
            Spec::Esteira(ConveyorSpec {
                length: p.length as f32,
                width: BELT_WIDTH,
                height: BELT_THICKNESS,
                pitch: 0.125,
            })
        }
        Spec::Area(_) => {
            let (half, _) = style::extents(Shape::LongBox);
            Spec::Area(AreaSpec {
                length: 2.0 * half[0],
                width: 2.0 * half[2],
            })
        }
        other => other,
    })
}

fn carry_of(spec: Option<&Spec>, role: Option<MobileRole>) -> Carry {
    match (spec, role) {
        (Some(Spec::Empilhadeira), _) | (None, Some(MobileRole::Forklift)) => Carry::Forks {
            forward: dims::FORK_CENTER,
            height: None,
        },
        (Some(Spec::Paleteira), _) => Carry::Forks {
            forward: dims::JACK_FORK_CENTER,
            height: Some(dims::JACK_FORK_TOP),
        },
        (Some(Spec::OperadorSimples), _) | (None, Some(MobileRole::Person)) => Carry::Hands,
        (Some(Spec::Agv), _) => Carry::Deck(dims::AGV_DECK),
        _ => Carry::Stack,
    }
}

/// The look of a block, or `None` for a simple box. `model_dir` resolves `visual.asset`
/// paths and finds the catalogue folder.
pub fn look_of(
    kind: &str,
    role: Option<MobileRole>,
    visual: Option<&VisualDef>,
    geom: &Geom,
    ctx: Context,
    model_dir: Option<&Path>,
) -> Option<Look> {
    let asset = visual.and_then(|v| v.asset.as_deref()).map(str::trim);
    let user_scale = visual
        .and_then(|v| v.scale)
        .filter(|s| s.is_finite() && *s > 0.0)
        .map_or(1.0, |s| s as f32);
    let user_yaw = visual
        .and_then(|v| v.yaw)
        .filter(|y| y.is_finite())
        .map_or(0.0, |y| (y as f32).to_radians());
    let color = visual
        .and_then(|v| v.color.as_deref())
        .and_then(parse_color);
    let library_or_file = asset.is_some_and(|a| {
        let n = a.to_lowercase();
        !matches!(n.as_str(), "operador" | "pessoa" | "person")
            && (Spec::by_name(&n).is_some() || n.ends_with(".glb"))
    });
    let entry = asset
        .filter(|a| Spec::by_name(a).is_none())
        .and_then(catalogue::by_name);
    let default_person =
        (role == Some(MobileRole::Person) && ctx.skeletal_people && !library_or_file)
            .then(|| catalogue::by_name(PERSON))
            .flatten();
    if let Some(entry) = entry.or(default_person) {
        let p = entry.placement(user_scale, user_yaw);
        return Some(Look {
            source: Source::Catalogue {
                name: entry.nome.clone(),
                file: catalogue::dir(model_dir).join(&entry.arquivo),
            },
            scale: p.scale,
            yaw: p.yaw,
            color,
            carry: carry_of(None, role),
            offset: p.offset,
            skeletal: is_skeletal(&entry.nome),
        });
    }
    let source = match asset {
        Some(a) if a.to_lowercase().ends_with(".glb") => {
            let path = Path::new(a);
            Source::File(match model_dir {
                Some(dir) if path.is_relative() => dir.join(path),
                _ => path.to_path_buf(),
            })
        }
        Some(a) => match Spec::by_name(a) {
            Some(spec) => Source::Library(sized(spec, geom).unwrap_or(spec)),
            None => Source::Library(default_spec(kind, role, geom, ctx)?),
        },
        None => Source::Library(default_spec(kind, role, geom, ctx)?),
    };
    let spec = match &source {
        Source::Library(s) => Some(*s),
        _ => None,
    };
    let belt_yaw = if geom.shape == Shape::Belt {
        geom.yaw
    } else {
        0.0
    };
    Some(Look {
        carry: carry_of(spec.as_ref(), role),
        source,
        scale: user_scale,
        yaw: belt_yaw + user_yaw,
        color,
        offset: [0.0; 3],
        skeletal: false,
    })
}

/// Animation clips of the skeletal person (see `visual.rs` and `people.rs` of the viewer).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Clip {
    Idle,
    Walk,
    Jog,
    Carry,
    Interact,
    PickUp,
    /// Legs of `Idle_Loop` under the arms of `Walk_Carry_Loop` held at its start.
    IdleCarry,
}

/// Speed above which a person jogs, m/s.
pub const JOG_SPEED: f32 = 2.0;
/// Crossfade between two clips, seconds of real time.
pub const FADE: f32 = 0.25;

impl Clip {
    pub const ALL: [Clip; 7] = [
        Clip::Idle,
        Clip::Walk,
        Clip::Jog,
        Clip::Carry,
        Clip::Interact,
        Clip::PickUp,
        Clip::IdleCarry,
    ];

    pub fn index(self) -> usize {
        Clip::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// glTF animation name and whether it is in the second file ([`PERSON_CLIPS`]).
    pub fn source(self) -> (&'static str, bool) {
        match self {
            Clip::Idle => ("Idle_Loop", false),
            Clip::Walk => ("Walk_Loop", false),
            Clip::Jog => ("Jog_Fwd_Loop", false),
            Clip::Carry | Clip::IdleCarry => ("Walk_Carry_Loop", true),
            Clip::Interact => ("Interact", false),
            Clip::PickUp => ("PickUp_Table", false),
        }
    }

    /// Ground covered by one loop of a walking clip, file units (measured on the stance
    /// foot, so the planted foot does not slide).
    pub fn stride(self) -> Option<f32> {
        match self {
            Clip::Walk => Some(1.306),
            Clip::Carry => Some(1.304),
            Clip::Jog => Some(5.44),
            _ => None,
        }
    }
}

pub fn person_clip(motion: Motion, carrying: bool) -> Clip {
    match motion {
        Motion::Moving { .. } if carrying => Clip::Carry,
        Motion::Moving { speed } if speed > JOG_SPEED => Clip::Jog,
        Motion::Moving { .. } => Clip::Walk,
        Motion::Loading { .. } | Motion::Unloading { .. } => Clip::PickUp,
        Motion::Working => Clip::Interact,
        _ if carrying => Clip::IdleCarry,
        _ => Clip::Idle,
    }
}

/// Time inside `clip` (lasting `duration` s) after walking `distance` metres at asset
/// `scale`, at simulated time `t`, `progress` into a load window: walking clips follow
/// the distance, the pick-up follows the window, the others loop on the simulated clock.
pub fn clip_time(
    clip: Clip,
    duration: f32,
    distance: f32,
    scale: f32,
    t: f64,
    progress: f32,
) -> f32 {
    if duration <= 0.0 {
        return 0.0;
    }
    let time = match (clip, clip.stride()) {
        (_, Some(stride)) => {
            let loops = distance as f64 / (stride * scale.max(1e-3)) as f64;
            loops.rem_euclid(1.0) as f32 * duration
        }
        (Clip::PickUp, _) => progress.clamp(0.0, 1.0) * duration,
        _ => t.rem_euclid(duration as f64) as f32,
    };
    time.clamp(0.0, duration * 0.9999)
}

/// Status light colour (sRGB): AGVs by motion, everything else by the state colour.
pub fn light_color(agv: bool, motion: Motion, state: Option<utsim_core::State>) -> [f32; 3] {
    if !agv {
        return style::state_color(state);
    }
    match motion {
        Motion::Moving { .. } => [0.15, 0.90, 0.30],
        Motion::Blocked => [1.0, 0.55, 0.0],
        Motion::Stopped => [0.95, 0.12, 0.08],
        Motion::Loading { .. } | Motion::Unloading { .. } => [0.15, 0.45, 1.0],
        Motion::Idle | Motion::Working => [0.85, 0.92, 1.0],
    }
}

/// Centre of the `index`-th item (`size` edge) carried at `base` heading `yaw` (Godot
/// space), or `None` for [`Carry::Stack`].
pub fn carried_center(
    carry: Carry,
    base: [f32; 3],
    yaw: f32,
    lift: f32,
    index: usize,
    size: f32,
) -> Option<[f32; 3]> {
    let ahead = |d: f32| [base[0] + d * yaw.cos(), base[2] - d * yaw.sin()];
    let k = index as f32;
    let (xz, y) = match carry {
        Carry::Forks { forward, height } => {
            let bottom = height.unwrap_or(lift);
            (ahead(forward), base[1] + bottom + size / 2.0 + k * size)
        }
        Carry::Hands => {
            let hands = if lift > 0.0 {
                lift
            } else {
                utsim_scene::compose::HAND_HEIGHT
            };
            (ahead(0.25 + size / 2.0), base[1] + hands + k * size)
        }
        Carry::Deck(h) => (ahead(0.0), base[1] + h + size / 2.0 + k * size),
        Carry::Stack => return None,
    };
    Some([xz[0], y, xz[1]])
}

/// Where an operator works at a station: at its front (`+Z`), facing it.
pub fn station_front(base: [f32; 3], half: [f32; 3]) -> ([f32; 3], f32) {
    ([base[0], base[1], base[2] + half[2] + 0.3], PI / 2.0)
}

/// Keeps a mobile of `radius` out of a station's footprint, moving it back along its
/// heading to the edge.
pub fn keep_outside(
    position: [f32; 3],
    yaw: f32,
    center: [f32; 3],
    half: [f32; 3],
    radius: f32,
) -> [f32; 3] {
    let (hx, hz) = (half[0] + radius, half[2] + radius);
    let (dx, dz) = (position[0] - center[0], position[2] - center[2]);
    if dx.abs() >= hx || dz.abs() >= hz {
        return position;
    }
    let (bx, bz) = (-yaw.cos(), yaw.sin());
    let exit = |d: f32, b: f32, h: f32| {
        if b.abs() < 1e-6 {
            f32::INFINITY
        } else {
            ((h * b.signum()) - d) / b
        }
    };
    let t = exit(dx, bx, hx).min(exit(dz, bz, hz));
    [position[0] + bx * t, position[1], position[2] + bz * t]
}

/// How far below the scene's stack height a queue's items rest (the realistic queue is a
/// floor area).
pub fn queue_drop() -> f32 {
    style::extents(Shape::LongBox).1 - 0.01
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(kind: &str) -> Geom {
        Geom::new(kind, None, None)
    }

    fn lib(l: Option<Look>) -> Option<&'static str> {
        match l.map(|l| l.source) {
            Some(Source::Library(s)) => Some(s.name()),
            _ => None,
        }
    }

    #[test]
    fn roles_and_kinds_pick_the_same_assets_as_the_bevy_viewer() {
        let ctx = Context::default();
        let pick = |kind: &str, role| look_of(kind, role, None, &geom(kind), ctx, None);
        assert_eq!(
            lib(pick("Transporter", Some(MobileRole::Forklift))),
            Some("empilhadeira")
        );
        assert_eq!(lib(pick("Transporter", Some(MobileRole::Agv))), Some("agv"));
        assert_eq!(
            lib(pick("Transporter", Some(MobileRole::Person))),
            Some("operador_simples")
        );
        assert_eq!(lib(pick("Processor", None)), Some("maquina"));
        assert_eq!(lib(pick("Queue", None)), Some("area"));
        assert_eq!(lib(pick("Conveyor", None)), Some("esteira"));
        assert_eq!(lib(pick("Combiner", None)), Some("bancada"));
        assert_eq!(lib(pick("Sink", None)), None);
        let pallets = Context {
            pallets: true,
            ..ctx
        };
        let jack = look_of(
            "Transporter",
            Some(MobileRole::Vehicle),
            None,
            &geom("x"),
            pallets,
            None,
        );
        assert_eq!(lib(jack), Some("paleteira"));
    }

    #[test]
    fn people_get_the_skeletal_mannequin_when_available() {
        let ctx = Context {
            skeletal_people: true,
            ..Context::default()
        };
        let l = look_of(
            "Transporter",
            Some(MobileRole::Person),
            None,
            &geom("x"),
            ctx,
            None,
        )
        .unwrap();
        assert!(l.skeletal);
        assert_eq!(l.carry, Carry::Hands);
        assert!(matches!(&l.source, Source::Catalogue { name, .. } if name == PERSON));
        // Naming the procedural one keeps it.
        let v = VisualDef {
            asset: Some("operador_simples".into()),
            ..VisualDef::default()
        };
        let l = look_of(
            "Transporter",
            Some(MobileRole::Person),
            Some(&v),
            &geom("x"),
            ctx,
            None,
        );
        assert_eq!(lib(l), Some("operador_simples"));
    }

    #[test]
    fn clips_follow_motion_and_distance() {
        assert_eq!(
            person_clip(Motion::Moving { speed: 1.2 }, false),
            Clip::Walk
        );
        assert_eq!(person_clip(Motion::Moving { speed: 3.0 }, false), Clip::Jog);
        assert_eq!(
            person_clip(Motion::Moving { speed: 1.0 }, true),
            Clip::Carry
        );
        assert_eq!(person_clip(Motion::Idle, true), Clip::IdleCarry);
        assert_eq!(person_clip(Motion::Working, false), Clip::Interact);
        assert_eq!(
            person_clip(Motion::Loading { progress: 0.3 }, false),
            Clip::PickUp
        );
        // Half a stride is half the clip, whatever the clock says.
        let half = clip_time(Clip::Walk, 1.0, 1.306 / 2.0, 1.0, 99.0, 0.0);
        assert!((half - 0.5).abs() < 1e-4);
        assert!((clip_time(Clip::PickUp, 2.0, 0.0, 1.0, 5.0, 0.25) - 0.5).abs() < 1e-6);
        assert!((clip_time(Clip::Idle, 2.0, 0.0, 1.0, 5.0, 0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn carried_loads_sit_ahead_of_the_carrier() {
        let forks = Carry::Forks {
            forward: 1.5,
            height: None,
        };
        let c = carried_center(forks, [0.0, 0.0, 0.0], 0.0, 0.3, 0, 0.8).unwrap();
        assert!((c[0] - 1.5).abs() < 1e-6 && (c[1] - 0.7).abs() < 1e-6 && c[2].abs() < 1e-6);
        // Facing +y of the plan is -Z in Godot.
        let c = carried_center(forks, [0.0, 0.0, 0.0], PI / 2.0, 0.3, 0, 0.8).unwrap();
        assert!((c[2] + 1.5).abs() < 1e-5);
        assert!(carried_center(Carry::Stack, [0.0; 3], 0.0, 0.0, 0, 0.3).is_none());
    }

    #[test]
    fn mobiles_stay_out_of_machines() {
        let p = keep_outside([0.0, 0.0, 0.0], 0.0, [0.0; 3], [1.0, 0.5, 1.0], 0.5);
        assert!((p[0] + 1.5).abs() < 1e-6);
        let far = keep_outside([5.0, 0.0, 0.0], 0.0, [0.0; 3], [1.0, 0.5, 1.0], 0.5);
        assert_eq!(far, [5.0, 0.0, 0.0]);
    }
}
