use super::*;
use crate::conferencia::{Relatorio, chaves, comparar};

static TESTE: Catalogo = Catalogo::novo(
    "teste",
    [
        "ola = Olá\nso-pt = Só em português\ndias = { $n ->\n    [one] falta { $n } dia\n   *[other] faltam { $n } dias\n}\nmedia = Média { $x }\n",
        "ola = Hello\ndias = { $n ->\n    [one] { $n } day left\n   *[other] { $n } days left\n}\nmedia = Mean { $x }\n",
        "ola = Hola\n",
        "ola = Bonjour\n",
        "ola = Ciao\n",
    ],
);

#[test]
fn textos_seguem_o_idioma_e_voltam_ao_portugues() {
    definir_nesta_thread(Some(Idioma::En));
    assert_eq!(TESTE.t("ola"), "Hello");
    assert_eq!(TESTE.t("so-pt"), "Só em português");
    assert_eq!(TESTE.f("dias", &[("n", 1.into())]), "1 day left");
    assert_eq!(TESTE.f("dias", &[("n", 3.into())]), "3 days left");
    assert_eq!(TESTE.f("media", &[("x", 2.5.into())]), "Mean 2.5");
    definir_nesta_thread(Some(Idioma::Fr));
    assert_eq!(TESTE.t("ola"), "Bonjour");
    assert_eq!(TESTE.f("media", &[("x", 2.5.into())]), "Média 2,5");
    definir_nesta_thread(Some(Idioma::PtBr));
    assert_eq!(TESTE.t("ola"), "Olá");
    assert_eq!(TESTE.f("dias", &[("n", 1.into())]), "falta 1 dia");
    assert_eq!(TESTE.f("dias", &[("n", 2.into())]), "faltam 2 dias");
    assert!(TESTE.faltas().is_empty());
    assert_eq!(TESTE.t("nao-existe"), "nao-existe");
    assert_eq!(TESTE.faltas(), vec!["nao-existe".to_string()]);
    definir_nesta_thread(None);
}

#[test]
fn codigos_e_formatos() {
    assert_eq!(Idioma::de_codigo("pt"), Some(Idioma::PtBr));
    assert_eq!(Idioma::de_codigo("en-US"), Some(Idioma::En));
    assert_eq!(Idioma::de_codigo("fr_CA"), Some(Idioma::Fr));
    assert_eq!(Idioma::de_codigo("it-IT"), Some(Idioma::It));
    assert_eq!(Idioma::de_codigo("de-DE"), None);
    for i in Idioma::TODOS {
        assert_eq!(Idioma::de_codigo(i.codigo()), Some(i));
        assert_ne!(i.separador_decimal(), i.separador_milhar());
    }
    definir_nesta_thread(Some(Idioma::PtBr));
    assert_eq!(dinheiro(1234567.891), "R$ 1.234.567,89");
    assert_eq!(dinheiro(-12.5), "−R$ 12,50");
    assert_eq!(data(2026, 11, 10), "10/11/2026");
    assert_eq!(mes(2026, 11), "11/2026");
    assert_eq!(ler_numero("1.234,5"), Some(1234.5));
    definir_nesta_thread(Some(Idioma::En));
    assert_eq!(dinheiro(1234567.891), "R$ 1,234,567.89");
    assert_eq!(data(2026, 11, 10), "2026-11-10");
    assert_eq!(ler_numero("1,234.5"), Some(1234.5));
    assert_eq!(ler_numero("1,5"), Some(1.5));
    definir_nesta_thread(Some(Idioma::Fr));
    assert_eq!(dinheiro(1234.5), "R$ 1\u{a0}234,50");
    assert_eq!(ler_numero("1\u{a0}234,5"), Some(1234.5));
    assert_eq!(marca_decimal("2.5"), "2,5");
    definir_nesta_thread(Some(Idioma::It));
    assert_eq!(dinheiro(1234.5), "R$ 1.234,50");
    assert_eq!(data(2026, 1, 2), "02/01/2026");
    definir_nesta_thread(None);
}

#[test]
fn a_conferencia_acha_faltas_sobras_e_argumentos() {
    let base = chaves("a = A\nb = B { $x }\nc = { $n ->\n *[other] C\n}\n").unwrap();
    let outro = chaves("a = A\nb = B { $y }\nc = C\nd = D\n").unwrap();
    let mut r = Relatorio::default();
    comparar(&base, &outro, &mut r);
    assert!(r.faltando.is_empty());
    assert_eq!(r.sobrando, vec!["d"]);
    let k: Vec<_> = r.argumentos.iter().map(|a| a.0.as_str()).collect();
    assert_eq!(k, vec!["b", "c"]);
    assert!(chaves("a = { $x").is_err());
}
