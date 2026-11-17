//! The drawn statement in the mission editor: the scene suggested from the mission, the
//! picture still to be generated as an error, the files going to the package and back.
//! Rendering with Blender is checked by hand (`renderiza_com_o_blender`, ignored).

use std::path::PathBuf;

use utsim_ensino_core::campanha::arquivo;
use utsim_ensino_core::cena::TipoCartao;
use utsim_ensino_core::editor::{Edicao, PacoteProfessor};
use utsim_ensino_core::editor_cena::{
    achar_blender, achar_elementos, caminhos_cena, elementos_completos,
};
use utsim_ensino_core::missao::Missao;

fn ler(p: &str) -> String {
    String::from_utf8(arquivo(p).unwrap().to_vec()).unwrap()
}

/// The mixed-model line of the campaign, opened in the editor without its scene.
fn mix_sem_cena() -> Edicao {
    let mut m = Missao::parse(&ler("ato4/mix.ron")).unwrap();
    m.cena = None;
    m.id = "prof/linha_mista".into();
    Edicao::abrir(m, &ler("modelos/ventilador_mix.ron"), &ler("ato4/mix.md")).unwrap()
}

#[test]
fn cena_sugerida_pela_missao() {
    let ed = mix_sem_cena();
    let c = ed.sugerir_cena();
    assert_eq!(c.imagem, "linha_mista.cena.png");
    // The line is about 66 m long: half of it.
    assert_eq!(c.escala, 0.5);
    let tem = |no: &str, t: &TipoCartao| c.cartoes.iter().any(|x| x.no == no && &x.tipo == t);
    for n in 1..=10 {
        assert!(tem(&format!("est{n}"), &TipoCartao::Estacao(n)), "est{n}");
    }
    // The sequence lever changes the arrival table: its card goes on the arrivals.
    assert!(
        tem("chegada", &TipoCartao::Alavanca("sequencia".into())),
        "{:?}",
        c.cartoes
    );
    // The prediction under the first queue; the measures on the exits they count.
    assert!(
        tem("fila1@pe", &TipoCartao::Previsao("lotes".into())),
        "{:?}",
        c.cartoes
    );
    assert!(tem("saida_mesa", &TipoCartao::Metrica("mesa".into())));
    assert!(tem("saida_coluna", &TipoCartao::Metrica("coluna".into())));
    // A measure of the pieces is live: a value card.
    assert!(c.cartoes.iter().any(
        |x| matches!(&x.tipo, TipoCartao::Valor { rotulo, .. } if rotulo == "Estações usadas")
    ));
    // The suggestion is a valid scene for the mission.
    assert!(
        c.validar(&ed.missao(), &ed.modelo).is_empty(),
        "{:?}",
        c.validar(&ed.missao(), &ed.modelo)
    );
}

#[test]
fn sem_imagem_nao_grava_e_os_arquivos_vao_ao_pacote() {
    let mut ed = mix_sem_cena();
    ed.base.cena = Some(ed.sugerir_cena());
    assert!(
        ed.erros().iter().any(|e| e.contains("gere a imagem")),
        "{:?}",
        ed.erros()
    );
    // The picture of the campaign stands in for one rendered here.
    let c = ed.base.cena.clone().unwrap();
    let mut campanha = Missao::parse(&ler("ato4/mix.ron")).unwrap().cena.unwrap();
    campanha.imagem = c.imagem.clone();
    ed.base.cena = Some(utsim_ensino_core::cena::Cena {
        cartoes: c.cartoes.clone(),
        ..campanha.clone()
    });
    ed.cena_arquivos.insert(
        c.imagem.clone(),
        arquivo("ato4/mix.cena.png").unwrap().to_vec(),
    );
    ed.cena_arquivos.insert(
        c.caminho_ancoras(),
        arquivo("ato4/mix.cena.ancoras.json").unwrap().to_vec(),
    );
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    let mut p = PacoteProfessor::novo("prof", "Pacote", "Ana");
    let caminho = p.gravar(&mut ed, 0).unwrap();
    let dir = caminho.rsplit_once('/').unwrap().0;
    for f in caminhos_cena(&c, &[]) {
        assert!(p.todos().contains_key(&format!("{dir}/{f}")), "{f}");
    }
    // The package reads the scene back; the mission opens again with its files.
    let carregado = p.conferir().unwrap_or_else(|e| panic!("{}", e.join("\n")));
    assert!(carregado.missoes[0].cena.is_some());
    let de_novo = p.abrir(&caminho).unwrap();
    assert_eq!(de_novo.cena_arquivos, ed.cena_arquivos);
}

/// The elements of the repository (the same folders the installer carries) are complete; a
/// folder with the models but without the clips of the people is not.
#[test]
fn elementos_precisam_dos_clipes() {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(elementos_completos(&raiz.join("assets/modelos3d")));
    let tmp = std::env::temp_dir().join(format!("utsim-elementos-{}", std::process::id()));
    let pasta = tmp.join("assets/modelos3d");
    std::fs::create_dir_all(&pasta).unwrap();
    for f in ["pessoa_homem.glb", "posto.glb"] {
        std::fs::write(pasta.join(f), b"glb").unwrap();
    }
    assert!(!elementos_completos(&pasta));
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Renders the scene of the mixed-model line with the Blender of this machine. Needs
/// Blender and the 3D elements: `cargo test -p utsim-ensino-core --test editor_cena --
/// --ignored`.
#[test]
#[ignore]
fn renderiza_com_o_blender() {
    let (Some(b), Some(el)) = (
        achar_blender(),
        achar_elementos().or_else(|| {
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/modelos3d"))
        }),
    ) else {
        panic!("sem Blender");
    };
    let mut ed = mix_sem_cena();
    ed.base.cena = Some(ed.sugerir_cena());
    let arquivos = ed.gerar_cena(&b, &el).unwrap();
    let c = ed.base.cena.as_ref().unwrap();
    assert!(arquivos[&c.imagem].len() > 10_000);
    ed.cena_arquivos = arquivos;
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
}
