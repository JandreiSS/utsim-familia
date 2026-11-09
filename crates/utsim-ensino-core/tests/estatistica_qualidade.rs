//! The package "Estatística e qualidade na fábrica" (`ensino/pacotes/estatistica_qualidade`):
//! it loads, every mission passes the checker, every question has an answer for every
//! student, the planted effects are recovered, the answers agree with an independent
//! computation by the textbook formulas, the glossary links point to manual chapters and
//! the texts follow the writing rules.

use std::path::{Path, PathBuf};

use utsim_ensino_core::conferir::{conferir, descrever};
use utsim_ensino_core::dados::Gerador;
use utsim_ensino_core::missao::Previsao;
use utsim_ensino_core::pacote::{Arquivos, MissaoCarregada, PacoteCarregado, carregar};
use utsim_ensino_core::questoes::QuestaoDados;

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pasta() -> PathBuf {
    raiz().join("ensino/pacotes/estatistica_qualidade")
}

fn ler(dir: &Path, base: &Path, out: &mut Arquivos) {
    for e in std::fs::read_dir(dir).expect("pasta") {
        let p = e.expect("entrada").path();
        if p.is_dir() {
            ler(&p, base, out);
        } else {
            let rel: Vec<String> = p
                .strip_prefix(base)
                .expect("dentro")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.insert(rel.join("/"), std::fs::read(&p).expect("ler"));
        }
    }
}

fn arquivos() -> Arquivos {
    let mut a = Arquivos::new();
    ler(&pasta(), &pasta(), &mut a);
    a
}

fn pacote() -> PacoteCarregado {
    carregar(&arquivos()).unwrap_or_else(|e| panic!("{}", e.join("\n")))
}

fn missao<'a>(p: &'a PacoteCarregado, id: &str) -> &'a MissaoCarregada {
    p.missoes
        .iter()
        .find(|m| m.missao.id == id)
        .unwrap_or_else(|| panic!("{id}"))
}

fn alunos() -> impl Iterator<Item = String> {
    (1..=200).map(|i| format!("2027{i:03}"))
}

/// The questions about data of a mission: (question, data set id).
fn questoes(m: &MissaoCarregada) -> Vec<(QuestaoDados, String)> {
    m.missao
        .previsao
        .iter()
        .filter_map(|p| match p {
            Previsao::Dados {
                questao, conjunto, ..
            } => Some((questao.clone(), conjunto.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn o_pacote_tem_as_seis_missoes_de_analise() {
    let p = pacote();
    assert_eq!(p.missoes.len(), 6, "{} missões", p.missoes.len());
    for m in &p.missoes {
        assert!(!m.missao.dados.is_empty(), "{}: sem dados", m.missao.id);
        assert!(
            !m.missao.analisar_em.is_empty(),
            "{}: sem app de análise",
            m.missao.id
        );
        assert!(!questoes(m).is_empty(), "{}: sem questões", m.missao.id);
    }
}

#[test]
fn todas_as_missoes_passam_no_conferidor() {
    let p = pacote();
    let mut falhas = Vec::new();
    for m in &p.missoes {
        let c = conferir(m).unwrap();
        if !c.passou() {
            falhas.push(format!(
                "{}: {} (melhor: {})",
                m.missao.id,
                c.erros.join("; "),
                descrever(m, &c.melhor)
            ));
        }
    }
    assert!(falhas.is_empty(), "\n{}", falhas.join("\n"));
}

#[test]
fn toda_questao_tem_resposta_para_todo_aluno() {
    let p = pacote();
    let mut falhas = Vec::new();
    for m in &p.missoes {
        for (q, cj) in questoes(m) {
            let conjunto = m.missao.conjunto(&cj).unwrap();
            for a in alunos() {
                match q.resposta(&conjunto.gerar(&m.missao.id, &a)) {
                    Ok(v) if v.is_finite() => {}
                    Ok(v) => falhas.push(format!("{} {q:?} {a}: {v}", m.missao.id)),
                    Err(e) => falhas.push(format!("{} {q:?} {a}: {e}", m.missao.id)),
                }
            }
        }
    }
    assert!(falhas.is_empty(), "\n{}", falhas.join("\n"));
}

#[test]
fn os_dados_mudam_de_um_aluno_para_outro() {
    let p = pacote();
    for m in &p.missoes {
        for cj in &m.missao.dados {
            let a = cj.gerar(&m.missao.id, "2027001");
            let b = cj.gerar(&m.missao.id, "2027002");
            assert_ne!(a.valores, b.valores, "{} {}", m.missao.id, cj.id);
            assert_eq!(a, cj.gerar(&m.missao.id, "2027001"));
        }
    }
}

#[test]
fn replicacoes_pela_formula_do_livro() {
    let p = pacote();
    let m = missao(&p, "eq/replicacoes");
    let cj = m.missao.conjunto("producao").unwrap();
    for a in alunos().take(30) {
        let c = cj.gerar(&m.missao.id, &a);
        let xs: Vec<f64> = c.valores.iter().map(|v| v[0]).collect();
        let n = xs.len() as f64;
        let media = xs.iter().sum::<f64>() / n;
        let s = (xs.iter().map(|x| (x - media).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        // t de 9 graus de liberdade a 95 %, da tabela.
        let h = 2.262 * s / n.sqrt();
        let meia = QuestaoDados::MeiaLargura { confianca: 0.95 }
            .resposta(&c)
            .unwrap();
        assert!((meia - h).abs() < 0.01 * h, "{a}: {meia} contra {h}");
        let r = QuestaoDados::Replicacoes { meia_largura: 5.0 }
            .resposta(&c)
            .unwrap();
        let esperado = ((2.262 * s / 5.0).powi(2)).ceil().max(n);
        assert!((r - esperado).abs() <= 1.0, "{a}: {r} contra {esperado}");
    }
}

#[test]
fn diferenca_pareada_recupera_o_efeito_plantado() {
    let p = pacote();
    let m = missao(&p, "eq/alternativas");
    let cj = m.missao.conjunto("comparacao").unwrap();
    let Gerador::Pareado { diferenca, .. } = cj.gerador else {
        panic!("gerador")
    };
    let mut soma = 0.0;
    let mut significativas = 0;
    for a in alunos() {
        let c = cj.gerar(&m.missao.id, &a);
        let d = QuestaoDados::DiferencaMedia.resposta(&c).unwrap();
        let manual = c.valores.iter().map(|v| v[1] - v[0]).sum::<f64>() / c.valores.len() as f64;
        assert!((d - manual).abs() < 1e-9);
        soma += d;
        if QuestaoDados::DiferencaSignificativa.resposta(&c).unwrap() == 0.0 {
            significativas += 1;
        }
    }
    let media = soma / 200.0;
    assert!(
        (media - diferenca).abs() < 0.6,
        "média das diferenças {media}"
    );
    // Com o efeito plantado, a maioria dos alunos vê a diferença, mas não todos.
    assert!(significativas > 120, "{significativas} de 200");
}

#[test]
fn carta_xbar_r_pelas_constantes_do_livro() {
    let p = pacote();
    let m = missao(&p, "eq/carta");
    let cj = m.missao.conjunto("diametros").unwrap();
    // A2 = 3 / (d2 √n) e D4 = 1 + 3 d3 / d2, com d2 = 2,326 e d3 = 0,864 para n = 5 (a
    // tabela arredonda A2 para 0,577, o que muda a resposta de um ponto rente ao limite).
    let (a2, d4) = (3.0 / (2.326 * 5f64.sqrt()), 1.0 + 3.0 * 0.864 / 2.326);
    for a in alunos() {
        let c = cj.gerar(&m.missao.id, &a);
        let medias: Vec<f64> = c
            .valores
            .iter()
            .map(|g| g.iter().sum::<f64>() / g.len() as f64)
            .collect();
        let amps: Vec<f64> = c
            .valores
            .iter()
            .map(|g| {
                g.iter().copied().fold(f64::MIN, f64::max)
                    - g.iter().copied().fold(f64::MAX, f64::min)
            })
            .collect();
        let xbb = medias[..15].iter().sum::<f64>() / 15.0;
        let rb = amps[..15].iter().sum::<f64>() / 15.0;
        let manual = (15..medias.len())
            .find(|&i| medias[i] > xbb + a2 * rb || medias[i] < xbb - a2 * rb || amps[i] > d4 * rb)
            .map(|i| (i + 1) as f64);
        let jogo = QuestaoDados::PrimeiroSubgrupoFora { base: 15 }
            .resposta(&c)
            .unwrap();
        assert_eq!(Some(jogo), manual, "{a}");
    }
}

#[test]
fn a_deriva_aparece_depois_que_comeca_na_maioria() {
    let p = pacote();
    let m = missao(&p, "eq/carta");
    let cj = m.missao.conjunto("diametros").unwrap();
    let depois = alunos()
        .filter(|a| {
            QuestaoDados::PrimeiroSubgrupoFora { base: 15 }
                .resposta(&cj.gerar(&m.missao.id, a))
                .unwrap()
                >= 21.0
        })
        .count();
    assert!(depois > 160, "{depois} de 200");
}

#[test]
fn cpk_pela_amplitude_media() {
    let p = pacote();
    let m = missao(&p, "eq/capacidade");
    for (id, minimo, maximo) in [("antes", 0.4, 0.8), ("depois", 1.3, 1.9)] {
        let cj = m.missao.conjunto(id).unwrap();
        for a in alunos().take(50) {
            let c = cj.gerar(&m.missao.id, &a);
            let todos: Vec<f64> = c.valores.iter().flatten().copied().collect();
            let media = todos.iter().sum::<f64>() / todos.len() as f64;
            let rb = c
                .valores
                .iter()
                .map(|g| {
                    g.iter().copied().fold(f64::MIN, f64::max)
                        - g.iter().copied().fold(f64::MAX, f64::min)
                })
                .sum::<f64>()
                / c.valores.len() as f64;
            let sigma = rb / 2.326;
            let manual = ((25.015 - media).min(media - 24.985)) / (3.0 * sigma);
            let jogo = QuestaoDados::Cpk {
                lie: Some(24.985),
                lse: Some(25.015),
            }
            .resposta(&c)
            .unwrap();
            assert!(
                (jogo - manual).abs() < 0.01,
                "{id} {a}: {jogo} contra {manual}"
            );
            assert!((minimo..maximo).contains(&jogo), "{id} {a}: Cpk {jogo}");
        }
    }
}

#[test]
fn estratificacao_acha_o_torno_sorteado() {
    let p = pacote();
    let m = missao(&p, "eq/estratificacao");
    let cj = m.missao.conjunto("comprimentos").unwrap();
    let mut certos = 0;
    let mut tornos = std::collections::BTreeSet::new();
    for a in alunos() {
        let r = cj.receita(&m.missao.id, &a);
        let Gerador::MediaDeslocada { afetado, .. } = r.gerador else {
            panic!("gerador")
        };
        tornos.insert(afetado);
        let achado = QuestaoDados::GrupoDeslocado.resposta(&r.gerar()).unwrap() as usize;
        if achado == afetado {
            certos += 1;
        }
    }
    assert_eq!(tornos.len(), 4, "o torno afetado muda de aluno para aluno");
    assert!(certos >= 198, "{certos} de 200");
}

#[test]
fn a_fonte_do_erro_de_medicao_vem_dos_dados() {
    let p = pacote();
    let m = missao(&p, "eq/medicao");
    let cj = m.missao.conjunto("rr_paquimetro").unwrap();
    let mut operadores = 0;
    for a in alunos() {
        let c = cj.gerar(&m.missao.id, &a);
        let (rep, reprod) = utsim_ensino_core::questoes::fontes_rr(&c).unwrap();
        let f = QuestaoDados::FonteDominante.resposta(&c).unwrap();
        assert_eq!(f, if rep >= reprod { 0.0 } else { 1.0 });
        if f == 1.0 {
            operadores += 1;
        }
    }
    // Com 3 operadores a estimativa da reprodutibilidade varia, mas o efeito plantado
    // aparece na maioria dos alunos.
    assert!(operadores > 120, "{operadores} de 200");
}

#[test]
fn glossario_aponta_capitulos_do_manual() {
    let p = pacote();
    let manual = raiz().join("docs/manual/src");
    for t in &p.glossario.termos {
        if let Some(m) = &t.manual {
            assert!(manual.join(m).exists(), "{}: capítulo {m} não existe", t.id);
        }
        assert!(
            p.missoes.iter().any(|m| m.missao.conceitos.contains(&t.id)),
            "termo '{}' não é usado por nenhuma missão",
            t.id
        );
    }
}

#[test]
fn textos_seguem_as_regras_de_escrita() {
    for (path, bytes) in arquivos() {
        if !(path.ends_with(".md") || path.ends_with(".ron")) {
            continue;
        }
        let txt = String::from_utf8(bytes).unwrap();
        for (i, l) in txt.lines().enumerate() {
            assert!(!l.contains('\u{2014}'), "{path}:{}: travessão", i + 1);
            if path.ends_with(".md") {
                assert!(!l.contains(';'), "{path}:{}: ponto e vírgula", i + 1);
            }
        }
    }
}
