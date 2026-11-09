//! The interface in English and Spanish (EN13): every screen draws in each language, and the
//! language chosen in the profile comes back. A test binary of its own, because the
//! language is global to the program.

use std::sync::Arc;

use egui::{Context, RawInput};
use utsim_ensino_store::perfil::Perfil;
use utsim_ensino_ui::texto::{self, Idioma};
use utsim_ensino_ui::{App, NaHora, Tela};

fn frames(app: &mut App, ctx: &Context, n: usize) {
    for _ in 0..n {
        let mut out = ctx.run_ui(RawInput::default(), |ui| app.draw(ui));
        out.textures_delta.clear();
    }
}

#[test]
fn todas_as_telas_nas_tres_linguas() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Context::default();
    for i in Idioma::TODOS {
        texto::definir(i);
        let mut app = App::com(Perfil::em(dir.path()), Arc::new(NaHora), || {
            1_800_000_000_000
        });
        frames(&mut app, &ctx, 2);
        app.salvar_identidade("Ana Lima", "2027001", "ana");
        app.config.professor = true;
        for t in [
            Tela::Mapa,
            Tela::Glossario,
            Tela::Turma,
            Tela::Professor,
            Tela::Ajustes,
        ] {
            app.tela = t;
            frames(&mut app, &ctx, 2);
        }
        app.abrir_missao(0);
        frames(&mut app, &ctx, 3);
        app.abrir_areia(0);
        frames(&mut app, &ctx, 2);
    }
    texto::definir(Idioma::En);
    assert_eq!(texto::aba_ajustes(), "Settings");
    assert_eq!(
        texto::f(texto::ato_titulo(), &[&1, &"Gargalo"]),
        "Act 1. Gargalo"
    );
    texto::definir(Idioma::Es);
    assert_eq!(texto::aba_campanha(), "Campaña");
    texto::definir(Idioma::Pt);
    assert_eq!(texto::aba_campanha(), "Campanha");
}

#[test]
fn a_lingua_do_perfil_volta_ao_abrir() {
    let dir = tempfile::tempdir().unwrap();
    let perfil = Perfil::em(dir.path());
    let mut c = perfil.config();
    c.idioma = Some("es".into());
    perfil.salvar_config(&c).unwrap();
    // App::new reads the profile's language (App::com is the test host without it).
    let app = App::new(Perfil::em(dir.path()));
    assert_eq!(app.config.idioma.as_deref(), Some("es"));
    assert_eq!(texto::idioma_atual(), Idioma::Es);
    texto::definir(Idioma::Pt);
}
