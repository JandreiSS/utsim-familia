//! The opening screen drawn without a window: the first run (who is playing) and a
//! returning student. With `UTSIM_CAPTURAS=<pasta>` the PNGs are saved there for review.

use std::sync::Arc;

use egui::{Context, vec2};
use utsim_ensino_store::perfil::Perfil;
use utsim_ensino_ui::{App, NaHora, Tela};
use utsim_ui_tema::captura::{Captura, Imagem};

fn conferir(img: &Imagem, nome: &str) {
    let mut cores = std::collections::BTreeSet::new();
    for y in (0..img.height).step_by(7) {
        for x in (0..img.width).step_by(7) {
            cores.insert(img.pixel(x, y));
        }
    }
    assert!(cores.len() > 12, "{nome}: a tela quase não pintou nada");
    if let Some(d) = std::env::var_os("UTSIM_CAPTURAS") {
        let d = std::path::PathBuf::from(d);
        std::fs::create_dir_all(&d).unwrap();
        img.salvar_png(&d.join(format!("{nome}.png"))).unwrap();
    }
}

#[test]
fn abertura_picture() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::com(
        Perfil::em(&dir.path().join("aluno")),
        Arc::new(NaHora),
        || 0,
    );
    let mut cap = Captura::new(Context::default(), vec2(1440.0, 900.0));
    assert_eq!(app.tela, Tela::Identidade);
    let img = cap.imagem(3, |ui| app.draw(ui));
    conferir(&img, "ensino_abertura_primeira_vez");
    assert!(app.salvar_identidade("Ana Lima", "2027001", "ana"));
    app.tela = Tela::Identidade;
    let img = cap.imagem(3, |ui| app.draw(ui));
    conferir(&img, "ensino_abertura");
}
