//! `requer` of a package (EN12): a need this version does not meet is a warning, and the
//! package still opens.

use utsim_ensino_core::requisitos::{NUCLEO, faltando, verificar};

fn v(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| x.to_string()).collect()
}

#[test]
fn o_que_esta_versao_atende() {
    for r in [
        "nucleo",
        "nucleo >= 0.1",
        "nucleo>=0.0.1",
        "nucleo < 99",
        "nucleo >= 0.1-dev.15",
        "linha:lean_office",
        "linha:servicos",
        "linha:logistica",
        "linha:qualidade",
        "linha:seguranca",
    ] {
        assert_eq!(verificar(r), Ok(()), "{r}");
    }
}

#[test]
fn o_que_falta_vem_com_a_razao() {
    let f = faltando(&v(&[
        "nucleo >= 99",
        "linha:mineracao",
        "placas",
        "nucleo >= x",
        "nucleo ~ 1",
        "linha:lean_office >= 2",
        "nucleo >= 0.1",
    ]));
    let r: Vec<&str> = f.iter().map(|x| x.requisito.as_str()).collect();
    assert_eq!(
        r,
        [
            "nucleo >= 99",
            "linha:mineracao",
            "placas",
            "nucleo >= x",
            "nucleo ~ 1",
            "linha:lean_office >= 2"
        ]
    );
    assert!(f[0].razao.contains(NUCLEO), "{}", f[0].razao);
    assert!(f[1].razao.contains("mineracao"));
    assert!(f[2].razao.contains("não é conhecido"));
    assert!(f[3].razao.contains("ilegível"));
    assert!(f[4].razao.contains("operador"));
    assert!(f[0].to_string().starts_with("nucleo >= 99 ("));
}

#[test]
fn pacote_com_requisito_ausente_abre_e_avisa() {
    let mut arquivos = utsim_ensino_core::campanha::arquivos();
    let txt = String::from_utf8(arquivos["pacote.ron"].clone()).unwrap();
    let novo = txt.replacen(
        "requer: [\"nucleo >= 0.1\"]",
        "requer: [\"nucleo >= 0.1\", \"linha:mineracao\", \"nucleo >= 99\"]",
        1,
    );
    assert_ne!(txt, novo, "o pacote da campanha mudou o requer");
    arquivos.insert("pacote.ron".into(), novo.into_bytes());
    let p = utsim_ensino_core::pacote::carregar(&arquivos).expect("o pacote abre");
    assert_eq!(p.faltando.len(), 2);
    assert!(!p.missoes.is_empty());
    let m = utsim_ensino_core::pacote::manifesto(&arquivos).unwrap();
    assert_eq!(faltando(&m.requer), p.faltando);
}
