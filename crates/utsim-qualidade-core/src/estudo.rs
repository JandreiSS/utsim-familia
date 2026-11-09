//! Dados simulados para as ferramentas dos estudos (`utsim-estudo`): o que a simulação de
//! um nó oferece a cada ferramenta ([`leituras`]) e a tabela pronta, com a origem
//! Simulado ([`puxar`]).
//!
//! As leituras são frases que a pessoa escolhe na tela:
//!
//! - `defeitos do nó por tipo`, `defeitos do modelo por tipo`, `defeitos por nó` (Pareto e
//!   folha de verificação, esta com uma coluna por replicação);
//! - `característica <nome>` (histograma, capacidade, carta de controle e estratificação,
//!   esta com a replicação como estrato);
//! - `<x> × <y>` (dispersão: duas características medidas na mesma peça).

use std::collections::BTreeMap;
use std::path::Path;

use utsim_estudo::{Ferramenta, Origem, Tabela, num};
use utsim_model::Model;

use crate::simulado::{self, Medicao};

const DEF_NO: &str = "defeitos do nó por tipo";
const DEF_MODELO: &str = "defeitos do modelo por tipo";
const DEF_POR_NO: &str = "defeitos por nó";
const CARAC: &str = "característica ";

/// As características do nó, e as do modelo todo depois.
fn caracteristicas(model: &Model, objeto: &str) -> (Vec<String>, Vec<String>) {
    let mut do_no = Vec::new();
    let mut todas = Vec::new();
    for o in &model.objects {
        if let Some(q) = &o.quality {
            for c in &q.characteristics {
                if o.name == objeto {
                    do_no.push(c.name.clone());
                }
                todas.push(c.name.clone());
            }
        }
    }
    (do_no, todas)
}

fn tipos_do_no(model: &Model, objeto: &str) -> Vec<String> {
    model
        .objects
        .iter()
        .filter(|o| o.name == objeto)
        .filter_map(|o| o.quality.as_ref())
        .flat_map(|q| q.defects.iter().map(|d| d.name.clone()))
        .collect()
}

/// O que a simulação oferece à ferramenta neste nó (vazio quando não oferece nada).
pub fn leituras(model: &Model, objeto: &str, f: Ferramenta) -> Vec<String> {
    let (do_no, todas) = caracteristicas(model, objeto);
    let tem_defeito = model
        .objects
        .iter()
        .any(|o| o.quality.as_ref().is_some_and(|q| !q.defects.is_empty()));
    let defeitos = || {
        let mut v = Vec::new();
        if !tipos_do_no(model, objeto).is_empty() {
            v.push(DEF_NO.to_string());
        }
        if tem_defeito {
            v.push(DEF_MODELO.to_string());
            v.push(DEF_POR_NO.to_string());
        }
        v
    };
    // As do nó primeiro; sem nenhuma no nó, as do modelo.
    let caracs = || {
        let base = if do_no.is_empty() { &todas } else { &do_no };
        base.iter()
            .map(|c| format!("{CARAC}{c}"))
            .collect::<Vec<_>>()
    };
    match f {
        Ferramenta::Pareto | Ferramenta::FolhaVerificacao => defeitos(),
        Ferramenta::Histograma
        | Ferramenta::Capacidade
        | Ferramenta::CartaControle
        | Ferramenta::Estratificacao => caracs(),
        Ferramenta::Dispersao => {
            let mut v = Vec::new();
            for (i, x) in todas.iter().enumerate() {
                for y in &todas[i + 1..] {
                    if do_no.contains(x) || do_no.contains(y) || do_no.is_empty() {
                        v.push(format!("{x} × {y}"));
                    }
                }
            }
            v
        }
        _ => Vec::new(),
    }
}

fn arquivo(modelo: &str) -> String {
    Path::new(modelo)
        .file_name()
        .map_or_else(|| modelo.to_string(), |f| f.to_string_lossy().into_owned())
}

fn origem(modelo: &str, objeto: &str, leitura: &str, reps: u32) -> Origem {
    Origem::Simulado {
        modelo: arquivo(modelo),
        objeto: objeto.into(),
        leitura: leitura.into(),
        replicacoes: reps,
        quando: utsim_estudo::hoje(),
    }
}

/// Defeitos por replicação e por categoria (tipo ou nó), na ordem do modelo.
fn defeitos(
    model: &Model,
    reps: u32,
    objeto: &str,
    leitura: &str,
) -> Result<(Vec<String>, Vec<Vec<f64>>), String> {
    let do_no = tipos_do_no(model, objeto);
    let mut categorias: Vec<String> = Vec::new();
    let mut por_rep: Vec<BTreeMap<String, f64>> = Vec::new();
    for r in 0..reps {
        let (_, items) = model.run_item_log(r).map_err(|e| e.to_string())?;
        let k = simulado::de_itens(model, &items);
        let mapa = if leitura == DEF_POR_NO {
            k.defeitos_por_posto
        } else {
            k.defeitos_por_tipo
                .into_iter()
                .filter(|(t, _)| leitura != DEF_NO || do_no.contains(t))
                .collect()
        };
        for c in mapa.keys() {
            if !categorias.contains(c) {
                categorias.push(c.clone());
            }
        }
        por_rep.push(mapa);
    }
    if leitura == DEF_NO {
        // A ordem dos tipos no nó, mesmo os que não apareceram.
        categorias = do_no;
    }
    let linhas = categorias
        .iter()
        .map(|c| {
            por_rep
                .iter()
                .map(|m| m.get(c).copied().unwrap_or(0.0))
                .collect()
        })
        .collect();
    Ok((categorias, linhas))
}

/// Roda a simulação e devolve a tabela da ferramenta com a origem Simulado. `modelo` é o
/// arquivo (vai na origem), `leitura` uma das [`leituras`].
pub fn puxar(
    model: &Model,
    modelo: &str,
    objeto: &str,
    f: Ferramenta,
    leitura: &str,
    reps: u32,
) -> Result<(Tabela, Origem), String> {
    let reps = reps.max(1);
    model.validate().map_err(|e| e.to_string())?;
    if !leituras(model, objeto, f).iter().any(|l| l == leitura) {
        return Err(format!(
            "a simulação não oferece \"{leitura}\" para {} no nó {objeto}",
            f.nome()
        ));
    }
    let mut t = Tabela::default();
    if leitura == DEF_NO || leitura == DEF_MODELO || leitura == DEF_POR_NO {
        let (cats, linhas) = defeitos(model, reps, objeto, leitura)?;
        let rotulo = if leitura == DEF_POR_NO {
            "Nó"
        } else {
            "Tipo de defeito"
        };
        if f == Ferramenta::FolhaVerificacao {
            t.colunas = std::iter::once(rotulo.to_string())
                .chain((1..=reps).map(|r| format!("Replicação {r}")))
                .collect();
            t.linhas = cats
                .iter()
                .zip(&linhas)
                .map(|(c, v)| {
                    std::iter::once(c.clone())
                        .chain(v.iter().map(|x| num(*x, 0)))
                        .collect()
                })
                .collect();
        } else {
            t.colunas = vec![rotulo.into(), "Quantidade por replicação".into()];
            t.linhas = cats
                .iter()
                .zip(&linhas)
                .map(|(c, v)| vec![c.clone(), num(v.iter().sum::<f64>() / reps as f64, 2)])
                .collect();
        }
        return Ok((t, origem(modelo, objeto, leitura, reps)));
    }
    let rel = simulado::rodar(model, Some(reps)).map_err(|e| e.to_string())?;
    let de = |c: &str| -> Vec<&Medicao> {
        rel.medicoes
            .iter()
            .filter(|m| m.caracteristica == c)
            .collect()
    };
    if let Some((x, y)) = leitura.split_once(" × ") {
        let ys: BTreeMap<(u32, u64), f64> = de(y)
            .iter()
            .map(|m| ((m.replicacao, m.item), m.valor))
            .collect();
        t.colunas = vec![x.into(), y.into()];
        t.linhas = de(x)
            .iter()
            .filter_map(|m| {
                ys.get(&(m.replicacao, m.item))
                    .map(|v| vec![num(m.valor, 6), num(*v, 6)])
            })
            .collect();
        if t.linhas.is_empty() {
            return Err(format!("nenhuma peça foi medida em {x} e em {y}"));
        }
        return Ok((t, origem(modelo, objeto, leitura, reps)));
    }
    let c = leitura.strip_prefix(CARAC).unwrap_or(leitura);
    let ms = de(c);
    if ms.is_empty() {
        return Err(format!("nenhuma medição de {c} na simulação"));
    }
    match f {
        Ferramenta::Estratificacao => {
            t.colunas = vec!["Replicação".into(), c.into()];
            t.linhas = ms
                .iter()
                .map(|m| vec![(m.replicacao + 1).to_string(), num(m.valor, 6)])
                .collect();
        }
        Ferramenta::CartaControle => {
            // A ordem do tempo da primeira replicação: a carta não mistura replicações.
            t.colunas = vec!["Peça".into(), c.into()];
            t.linhas = ms
                .iter()
                .filter(|m| m.replicacao == 0)
                .map(|m| vec![m.item.to_string(), num(m.valor, 6)])
                .collect();
        }
        _ => {
            t.colunas = vec![c.into()];
            t.linhas = ms.iter().map(|m| vec![num(m.valor, 6)]).collect();
        }
    }
    let leitura_txt = if f == Ferramenta::CartaControle && reps > 1 {
        format!("{leitura}, ordem do tempo da replicação 1")
    } else {
        leitura.to_string()
    };
    Ok((t, origem(modelo, objeto, &leitura_txt, reps)))
}

/// Os limites de especificação de uma característica do modelo (para preencher LIE e LSE
/// quando a ferramenta puxa a característica).
pub fn limites(model: &Model, caracteristica: &str) -> (Option<f64>, Option<f64>) {
    model
        .objects
        .iter()
        .filter_map(|o| o.quality.as_ref())
        .flat_map(|q| q.characteristics.iter())
        .find(|c| c.name == caracteristica)
        .map_or((None, None), |c| (c.lsl, c.usl))
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_estudo::calculo::calcular;
    use utsim_estudo::{Aplicacao, Ferramenta};

    fn modelo() -> Model {
        Model::load(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/inspecao_e_retrabalho.ron"
        ))
        .unwrap()
    }

    #[test]
    fn readings_per_tool_and_tables_that_calculate() {
        let m = modelo();
        assert_eq!(
            leituras(&m, "torno", Ferramenta::Pareto),
            [DEF_NO, DEF_MODELO, DEF_POR_NO]
        );
        assert_eq!(
            leituras(&m, "torno", Ferramenta::Capacidade),
            ["característica diametro"]
        );
        assert!(leituras(&m, "torno", Ferramenta::Ishikawa).is_empty());
        for (f, l) in [
            (Ferramenta::Pareto, DEF_NO),
            (Ferramenta::FolhaVerificacao, DEF_MODELO),
            (Ferramenta::Histograma, "característica diametro"),
            (Ferramenta::Capacidade, "característica diametro"),
            (Ferramenta::CartaControle, "característica diametro"),
            (Ferramenta::Estratificacao, "característica diametro"),
        ] {
            let (t, o) = puxar(&m, "modelos/inspecao_e_retrabalho.ron", "torno", f, l, 2).unwrap();
            assert!(!t.vazia(), "{f:?}");
            let Origem::Simulado {
                modelo,
                replicacoes,
                ..
            } = &o
            else {
                panic!()
            };
            assert_eq!(modelo, "inspecao_e_retrabalho.ron");
            assert_eq!(*replicacoes, 2);
            let mut a = Aplicacao::nova("a1", f);
            a.tabela = t;
            a.origem = o;
            if matches!(f, Ferramenta::Capacidade | Ferramenta::Histograma) {
                let (lie, lse) = limites(&m, "diametro");
                a.parametros.lie = lie;
                a.parametros.lse = lse;
            }
            let r = calcular(&a, "");
            assert!(!r.vazio(), "{f:?}: {:?}", r.avisos);
        }
        assert!(puxar(&m, "x.ron", "torno", Ferramenta::Pareto, "outra coisa", 1).is_err());
    }
}
