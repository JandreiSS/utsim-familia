//! The files of the repository (`idiomas/`): every language has every key of pt-BR and no
//! other, with the same arguments, and no mistake of syntax.

use std::path::PathBuf;

use utsim_idiomas::Idioma;
use utsim_idiomas::conferencia::{conferir, texto};

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../idiomas")
}

#[test]
fn todos_os_idiomas_tem_as_mesmas_chaves_e_argumentos() {
    let relatorios = conferir(&raiz());
    assert!(!relatorios.is_empty(), "idiomas/pt-BR has no .ftl file");
    let t = texto(&relatorios);
    assert!(t.is_empty(), "the languages differ from pt-BR:\n{t}");
}

#[test]
fn cada_idioma_tem_sua_pasta() {
    for i in Idioma::TODOS {
        assert!(
            raiz().join(i.codigo()).is_dir(),
            "idiomas/{} missing",
            i.codigo()
        );
    }
}

#[test]
fn as_traducoes_de_maquina_avisam_no_cabecalho() {
    for i in [Idioma::Fr, Idioma::It] {
        for e in std::fs::read_dir(raiz().join(i.codigo()))
            .unwrap()
            .flatten()
        {
            let t = std::fs::read_to_string(e.path()).unwrap();
            assert!(
                t.lines()
                    .take(3)
                    .any(|l| l.starts_with("# ") && l.contains("revisão humana")),
                "{} needs the header comment",
                e.path().display()
            );
        }
    }
}
