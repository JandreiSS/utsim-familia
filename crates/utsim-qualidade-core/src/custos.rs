//! Cost of quality (stage QS10 of `docs/plano_qualidade_e_seguranca.md`, section 3.5), in
//! the four usual categories: prevention, appraisal, internal failure and external failure.
//!
//! Two sources:
//! - counts of the model times unit costs written in its blocks: `inspection.cost` per
//!   item inspected (appraisal), `rework.cost` per pass and `quality.unit_cost` of every
//!   process a scrapped item went through (internal failure), `quality.escape_cost` per
//!   defect of that process that reaches the customer (external failure);
//! - entries typed in the app ([`Lancamento`]): training, calibration, warranty, returns.
//!
//! Two scenarios of the same model are compared replication by replication (common random
//! numbers: the same seed and replication numbers on both sides).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_core::ItemRecord;
use utsim_model::quality::present_in;
use utsim_model::{Model, ModelError, ObjectKind, SinkRole};

/// The four categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Categoria {
    Prevencao,
    Avaliacao,
    FalhaInterna,
    FalhaExterna,
}

impl Categoria {
    pub const TODAS: [Categoria; 4] = [
        Categoria::Prevencao,
        Categoria::Avaliacao,
        Categoria::FalhaInterna,
        Categoria::FalhaExterna,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Categoria::Prevencao => "Prevenção",
            Categoria::Avaliacao => "Avaliação",
            Categoria::FalhaInterna => "Falha interna",
            Categoria::FalhaExterna => "Falha externa",
        }
    }
}

/// A cost typed by the user for the period studied.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lancamento {
    #[serde(default)]
    pub id: String,
    pub categoria: Option<Categoria>,
    pub descricao: String,
    pub valor: f64,
    /// Station the cost belongs to (optional).
    #[serde(default)]
    pub posto: String,
}

/// Costs of one replication, from the model.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Custos {
    pub por_categoria: BTreeMap<Categoria, f64>,
    /// By station (object of the model).
    pub por_posto: BTreeMap<String, f64>,
    /// By defect type (scrap and escapes).
    pub por_defeito: BTreeMap<String, f64>,
    /// Counts behind the values.
    pub inspecoes: f64,
    pub passagens_retrabalho: f64,
    pub refugados: f64,
    pub defeitos_escapados: f64,
}

impl Custos {
    pub fn total(&self) -> f64 {
        self.por_categoria.values().sum()
    }

    fn somar(&mut self, cat: Categoria, posto: &str, defeito: Option<&str>, v: f64) {
        if v == 0.0 {
            return;
        }
        *self.por_categoria.entry(cat).or_default() += v;
        *self.por_posto.entry(posto.to_string()).or_default() += v;
        if let Some(d) = defeito {
            *self.por_defeito.entry(d.to_string()).or_default() += v;
        }
    }
}

/// Costs of the items of one replication.
pub fn custos(model: &Model, items: &[ItemRecord]) -> Custos {
    let mut c = Custos::default();
    let refugo: Vec<usize> = model
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o.kind, ObjectKind::Sink) && o.sink_role == Some(SinkRole::Scrap))
        .map(|(i, _)| i)
        .collect();
    let entrega: Vec<usize> = model
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o.kind, ObjectKind::Sink) && o.sink_role != Some(SinkRole::Scrap))
        .map(|(i, _)| i)
        .collect();
    // Origin process of each defect type and its escape cost.
    let mut origem: BTreeMap<&str, (&str, f64)> = BTreeMap::new();
    for o in &model.objects {
        if let Some(q) = &o.quality {
            let e = q.escape_cost.unwrap_or(0.0);
            for d in &q.defects {
                origem.insert(&d.name, (&o.name, e));
            }
            for k in &q.characteristics {
                origem.insert(&k.name, (&o.name, e));
            }
        }
    }
    for item in items {
        for (&b, &n) in &item.visits {
            let Some(o) = model.objects.get(b) else {
                continue;
            };
            let n = f64::from(n);
            if let Some(i) = &o.inspection {
                c.inspecoes += n;
                c.somar(
                    Categoria::Avaliacao,
                    &o.name,
                    None,
                    n * i.cost.unwrap_or(0.0),
                );
            }
            if let Some(r) = &o.rework {
                c.passagens_retrabalho += n;
                c.somar(
                    Categoria::FalhaInterna,
                    &o.name,
                    None,
                    n * r.cost.unwrap_or(0.0),
                );
            }
        }
        let fim = item.ended_at.is_some();
        if fim && item.block.is_some_and(|b| refugo.contains(&b)) {
            c.refugados += 1.0;
            // The defect blamed for the scrap: the first one still present (by name).
            let culpa = present_in(&item.labels).into_iter().next();
            for &b in item.visits.keys() {
                let Some(o) = model.objects.get(b) else {
                    continue;
                };
                if let Some(v) = o.quality.as_ref().and_then(|q| q.unit_cost) {
                    c.somar(Categoria::FalhaInterna, &o.name, culpa.as_deref(), v);
                }
            }
        }
        if fim && item.block.is_some_and(|b| entrega.contains(&b)) {
            for d in present_in(&item.labels) {
                c.defeitos_escapados += 1.0;
                if let Some((posto, e)) = origem.get(d.as_str()) {
                    c.somar(Categoria::FalhaExterna, posto, Some(&d), *e);
                }
            }
        }
    }
    c
}

/// Mean over the replications and half-width of the 95 % interval.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Intervalo {
    pub media: f64,
    pub meia_largura: f64,
}

pub fn intervalo(v: &[f64]) -> Intervalo {
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

/// Cost of quality of a model over its replications, with the typed entries.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelatorioCustos {
    pub modelo: String,
    pub replicacoes: Vec<Custos>,
    /// Model costs by category (mean and interval over the replications).
    pub por_categoria: Vec<(Categoria, Intervalo)>,
    /// Typed entries summed by category (the same for every replication).
    pub lancamentos: BTreeMap<Categoria, f64>,
    /// Mean total: model plus entries.
    pub total: f64,
    pub por_posto: Vec<(String, f64)>,
    pub por_defeito: Vec<(String, f64)>,
}

fn medias(reps: &[Custos], f: impl Fn(&Custos) -> &BTreeMap<String, f64>) -> Vec<(String, f64)> {
    let mut soma: BTreeMap<String, f64> = BTreeMap::new();
    for r in reps {
        for (k, v) in f(r) {
            *soma.entry(k.clone()).or_default() += v / reps.len() as f64;
        }
    }
    let mut v: Vec<(String, f64)> = soma.into_iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// Runs `reps` replications (absent: `run.replications`) with the per-item log.
pub fn rodar(
    model: &Model,
    reps: Option<u32>,
    lancamentos: &[Lancamento],
) -> Result<RelatorioCustos, ModelError> {
    model.validate()?;
    let n = reps.unwrap_or(model.run.replications).max(1);
    let mut replicacoes = Vec::new();
    for r in 0..n {
        let (_, items) = model.run_item_log(r)?;
        replicacoes.push(custos(model, &items));
    }
    let por_categoria: Vec<(Categoria, Intervalo)> = Categoria::TODAS
        .iter()
        .map(|c| {
            let v: Vec<f64> = replicacoes
                .iter()
                .map(|x| x.por_categoria.get(c).copied().unwrap_or(0.0))
                .collect();
            (*c, intervalo(&v))
        })
        .collect();
    let mut manual: BTreeMap<Categoria, f64> = BTreeMap::new();
    for l in lancamentos {
        if let Some(c) = l.categoria {
            *manual.entry(c).or_default() += l.valor;
        }
    }
    let total =
        por_categoria.iter().map(|(_, i)| i.media).sum::<f64>() + manual.values().sum::<f64>();
    Ok(RelatorioCustos {
        modelo: model.name.clone(),
        por_posto: medias(&replicacoes, |c| &c.por_posto),
        por_defeito: medias(&replicacoes, |c| &c.por_defeito),
        replicacoes,
        por_categoria,
        lancamentos: manual,
        total,
    })
}

/// Two scenarios compared replication by replication.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Comparacao {
    pub a: RelatorioCustos,
    pub b: RelatorioCustos,
    /// `b - a` of the model total, per replication, with the interval.
    pub diferenca: Intervalo,
    /// Whether the interval of the difference leaves out zero.
    pub significativa: bool,
}

/// Compares two scenarios with common random numbers. Both models should keep the same
/// seed; the replications are paired by number.
pub fn comparar(
    a: &Model,
    b: &Model,
    reps: Option<u32>,
    lancamentos_a: &[Lancamento],
    lancamentos_b: &[Lancamento],
) -> Result<Comparacao, ModelError> {
    let n = reps.unwrap_or(a.run.replications.min(b.run.replications));
    let ra = rodar(a, Some(n), lancamentos_a)?;
    let rb = rodar(b, Some(n), lancamentos_b)?;
    let manual = |r: &RelatorioCustos| r.lancamentos.values().sum::<f64>();
    let d: Vec<f64> = ra
        .replicacoes
        .iter()
        .zip(&rb.replicacoes)
        .map(|(x, y)| (y.total() + manual(&rb)) - (x.total() + manual(&ra)))
        .collect();
    let diferenca = intervalo(&d);
    let significativa = diferenca.meia_largura.is_finite()
        && (diferenca.media - diferenca.meia_largura > 0.0
            || diferenca.media + diferenca.meia_largura < 0.0);
    Ok(Comparacao {
        a: ra,
        b: rb,
        diferenca,
        significativa,
    })
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn fmt(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.2}").replace('.', ",")
    } else {
        "–".into()
    }
}

/// Self-contained HTML report.
pub fn html(r: &RelatorioCustos, comparacao: Option<&Comparacao>) -> String {
    use std::fmt::Write;
    let mut s = format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>Custo da qualidade: {m}</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:24px;max-width:960px;color:#222}}\
         table{{border-collapse:collapse;margin:8px 0 20px}}td,th{{border:1px solid #ccc;padding:4px 10px;text-align:right}}\
         td:first-child,th:first-child{{text-align:left}}</style></head><body>\
         <h1>Custo da qualidade</h1><p>Modelo <b>{m}</b>, {n} replicações. Custos do modelo de origem \
         <b>Simulado</b>, lançamentos digitados pelo usuário.</p>\
         <table><tr><th>Categoria</th><th>Modelo (média)</th><th>± IC 95 %</th><th>Lançamentos</th><th>Total</th></tr>",
        m = esc(&r.modelo),
        n = r.replicacoes.len()
    );
    for (c, i) in &r.por_categoria {
        let l = r.lancamentos.get(c).copied().unwrap_or(0.0);
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            c.nome(),
            fmt(i.media),
            fmt(i.meia_largura),
            fmt(l),
            fmt(i.media + l)
        );
    }
    let _ = write!(
        s,
        "<tr><th>Total</th><td></td><td></td><td></td><th>{}</th></tr></table>",
        fmt(r.total)
    );
    for (titulo, v) in [
        ("Por posto", &r.por_posto),
        ("Por tipo de defeito", &r.por_defeito),
    ] {
        if v.is_empty() {
            continue;
        }
        let _ = write!(
            s,
            "<h2>{titulo}</h2><table><tr><th></th><th>Média por replicação</th></tr>"
        );
        for (k, x) in v {
            let _ = write!(s, "<tr><td>{}</td><td>{}</td></tr>", esc(k), fmt(*x));
        }
        s.push_str("</table>");
    }
    if let Some(c) = comparacao {
        let _ = write!(
            s,
            "<h2>Comparação de cenários</h2><p>{} contra {}: diferença do total {} ± {} por replicação \
             (números aleatórios comuns). {}</p>",
            esc(&c.b.modelo),
            esc(&c.a.modelo),
            fmt(c.diferenca.media),
            fmt(c.diferenca.meia_largura),
            if c.significativa {
                "A diferença é significativa a 95 %."
            } else {
                "A diferença não é significativa a 95 %."
            }
        );
    }
    s.push_str("</body></html>");
    s
}
