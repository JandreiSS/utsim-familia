//! The common start of every desktop window of the family: the viewport (title, size,
//! minimum size, maximized, icon of the window and the taskbar), the `--versao`/`--ajuda`
//! options of the command line and the exit code of the window.
//!
//! Pure egui and std: each app keeps its own `eframe::run_native` and only builds the
//! options and reads the result through these helpers.

use std::fmt::Display;
use std::process::ExitCode;
use std::sync::Arc;

use egui::{IconData, ViewportBuilder};

/// The icon of the window from a PNG (`None` if the bytes are not a readable PNG).
pub fn icone(png: &[u8]) -> Option<Arc<IconData>> {
    let (w, h, rgba) = crate::familia::png_rgba(png)?;
    Some(Arc::new(IconData {
        rgba,
        width: w as u32,
        height: h as u32,
    }))
}

/// The viewport of a family window: the title, the size before maximizing, the minimum
/// size, maximized from the start and, if given and readable, the icon.
pub fn viewport(
    titulo: &str,
    tamanho: [f32; 2],
    minimo: [f32; 2],
    icone_png: Option<&[u8]>,
) -> ViewportBuilder {
    let v = ViewportBuilder::default()
        .with_title(titulo)
        .with_inner_size(tamanho)
        .with_min_inner_size(minimo)
        .with_maximized(true);
    match icone_png.and_then(icone) {
        Some(i) => v.with_icon(i),
        None => v,
    }
}

/// Answers `--versao`/`--version`/`-V` (prints `<programa> <versao>`) and
/// `--ajuda`/`--help`/`-h` (prints `ajuda`) when `primeiro` is one of them; `None` for any
/// other first argument, so the program goes on.
pub fn versao_ou_ajuda(
    primeiro: Option<&str>,
    programa: &str,
    versao: &str,
    ajuda: &str,
) -> Option<ExitCode> {
    match primeiro? {
        "--versao" | "--version" | "-V" => {
            println!("{programa} {versao}");
            Some(ExitCode::SUCCESS)
        }
        "--ajuda" | "--help" | "-h" => {
            println!("{ajuda}");
            Some(ExitCode::SUCCESS)
        }
        _ => None,
    }
}

/// The exit code of the window: success, or the error on stderr and failure.
pub fn saida<E: Display>(resultado: Result<(), E>) -> ExitCode {
    match resultado {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro na janela: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_icone_e_icone_invalido_dao_o_mesmo_viewport() {
        let a = viewport("T", [10.0, 20.0], [5.0, 6.0], None);
        let b = viewport("T", [10.0, 20.0], [5.0, 6.0], Some(b"nao e png"));
        assert_eq!(a.title.as_deref(), Some("T"));
        assert_eq!(a.inner_size, Some(egui::vec2(10.0, 20.0)));
        assert_eq!(a.min_inner_size, Some(egui::vec2(5.0, 6.0)));
        assert_eq!(a.maximized, Some(true));
        assert!(a.icon.is_none() && b.icon.is_none());
        let c = viewport("T", [1.0, 1.0], [1.0, 1.0], Some(PNG));
        let i = c.icon.expect("ícone");
        assert_eq!((i.width, i.height), (64, 64));
        assert_eq!(i.rgba.len(), 64 * 64 * 4);
    }

    const PNG: &[u8] = include_bytes!("../../../assets/icone/familia/utsim_64.png");

    #[test]
    fn outros_argumentos_seguem() {
        assert!(versao_ou_ajuda(None, "p", "1", "a").is_none());
        assert!(versao_ou_ajuda(Some("--utsim-abrir=x"), "p", "1", "a").is_none());
        assert!(versao_ou_ajuda(Some("-V"), "p", "1", "a").is_some());
    }
}
