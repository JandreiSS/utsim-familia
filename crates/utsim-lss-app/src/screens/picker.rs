//! A folder (and file) picker drawn by the app itself: no system dialog, so nothing more
//! to install on Windows or Linux. It lists the folders of the current place, the drives
//! (Windows) or the root and the home folder, and, when choosing a file, the files with
//! the extension asked.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::text::l1::l1;

/// What the chosen path is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A `.lss` folder to open.
    OpenProject,
    /// The folder where a new project goes.
    NewProjectParent,
    /// A `.lssz` file to import.
    ImportFile,
    /// The folder where an imported project goes.
    ImportDest,
}

impl Target {
    /// The extension of the files listed (choosing a file), if any.
    pub fn file_extension(self) -> Option<&'static str> {
        match self {
            Target::ImportFile => Some(utsim_lss_store::lssz::EXTENSION),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct FolderPicker {
    open: Option<Target>,
    current: PathBuf,
    typed: String,
    chosen: Option<(Target, PathBuf)>,
}

/// Folders and matching files of `dir`, each sorted by name (case ignored). Hidden entries
/// (starting with `.`) are left out, except `.lss` project folders.
pub fn list(dir: &Path, extension: Option<&str>) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                dirs.push(path);
            } else if let Some(ext) = extension
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case(ext))
            {
                files.push(path);
            }
        }
    }
    let key = |p: &PathBuf| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    (dirs, files)
}

/// Places to start from: the home folder, then the drives (Windows) or the root.
pub fn roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in ["USERPROFILE", "HOME"] {
        if let Some(h) = std::env::var_os(var).map(PathBuf::from)
            && h.is_dir()
            && !v.contains(&h)
        {
            v.push(h);
        }
    }
    if cfg!(windows) {
        for letter in b'A'..=b'Z' {
            let p = PathBuf::from(format!("{}:\\", letter as char));
            if p.is_dir() {
                v.push(p);
            }
        }
    } else {
        v.push(PathBuf::from("/"));
    }
    v
}

impl FolderPicker {
    /// Opens the picker for `target`, starting at `start` (or the home folder).
    pub fn open(&mut self, target: Target, start: &str) {
        let s = PathBuf::from(start.trim());
        let start = if s.is_dir() {
            s
        } else if s.parent().is_some_and(Path::is_dir) {
            s.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            roots().into_iter().next().unwrap_or_default()
        };
        self.typed = start.display().to_string();
        self.current = start;
        self.open = Some(target);
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The path chosen since the last call.
    pub fn take(&mut self) -> Option<(Target, PathBuf)> {
        self.chosen.take()
    }

    fn go(&mut self, p: PathBuf) {
        self.typed = p.display().to_string();
        self.current = p;
    }

    /// Draws the picker window when open.
    pub fn show(&mut self, ctx: &egui::Context) {
        let Some(target) = self.open else { return };
        let t = l1();
        let mut keep_open = true;
        let mut chosen = None;
        let mut go = None;
        egui::Window::new(t.picker_title)
            .collapsible(false)
            .resizable(true)
            .default_size([560.0, 420.0])
            .open(&mut keep_open)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for r in roots() {
                        if ui.small_button(r.display().to_string()).clicked() {
                            go = Some(r);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(t.picker_up).clicked()
                        && let Some(p) = self.current.parent()
                    {
                        go = Some(p.to_path_buf());
                    }
                    let r =
                        ui.add(egui::TextEdit::singleline(&mut self.typed).desired_width(420.0));
                    if r.lost_focus() && PathBuf::from(self.typed.trim()).is_dir() {
                        go = Some(PathBuf::from(self.typed.trim()));
                    }
                });
                ui.separator();
                let (dirs, files) = list(&self.current, target.file_extension());
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        if dirs.is_empty() && files.is_empty() {
                            ui.weak(t.picker_empty);
                        }
                        for d in &dirs {
                            let name = d
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let is_project = utsim_lss_store::project::is_project(d);
                            let label = if is_project {
                                format!("▣ {name}")
                            } else {
                                format!("📁 {name}")
                            };
                            let r = ui.selectable_label(false, label);
                            if r.double_clicked() && is_project && target == Target::OpenProject {
                                chosen = Some(d.clone());
                            } else if r.clicked() {
                                go = Some(d.clone());
                            }
                        }
                        for f in &files {
                            let name = f
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            if ui.selectable_label(false, format!("🗋 {name}")).clicked() {
                                chosen = Some(f.clone());
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if target.file_extension().is_none() && ui.button(t.picker_choose).clicked() {
                        chosen = Some(self.current.clone());
                    }
                    if ui.button(t.picker_cancel).clicked() {
                        self.open = None;
                    }
                    ui.weak(if target.file_extension().is_some() {
                        t.picker_file_tip
                    } else {
                        t.picker_tip
                    });
                });
            });
        if let Some(p) = go {
            self.go(p);
        }
        if let Some(p) = chosen {
            self.chosen = Some((target, p));
            self.open = None;
        }
        if !keep_open {
            self.open = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_and_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["b", "A", ".oculta", "Projeto.lss"] {
            std::fs::create_dir(dir.path().join(d)).unwrap();
        }
        for f in ["x.lssz", "y.LSSZ", "z.txt"] {
            std::fs::write(dir.path().join(f), b"").unwrap();
        }
        let (dirs, files) = list(dir.path(), Some("lssz"));
        let names = |v: &[PathBuf]| {
            v.iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&dirs), ["A", "b", "Projeto.lss"]);
        assert_eq!(names(&files), ["x.lssz", "y.LSSZ"]);
        assert!(list(dir.path(), None).1.is_empty());
        assert!(!roots().is_empty());

        let mut p = FolderPicker::default();
        p.open(Target::NewProjectParent, &dir.path().display().to_string());
        assert!(p.is_open());
        assert_eq!(p.current, dir.path());
        p.open(
            Target::ImportFile,
            &dir.path().join("x.lssz").display().to_string(),
        );
        assert_eq!(p.current, dir.path(), "a file starts at its folder");
        assert_eq!(Target::ImportFile.file_extension(), Some("lssz"));
    }
}
