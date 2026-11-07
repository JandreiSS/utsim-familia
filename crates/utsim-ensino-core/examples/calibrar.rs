//! Calibration of the missions: every feasible combination with the mean of each measure
//! and whether it reaches the goals, then the checker's verdict.
//!
//! `cargo run -p utsim-ensino-core --example calibrar -- [pasta_do_pacote] [trecho_do_id]`
//! (without a folder, the built-in campaign).

use std::path::Path;

use utsim_ensino_core::conferir::{conferir, descrever};
use utsim_ensino_core::motor::{avaliar, rodar, valor_metrica};
use utsim_ensino_core::nota::{Intervalo, portao, situacao};
use utsim_ensino_core::pacote::{Arquivos, carregar};

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arquivos = match args.first().filter(|a| Path::new(a).is_dir()) {
        Some(d) => {
            let mut a = Arquivos::new();
            ler(Path::new(d), Path::new(d), &mut a);
            a
        }
        None => utsim_ensino_core::campanha::arquivos(),
    };
    let filtro = args
        .iter()
        .find(|a| !Path::new(a).is_dir())
        .cloned()
        .unwrap_or_default();
    let p = match carregar(&arquivos) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("pacote inválido:\n  {}", e.join("\n  "));
            std::process::exit(1);
        }
    };
    for m in p.missoes.iter().filter(|m| m.missao.id.contains(&filtro)) {
        let missao = &m.missao;
        println!("\n=== {} ({})", missao.id, missao.titulo);
        let mut lista = vec![missao.decisoes_iniciais()];
        lista.extend(
            missao
                .combinacoes()
                .into_iter()
                .filter(|d| portao(missao, d).is_none()),
        );
        let cen = match rodar(m, "", &lista, &|_| true) {
            Ok(c) => c,
            Err(e) => {
                println!("erro: {e}");
                continue;
            }
        };
        for (i, c) in cen.iter().enumerate() {
            let mut linha = format!(
                "{:>3} {:<60}",
                if i == 0 {
                    "ini".to_string()
                } else {
                    i.to_string()
                },
                descrever(m, &c.decisoes)
            );
            let mut bate = true;
            for mt in &missao.metricas {
                let v = Intervalo::de(&valor_metrica(missao, mt, c).expect("métrica"));
                let s = situacao(mt, &v);
                bate &= s.ok();
                linha.push_str(&format!(
                    " | {} {:.3} ± {:.3}",
                    mt.nome, v.media, v.meia_largura
                ));
            }
            println!("{linha} {}", if bate { "META" } else { "" });
        }
        // A modelling mission asks about the reference model, not the starting one.
        for pv in missao
            .previsao
            .iter()
            .filter(|_| missao.modelagem.is_none())
        {
            if let utsim_ensino_core::missao::Previsao::Valor { id, valor, .. } = pv {
                let v = Intervalo::de(&avaliar(missao, valor, &cen[0]).expect("previsão"));
                println!("previsão {id}: {:.3} ± {:.3}", v.media, v.meia_largura);
            }
            if let utsim_ensino_core::missao::Previsao::Escolha { id, opcoes, .. } = pv {
                let vs: Vec<String> = opcoes
                    .iter()
                    .filter(|o| !o.valor.is_empty())
                    .map(|o| {
                        format!(
                            "{} {:.3}",
                            o.rotulo,
                            Intervalo::de(&avaliar(missao, &o.valor, &cen[0]).expect("x")).media
                        )
                    })
                    .collect();
                println!("previsão {id}: {}", vs.join(" · "));
            }
        }
        match conferir(m) {
            Ok(c) => {
                println!(
                    "conferência: {} combinações, {} viáveis, {} batem a meta; melhor: {}",
                    c.combinacoes,
                    c.viaveis,
                    c.batem_meta,
                    descrever(m, &c.melhor)
                );
                println!("  melhor (ron): {:?}", c.melhor);
                for e in &c.erros {
                    println!("  ERRO: {e}");
                }
                for a in &c.avisos {
                    println!("  aviso: {a}");
                }
            }
            Err(e) => println!("conferência falhou: {e}"),
        }
    }
}
