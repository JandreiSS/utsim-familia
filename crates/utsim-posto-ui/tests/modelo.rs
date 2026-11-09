//! Tela "Modelo" sem janela: abrir, editar, gravar, trava entre dois apps e recarga.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use utsim_edit::abas::Aba;
use utsim_pasta::trava;
use utsim_posto_ui::{FnResumo, TelaModelo};

const EXEMPLO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/inspecao_e_retrabalho.ron"
);

fn resumo_vazio() -> FnResumo {
    Arc::new(|_| Ok(Default::default()))
}

fn copia(dir: &Path) -> PathBuf {
    let destino = dir.join("linha.ron");
    std::fs::copy(EXEMPLO, &destino).unwrap();
    destino
}

fn qualidade() -> TelaModelo {
    TelaModelo::nova("qualidade", Aba::Qualidade, resumo_vazio())
}

fn seguranca() -> TelaModelo {
    TelaModelo::nova("seguranca", Aba::Seguranca, resumo_vazio())
}

fn valor(t: &TelaModelo, aba: Aba, caminho: &str) -> Option<String> {
    t.campos(aba)
        .into_iter()
        .find(|c| c.caminho == caminho)
        .map(|c| c.valor)
}

#[test]
fn edita_e_grava_no_modelo() {
    let dir = tempfile::tempdir().unwrap();
    let m = copia(dir.path());
    let mut t = qualidade();
    t.abrir(&m, "Ana").unwrap();
    assert!(t.pode_editar());
    assert!(trava::caminho(&m).exists());
    t.selecionar("torno");
    assert_eq!(
        valor(&t, Aba::Qualidade, "inspection").as_deref(),
        Some("não")
    );
    t.editar("inspection", "sim").unwrap();
    t.editar("inspection.detect", "0,85").unwrap();
    assert!(t.pendente());
    t.gravar().unwrap();
    assert!(!t.pendente());
    // O arquivo relido tem o valor.
    let lido = utsim_edit::Document::open(&m).unwrap();
    let o = lido
        .model()
        .objects
        .iter()
        .find(|o| o.name == "torno")
        .unwrap();
    let det = o.inspection.as_ref().expect("inspeção ligada").detect;
    assert!((det - 0.85).abs() < 1e-9, "detect = {det}");
    // Desfazer e refazer.
    t.desfazer();
    let depois_de_desfazer = valor(&t, Aba::Qualidade, "inspection.detect").unwrap();
    assert_ne!(depois_de_desfazer.replace(',', "."), "0.85");
    t.refazer();
    let refeito = valor(&t, Aba::Qualidade, "inspection.detect").unwrap();
    assert_eq!(refeito.replace(',', "."), "0.85");
    // Fechar solta a trava.
    t.fechar();
    assert!(!trava::caminho(&m).exists());
}

#[test]
fn erro_de_faixa_aparece_e_nao_grava() {
    let dir = tempfile::tempdir().unwrap();
    let m = copia(dir.path());
    let antes = std::fs::read_to_string(&m).unwrap();
    let mut t = qualidade();
    t.abrir(&m, "Ana").unwrap();
    t.selecionar("torno");
    t.editar("inspection", "sim").unwrap();
    let e = t.editar("inspection.detect", "2").unwrap_err();
    assert!(!e.is_empty());
    assert_eq!(t.erro_do_campo("inspection.detect"), Some(e.as_str()));
    // Depois de um valor bom, o erro some.
    t.editar("inspection.detect", "0,9").unwrap();
    assert_eq!(t.erro_do_campo("inspection.detect"), None);
    // Sem gravar, o arquivo segue igual.
    assert_eq!(std::fs::read_to_string(&m).unwrap(), antes);
    t.fechar();
}

#[test]
fn dois_apps_um_escritor_e_mao_dupla() {
    let dir = tempfile::tempdir().unwrap();
    let m = copia(dir.path());

    let mut q = qualidade();
    q.abrir(&m, "Ana").unwrap();
    q.selecionar("torno");
    q.editar("inspection", "sim").unwrap();
    q.editar("inspection.detect", "0,8").unwrap();
    q.gravar().unwrap();

    // Outro app abre o mesmo arquivo: só leitura, com o aviso de quem edita.
    let mut s = seguranca();
    s.abrir(&m, "Bruno").unwrap();
    assert!(!s.pode_editar());
    let aviso = s.aviso_trava().expect("aviso");
    assert!(
        aviso.starts_with("Em edição por Ana no qualidade desde "),
        "{aviso}"
    );
    s.selecionar("torno");
    assert!(s.editar("ppe", "sim").is_err());
    assert!(s.gravar().is_err());
    // Tentar editar enquanto Ana segura não adianta.
    assert!(!s.tentar_editar());
    // Ele vê o que Ana gravou (Qualidade dele é só leitura aqui, mas o dado está lá).
    assert_eq!(
        valor(&s, Aba::Qualidade, "inspection").as_deref(),
        Some("sim")
    );

    // Ana solta. Bruno consegue editar e grava.
    q.soltar_edicao();
    assert!(!q.pode_editar());
    assert!(s.tentar_editar());
    assert!(s.pode_editar());
    s.selecionar("torno");
    s.editar("ppe", "oculos, luvas").unwrap();
    s.gravar().unwrap();

    // Ana, ao recarregar, vê a mudança de Bruno.
    q.selecionar("torno");
    assert_eq!(
        valor(&q, Aba::Seguranca, "ppe").as_deref(),
        Some("oculos, protetor_auricular")
    );
    q.recarregar();
    assert_eq!(
        valor(&q, Aba::Seguranca, "ppe").as_deref(),
        Some("oculos, luvas")
    );
    // E o que Ana gravou antes continua.
    assert_eq!(
        valor(&q, Aba::Qualidade, "inspection").as_deref(),
        Some("sim")
    );
    s.fechar();
    q.fechar();
    assert!(!trava::caminho(&m).exists());
}

#[test]
fn recarrega_sozinha_se_nao_ha_mudanca_local() {
    let dir = tempfile::tempdir().unwrap();
    let m = copia(dir.path());
    let mut q = qualidade();
    q.abrir(&m, "Ana").unwrap();
    q.selecionar("torno");
    assert_eq!(
        valor(&q, Aba::Qualidade, "inspection").as_deref(),
        Some("não")
    );

    // Alguém grava no disco (como o UTSim faria). Garante data de modificação nova.
    let mut d = utsim_edit::Document::open(&m).unwrap();
    let c = utsim_edit::abas::editar(d.model(), "torno", "inspection", "sim").unwrap();
    d.apply(c).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    utsim_pasta::write_replace(&m, d.to_text().as_bytes()).unwrap();

    q.verificar_disco();
    assert!(!q.mudou_no_disco);
    assert_eq!(
        valor(&q, Aba::Qualidade, "inspection").as_deref(),
        Some("sim")
    );

    // Com mudança local pendente, avisa e deixa escolher.
    q.editar("inspection.detect", "0,7").unwrap();
    let mut d = utsim_edit::Document::open(&m).unwrap();
    let c = utsim_edit::abas::editar(d.model(), "torno", "ppe", "luvas").unwrap();
    d.apply(c).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    utsim_pasta::write_replace(&m, d.to_text().as_bytes()).unwrap();
    q.verificar_disco();
    assert!(q.mudou_no_disco);
    assert!(q.pendente());
    q.recarregar();
    assert!(!q.mudou_no_disco);
    assert!(!q.pendente());
    q.fechar();
}

#[test]
fn renova_a_trava_pelo_relogio_do_quadro() {
    let dir = tempfile::tempdir().unwrap();
    let m = copia(dir.path());
    let mut q = qualidade();
    q.abrir(&m, "Ana").unwrap();
    let antes = trava::ler(&m, trava::agora_ms()).unwrap().renovada_ms;
    q.renovar_se_preciso(0.0);
    std::thread::sleep(std::time::Duration::from_millis(20));
    q.renovar_se_preciso(10.0);
    assert_eq!(
        trava::ler(&m, trava::agora_ms()).unwrap().renovada_ms,
        antes
    );
    q.renovar_se_preciso(61.0);
    assert!(trava::ler(&m, trava::agora_ms()).unwrap().renovada_ms > antes);
    q.fechar();
}

#[test]
fn lista_filtra_por_nome() {
    let dir = tempfile::tempdir().unwrap();
    let m = copia(dir.path());
    let mut q = qualidade();
    q.abrir(&m, "Ana").unwrap();
    let todos = q.objetos();
    assert!(todos.contains(&"torno".to_string()));
    q.filtro = "tor".into();
    let f = q.objetos();
    assert!(f.contains(&"torno".to_string()) && f.len() < todos.len());
    assert!(q.etiquetas("torno").contains(&"qualidade"));
    assert!(q.etiquetas("cliente").is_empty());
    q.fechar();
}
