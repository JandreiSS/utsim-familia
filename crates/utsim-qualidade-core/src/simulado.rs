//! Quality indicators of a model with `quality`, `inspection` and `rework` blocks, per
//! replication and over the replications with a 95 % interval.
//!
//! Every figure comes from the per-item log of the kernel (`Model::run_item_log`): the
//! labels an item carries at the end (`defeito.<tipo>`, `defeitos`, `passagens`,
//! characteristics) and the sink where it ended.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_core::{ItemRecord, Value};
use utsim_model::quality::{DEFECT_PREFIX, DEFECTS_LABEL, INSTANT_SUFFIX, PASSES_LABEL};
use utsim_model::{Model, ModelError, ObjectKind, SinkRole};
use utsim_quality::Spec;
use utsim_quality::capability::{self, Capability};

/// Indicators of one replication.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Indicadores {
    /// Items that ended in a sink (customer or scrap).
    pub concluidos: f64,
    pub entregues: f64,
    pub refugados: f64,
    /// Items that went through rework at least once.
    pub retrabalhados: f64,
    /// Delivered items still carrying a defect (external failure).
    pub escapados: f64,
    /// Delivered without rework over finished: first-pass yield.
    pub rendimento_primeira: f64,
    /// Product of the first-pass yields of the inspection stations.
    pub rendimento_acumulado: f64,
    pub pct_refugo: f64,
    pub pct_retrabalho: f64,
    /// Defects that appeared, per finished item.
    pub dpu: f64,
    pub dpmo: f64,
    /// Escaped items over delivered ones.
    pub pct_escape: f64,
    /// Defective items caught (reworked or scrapped) over defective items.
    pub eficacia_inspecao: f64,
    /// Defects that appeared, by type.
    pub defeitos_por_tipo: BTreeMap<String, f64>,
    /// Defects that appeared, by the object that generates the type.
    pub defeitos_por_posto: BTreeMap<String, f64>,
    /// First-pass yield of each inspection station (items inspected only once and
    /// delivered, over items inspected).
    pub rendimento_por_inspecao: BTreeMap<String, f64>,
}

/// A measured value of a characteristic (long format).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Medicao {
    pub replicacao: u32,
    pub item: u64,
    pub objeto: String,
    pub caracteristica: String,
    pub instante: f64,
    pub valor: f64,
}

/// One replication.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Replicacao {
    pub replicacao: u32,
    pub indicadores: Indicadores,
}

/// Mean over the replications and half-width of the 95 % interval.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Intervalo {
    pub media: f64,
    pub meia_largura: f64,
}

/// Capability of a simulated characteristic, all replications together.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CapacidadeSimulada {
    pub caracteristica: String,
    pub objeto: String,
    pub n: usize,
    pub capacidade: Option<Capability>,
    pub erro: Option<String>,
}

/// Everything the quality report shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelatorioQualidade {
    pub modelo: String,
    pub replicacoes: Vec<Replicacao>,
    /// Indicator name → interval (only the scalar indicators).
    pub resumo: Vec<(String, Intervalo)>,
    /// Defect types (or stations) with the mean count per replication, largest first.
    pub pareto_tipo: Vec<(String, f64)>,
    pub pareto_posto: Vec<(String, f64)>,
    pub medicoes: Vec<Medicao>,
    pub capacidades: Vec<CapacidadeSimulada>,
    /// Defect opportunities per unit used in the DPMO.
    pub oportunidades: usize,
}

/// Which object generates each defect type and characteristic, and its limits.
struct Mapa {
    origem: BTreeMap<String, String>,
    specs: BTreeMap<String, Spec>,
    oportunidades: usize,
    entrega: Vec<usize>,
    refugo: Vec<usize>,
    inspecoes: Vec<(usize, String)>,
}

fn mapa(model: &Model) -> Mapa {
    let mut origem = BTreeMap::new();
    let mut specs = BTreeMap::new();
    let mut entrega = Vec::new();
    let mut refugo = Vec::new();
    let mut inspecoes = Vec::new();
    for (i, o) in model.objects.iter().enumerate() {
        if let Some(q) = &o.quality {
            for d in &q.defects {
                origem.insert(d.name.clone(), o.name.clone());
            }
            for c in &q.characteristics {
                origem.insert(c.name.clone(), o.name.clone());
                specs.insert(
                    c.name.clone(),
                    Spec {
                        lsl: c.lsl,
                        usl: c.usl,
                        target: None,
                    },
                );
            }
        }
        if o.inspection.is_some() {
            inspecoes.push((i, o.name.clone()));
        }
        if matches!(o.kind, ObjectKind::Sink) {
            match o.sink_role {
                Some(SinkRole::Scrap) => refugo.push(i),
                _ => entrega.push(i),
            }
        }
    }
    let oportunidades = origem.len().max(1);
    Mapa {
        origem,
        specs,
        oportunidades,
        entrega,
        refugo,
        inspecoes,
    }
}

fn num(r: &ItemRecord, label: &str) -> f64 {
    r.labels.get(label).and_then(Value::as_num).unwrap_or(0.0)
}

/// Defect types that appeared on the item (present or fixed later).
fn surgidos(r: &ItemRecord) -> impl Iterator<Item = &str> {
    r.labels
        .keys()
        .filter_map(|k| k.strip_prefix(DEFECT_PREFIX))
}

fn ratio(a: f64, b: f64) -> f64 {
    if b > 0.0 { a / b } else { 0.0 }
}

fn indicadores(m: &Mapa, items: &[ItemRecord]) -> Indicadores {
    let mut k = Indicadores::default();
    let fim = |r: &&ItemRecord, set: &[usize]| {
        r.ended_at.is_some() && r.block.is_some_and(|b| set.contains(&b))
    };
    let entregues: Vec<&ItemRecord> = items.iter().filter(|r| fim(r, &m.entrega)).collect();
    let refugados: Vec<&ItemRecord> = items.iter().filter(|r| fim(r, &m.refugo)).collect();
    let concluidos: Vec<&ItemRecord> = entregues.iter().chain(&refugados).copied().collect();
    k.entregues = entregues.len() as f64;
    k.refugados = refugados.len() as f64;
    k.concluidos = concluidos.len() as f64;
    k.retrabalhados = concluidos
        .iter()
        .filter(|r| num(r, PASSES_LABEL) > 0.0)
        .count() as f64;
    k.escapados = entregues
        .iter()
        .filter(|r| num(r, DEFECTS_LABEL) > 0.0)
        .count() as f64;
    let primeira = entregues
        .iter()
        .filter(|r| num(r, PASSES_LABEL) == 0.0)
        .count() as f64;
    k.rendimento_primeira = ratio(primeira, k.concluidos);
    k.pct_refugo = ratio(k.refugados, k.concluidos);
    k.pct_retrabalho = ratio(k.retrabalhados, k.concluidos);
    k.pct_escape = ratio(k.escapados, k.entregues);
    let mut total = 0.0;
    let mut defeituosos = 0.0;
    let mut pegos = 0.0;
    for r in &concluidos {
        let mut algum = false;
        for d in surgidos(r) {
            algum = true;
            total += 1.0;
            *k.defeitos_por_tipo.entry(d.to_string()).or_default() += 1.0;
            let posto = m.origem.get(d).cloned().unwrap_or_else(|| "?".into());
            *k.defeitos_por_posto.entry(posto).or_default() += 1.0;
        }
        if algum {
            defeituosos += 1.0;
            if num(r, PASSES_LABEL) > 0.0 || refugados.iter().any(|x| std::ptr::eq(*x, *r)) {
                pegos += 1.0;
            }
        }
    }
    k.dpu = ratio(total, k.concluidos);
    k.dpmo = utsim_quality::capability::dpmo(total, k.concluidos.max(1.0), m.oportunidades as f64);
    if k.concluidos == 0.0 {
        k.dpmo = 0.0;
    }
    k.eficacia_inspecao = ratio(pegos, defeituosos);
    let mut produto = 1.0;
    for (i, nome) in &m.inspecoes {
        let vistos: Vec<&&ItemRecord> = concluidos
            .iter()
            .filter(|r| r.visits.get(i).copied().unwrap_or(0) > 0)
            .collect();
        let uma = vistos
            .iter()
            .filter(|r| r.visits.get(i).copied() == Some(1) && fim(r, &m.entrega))
            .count() as f64;
        let y = ratio(uma, vistos.len() as f64);
        produto *= y;
        k.rendimento_por_inspecao.insert(nome.clone(), y);
    }
    k.rendimento_acumulado = if m.inspecoes.is_empty() {
        k.rendimento_primeira
    } else {
        produto
    };
    k
}

fn intervalo(v: &[f64]) -> Intervalo {
    let n = v.len();
    let media = v.iter().sum::<f64>() / n.max(1) as f64;
    let meia_largura = if n > 1 {
        let var = v.iter().map(|x| (x - media).powi(2)).sum::<f64>() / (n - 1) as f64;
        utsim_core::stats::t_975(n - 1) * (var / n as f64).sqrt()
    } else {
        f64::NAN
    };
    Intervalo {
        media,
        meia_largura,
    }
}

/// Indicators listed in the summary, with their names in the report.
pub const RESUMO: &[&str] = &[
    "Concluídos",
    "Entregues",
    "Refugados",
    "Rendimento de primeira passagem",
    "Rendimento acumulado",
    "% refugo",
    "% retrabalho",
    "% escape (defeitos no cliente)",
    "Defeitos por unidade",
    "DPMO",
    "Eficácia da inspeção",
];

fn escalares(k: &Indicadores) -> [f64; 11] {
    [
        k.concluidos,
        k.entregues,
        k.refugados,
        k.rendimento_primeira,
        k.rendimento_acumulado,
        k.pct_refugo,
        k.pct_retrabalho,
        k.pct_escape,
        k.dpu,
        k.dpmo,
        k.eficacia_inspecao,
    ]
}

/// Runs `reps` replications (absent: `run.replications`) with the per-item log and
/// computes the indicators, the Pareto charts, the long output of the characteristics and
/// their capability.
pub fn rodar(model: &Model, reps: Option<u32>) -> Result<RelatorioQualidade, ModelError> {
    model.validate()?;
    let m = mapa(model);
    let n = reps.unwrap_or(model.run.replications).max(1);
    let mut replicacoes = Vec::new();
    let mut medicoes = Vec::new();
    for r in 0..n {
        let (_, items) = model.run_item_log(r)?;
        for item in &items {
            if item.ended_at.is_none() {
                continue;
            }
            for (c, objeto) in &m.origem {
                if !m.specs.contains_key(c) {
                    continue;
                }
                if let Some(Value::Num(v)) = item.labels.get(c) {
                    medicoes.push(Medicao {
                        replicacao: r,
                        item: item.key,
                        objeto: objeto.clone(),
                        caracteristica: c.clone(),
                        instante: num(item, &format!("{c}{INSTANT_SUFFIX}")),
                        valor: *v,
                    });
                }
            }
        }
        replicacoes.push(Replicacao {
            replicacao: r,
            indicadores: indicadores(&m, &items),
        });
    }
    medicoes.sort_by(|a, b| {
        (a.replicacao, a.caracteristica.as_str())
            .cmp(&(b.replicacao, b.caracteristica.as_str()))
            .then(a.instante.total_cmp(&b.instante))
    });
    let valores: Vec<[f64; 11]> = replicacoes
        .iter()
        .map(|r| escalares(&r.indicadores))
        .collect();
    let resumo = RESUMO
        .iter()
        .enumerate()
        .map(|(j, nome)| {
            let v: Vec<f64> = valores.iter().map(|x| x[j]).collect();
            (nome.to_string(), intervalo(&v))
        })
        .collect();
    let pareto = |f: &dyn Fn(&Indicadores) -> &BTreeMap<String, f64>| {
        let mut soma: BTreeMap<String, f64> = BTreeMap::new();
        for r in &replicacoes {
            for (k, v) in f(&r.indicadores) {
                *soma.entry(k.clone()).or_default() += v / n as f64;
            }
        }
        let mut v: Vec<(String, f64)> = soma.into_iter().collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    };
    let pareto_tipo = pareto(&|k| &k.defeitos_por_tipo);
    let pareto_posto = pareto(&|k| &k.defeitos_por_posto);
    let capacidades = m
        .specs
        .iter()
        .map(|(c, spec)| {
            let v: Vec<Vec<f64>> = medicoes
                .iter()
                .filter(|x| &x.caracteristica == c)
                .map(|x| vec![x.valor])
                .collect();
            let res = capability::capability(&v, spec, &capability::Options::default());
            CapacidadeSimulada {
                caracteristica: c.clone(),
                objeto: m.origem.get(c).cloned().unwrap_or_default(),
                n: v.len(),
                erro: res.as_ref().err().cloned(),
                capacidade: res.ok(),
            }
        })
        .collect();
    Ok(RelatorioQualidade {
        modelo: model.name.clone(),
        replicacoes,
        resumo,
        pareto_tipo,
        pareto_posto,
        medicoes,
        capacidades,
        oportunidades: m.oportunidades,
    })
}

/// Most replications [`capacidade_para_troca`] runs.
pub const REPLICACOES_TROCA: u32 = 30;

/// The simulated capability of every characteristic of the model, as the `dados` of the
/// `capacidade_simulada` result (format 1) other apps read from the response: model name,
/// replications and, per characteristic, `nome`, `objeto`, `n`, `lie`, `lse`, `media`,
/// `desvio`, `cp`, `cpk`, `cpk_lo`, `cpk_hi`, `pp`, `ppk`, `ppm` and `erro` (absent values are
/// null). `reps` absent: the model's own replications; capped at [`REPLICACOES_TROCA`].
pub fn capacidade_para_troca(
    model: &Model,
    reps: Option<u32>,
) -> Result<serde_json::Value, ModelError> {
    use serde_json::json;
    let n = reps
        .unwrap_or(model.run.replications)
        .clamp(1, REPLICACOES_TROCA);
    let rel = rodar(model, Some(n))?;
    let caracteristicas: Vec<_> = rel
        .capacidades
        .iter()
        .map(|c| {
            let k = c.capacidade.as_ref();
            json!({
                "nome": c.caracteristica,
                "objeto": c.objeto,
                "n": c.n,
                "lie": k.and_then(|k| k.spec.lsl),
                "lse": k.and_then(|k| k.spec.usl),
                "media": k.map(|k| k.mean),
                "desvio": k.map(|k| k.sigma_overall),
                "cp": k.and_then(|k| k.cp.as_ref()).map(|i| i.value),
                "cpk": k.map(|k| k.cpk.value),
                "cpk_lo": k.map(|k| k.cpk.lo),
                "cpk_hi": k.map(|k| k.cpk.hi),
                "pp": k.and_then(|k| k.pp.as_ref()).map(|i| i.value),
                "ppk": k.map(|k| k.ppk.value),
                "ppm": k.map(|k| k.ppm_overall.total),
                "erro": c.erro,
            })
        })
        .collect();
    Ok(json!({
        "modelo": rel.modelo,
        "replicacoes": n,
        "caracteristicas": caracteristicas,
    }))
}

/// The long output as CSV (`replicacao,item,objeto,caracteristica,instante,valor`), the
/// format `utsim qualidade` reads.
pub fn csv_medicoes(medicoes: &[Medicao]) -> String {
    let mut out = String::from("replicacao,item,objeto,caracteristica,instante,valor\n");
    for m in medicoes {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            m.replicacao, m.item, m.objeto, m.caracteristica, m.instante, m.valor
        ));
    }
    out
}

/// Values of one characteristic in one replication, in time order: a column for
/// `utsim qualidade carta` and `capacidade`.
pub fn coluna(medicoes: &[Medicao], caracteristica: &str, replicacao: u32) -> Vec<f64> {
    medicoes
        .iter()
        .filter(|m| m.caracteristica == caracteristica && m.replicacao == replicacao)
        .map(|m| m.valor)
        .collect()
}

/// Indicators of one replication from its per-item log (the same as [`rodar`] computes).
pub fn de_itens(model: &Model, items: &[ItemRecord]) -> Indicadores {
    indicadores(&mapa(model), items)
}

/// Quality and cost of quality of one replication as flat keys, for the teaching missions
/// and the experiments: `qualidade.<indicador>` and `custo_qualidade.<categoria>` (and
/// `custo_qualidade.total`), plus `qualidade.defeitos.<tipo>` per defect type.
pub fn metricas(model: &Model, items: &[ItemRecord]) -> BTreeMap<String, f64> {
    let k = de_itens(model, items);
    let mut out = BTreeMap::new();
    for (nome, v) in [
        ("concluidos", k.concluidos),
        ("entregues", k.entregues),
        ("refugados", k.refugados),
        ("retrabalhados", k.retrabalhados),
        ("escapados", k.escapados),
        ("rendimento_primeira", k.rendimento_primeira),
        ("rendimento_acumulado", k.rendimento_acumulado),
        ("pct_refugo", k.pct_refugo),
        ("pct_retrabalho", k.pct_retrabalho),
        ("pct_escape", k.pct_escape),
        ("dpu", k.dpu),
        ("dpmo", k.dpmo),
        ("eficacia_inspecao", k.eficacia_inspecao),
    ] {
        out.insert(format!("qualidade.{nome}"), v);
    }
    for (t, v) in &k.defeitos_por_tipo {
        out.insert(format!("qualidade.defeitos.{t}"), *v);
    }
    let c = crate::custos::custos(model, items);
    for cat in crate::custos::Categoria::TODAS {
        let chave = match cat {
            crate::custos::Categoria::Prevencao => "prevencao",
            crate::custos::Categoria::Avaliacao => "avaliacao",
            crate::custos::Categoria::FalhaInterna => "falha_interna",
            crate::custos::Categoria::FalhaExterna => "falha_externa",
        };
        out.insert(
            format!("custo_qualidade.{chave}"),
            c.por_categoria.get(&cat).copied().unwrap_or(0.0),
        );
    }
    out.insert("custo_qualidade.total".into(), c.total());
    out
}
