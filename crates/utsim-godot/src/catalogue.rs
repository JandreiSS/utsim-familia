//! The third-party asset catalogue (`assets/terceiros/catalogo.ron`), as the Bevy viewer
//! reads it (`utsim_viewer::terceiros`): the fields the Godot interface needs to place an
//! entry at real size facing `+x`. The text is compiled in; the files are on disk.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

const TEXT: &str = include_str!("../../../assets/terceiros/catalogo.ron");

#[derive(Debug, Deserialize)]
struct Catalogue {
    ativos: Vec<Entry>,
}

/// One entry (other fields of the file are ignored).
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Entry {
    pub nome: String,
    /// File, relative to the `assets/terceiros` folder.
    pub arquivo: String,
    pub minimo_m: (f32, f32, f32),
    pub centro_m: (f32, f32),
    pub giro_y_graus: f32,
    pub escala_real: f32,
}

/// Where the glTF scene of an entry goes under the entity's root (Godot space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub scale: f32,
    /// Rotation about `+Y`, radians.
    pub yaw: f32,
    /// Translation that puts the footprint centre on the root and the lowest point on the
    /// floor.
    pub offset: [f32; 3],
}

impl Entry {
    /// The same numbers as `utsim_viewer::terceiros::Entry::placement` (Godot and Bevy
    /// share the Y-up axes, so the rotation about Y is the same).
    pub fn placement(&self, scale: f32, yaw: f32) -> Placement {
        let s = self.escala_real * scale;
        let yaw = self.giro_y_graus.to_radians() + yaw;
        let (cx, cy, cz) = (self.centro_m.0, self.minimo_m.1, self.centro_m.1);
        let (sin, cos) = yaw.sin_cos();
        let x = cx * cos + cz * sin;
        let z = -cx * sin + cz * cos;
        Placement {
            scale: s,
            yaw,
            offset: [-x * s, -cy * s, -z * s],
        }
    }
}

pub fn entries() -> &'static [Entry] {
    static CATALOGUE: OnceLock<Vec<Entry>> = OnceLock::new();
    CATALOGUE.get_or_init(|| match ron::from_str::<Catalogue>(TEXT) {
        Ok(c) => c.ativos,
        Err(_) => Vec::new(),
    })
}

/// The entry called `name` (ignoring case and surrounding spaces).
pub fn by_name(name: &str) -> Option<&'static Entry> {
    let n = name.trim();
    entries().iter().find(|e| e.nome.eq_ignore_ascii_case(n))
}

/// The `assets/terceiros` folder: `UTSIM_TERCEIROS` when set, else the first
/// `assets/terceiros` with `catalogo.ron` above `start` (the model's folder), the working
/// directory or the library, else the source tree this crate was built from.
pub fn dir(start: Option<&Path>) -> PathBuf {
    if let Some(d) = std::env::var_os("UTSIM_TERCEIROS") {
        return PathBuf::from(d);
    }
    let starts = [
        start.map(Path::to_path_buf),
        std::env::current_dir().ok(),
        std::env::current_exe().ok(),
    ];
    for s in starts.into_iter().flatten() {
        for a in s.ancestors() {
            let d = a.join("assets").join("terceiros");
            if d.join("catalogo.ron").is_file() {
                return d;
            }
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/terceiros")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_person_is_in_the_catalogue_at_real_size() {
        let e = by_name("pessoa_operador").expect("entry");
        assert!(e.arquivo.ends_with("UAL1_Standard.glb"));
        let p = e.placement(1.0, 0.0);
        assert!((p.scale - e.escala_real).abs() < 1e-6);
        assert!((p.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!(by_name("PESSOA_OPERADOR_CARGA").is_some());
        assert!(dir(None).join(&e.arquivo).is_file());
    }
}
