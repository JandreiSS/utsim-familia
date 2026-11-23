//! Permanent control and inspection plans per work station (stage QS11 of
//! `docs/plano_qualidade_e_seguranca.md`, section 3.3), kept in the unit folder
//! (collections [`CONTROLE`] and [`INSPECAO`]) and suggested from a model: one control line
//! per measured characteristic with limits, one inspection line per inspection station.

use serde::{Deserialize, Serialize};
use utsim_model::{Model, RejectTo};

use crate::amostragem::Plano;

pub const CONTROLE: &str = "plano_controle";
pub const INSPECAO: &str = "plano_inspecao";

/// One line of the control plan.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinhaControle {
    #[serde(default)]
    pub id: String,
    /// Work station (id in the unit).
    #[serde(default)]
    pub posto: String,
    pub caracteristica: String,
    #[serde(default)]
    pub lie: Option<f64>,
    #[serde(default)]
    pub lse: Option<f64>,
    #[serde(default)]
    pub alvo: Option<f64>,
    #[serde(default)]
    pub unidade: String,
    /// Measurement method and instrument.
    #[serde(default)]
    pub metodo: String,
    /// Whether the measurement system passed its study (MSA); `None`: not studied.
    #[serde(default)]
    pub msa_aprovado: Option<bool>,
    /// Standard deviation of the measurement system, when known (goes to `measure.sigma`).
    #[serde(default)]
    pub sigma_medicao: Option<f64>,
    #[serde(default)]
    pub frequencia: String,
    #[serde(default)]
    pub tamanho_amostra: u32,
    /// Control chart used (`imr`, `xbarr`, `p`...), as `utsim qualidade carta` names them.
    #[serde(default)]
    pub carta: String,
    /// What to do on a signal.
    #[serde(default)]
    pub reacao: String,
    /// Line of the FMEA this control covers (free reference).
    #[serde(default)]
    pub linha_fmea: String,
    /// Object and characteristic of a model this line came from.
    #[serde(default)]
    pub objeto_modelo: Option<String>,
}

/// The control charts a control plan line can name, as `utsim qualidade carta` and the
/// tools read them (`utsim_quality::charts::ChartKind::parse`).
pub const CARTAS: [&str; 7] = ["imr", "xbarr", "xbars", "p", "np", "c", "u"];

/// Whether `carta` is one of [`CARTAS`] (empty counts: no chart chosen).
pub fn carta_valida(carta: &str) -> bool {
    carta.is_empty() || CARTAS.contains(&carta)
}

/// One line of the inspection plan.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinhaInspecao {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub posto: String,
    /// What is inspected.
    pub o_que: String,
    #[serde(default)]
    pub como: String,
    #[serde(default)]
    pub quando: String,
    #[serde(default)]
    pub quem: String,
    /// Sampling plan by attributes, when there is one.
    #[serde(default)]
    pub amostragem: Option<Plano>,
    /// Lot size the plan applies to.
    #[serde(default)]
    pub lote: Option<u64>,
    /// Inspection station of the model this line came from or is tested on.
    #[serde(default)]
    pub estacao_modelo: Option<String>,
}

/// Lines suggested from a model, linked to the stations of the unit by object name
/// (`posto_de(objeto)` returns the station id).
pub fn sugerir(
    model: &Model,
    posto_de: impl Fn(&str) -> Option<String>,
) -> (Vec<LinhaControle>, Vec<LinhaInspecao>) {
    let mut controle = Vec::new();
    let mut inspecao = Vec::new();
    for o in &model.objects {
        if let Some(q) = &o.quality {
            for c in q
                .characteristics
                .iter()
                .filter(|c| c.lsl.is_some() || c.usl.is_some())
            {
                controle.push(LinhaControle {
                    posto: posto_de(&o.name).unwrap_or_default(),
                    caracteristica: c.name.clone(),
                    lie: c.lsl,
                    lse: c.usl,
                    carta: "imr".into(),
                    tamanho_amostra: 1,
                    objeto_modelo: Some(o.name.clone()),
                    ..LinhaControle::default()
                });
            }
        }
        if let Some(i) = &o.inspection {
            let medidas: Vec<&str> = i
                .measure
                .iter()
                .map(|m| m.characteristic.as_str())
                .collect();
            inspecao.push(LinhaInspecao {
                posto: posto_de(&o.name).unwrap_or_default(),
                o_que: if medidas.is_empty() {
                    "defeitos visuais".into()
                } else {
                    format!("medição de {}", medidas.join(", "))
                },
                como: match i.on_reject {
                    RejectTo::Rework => "rejeitada vai para o retrabalho".into(),
                    RejectTo::Scrap => "rejeitada vai para o refugo".into(),
                },
                quando: if i.every <= 1 {
                    "todas as peças".into()
                } else {
                    format!("uma peça a cada {}", i.every)
                },
                estacao_modelo: Some(o.name.clone()),
                ..LinhaInspecao::default()
            });
        }
    }
    (controle, inspecao)
}

/// The control line applied back to the model: the measurement system deviation of the
/// line goes to the `measure.sigma` of every inspection that measures the characteristic.
/// Returns how many inspections changed.
pub fn aplicar_sigma(model: &mut Model, linha: &LinhaControle) -> usize {
    let Some(s) = linha.sigma_medicao else {
        return 0;
    };
    let mut n = 0;
    for o in &mut model.objects {
        if let Some(i) = o.inspection.as_deref_mut() {
            for m in i
                .measure
                .iter_mut()
                .filter(|m| m.characteristic == linha.caracteristica)
            {
                m.sigma = s;
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_from_the_inspection_example() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/inspecao_e_retrabalho.ron");
        let mut model = Model::load(&p).unwrap();
        let (c, i) = sugerir(&model, |o| Some(format!("posto-{o}")));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].caracteristica, "diametro");
        assert_eq!(c[0].posto, "posto-torno");
        assert_eq!((c[0].lie, c[0].lse), (Some(24.95), Some(25.05)));
        assert_eq!(i.len(), 1);
        assert!(i[0].o_que.contains("diametro"));
        let mut l = c[0].clone();
        l.sigma_medicao = Some(0.002);
        assert_eq!(aplicar_sigma(&mut model, &l), 1);
        assert_eq!(
            model.objects[3].inspection.as_ref().unwrap().measure[0].sigma,
            0.002
        );
    }
}

#[cfg(test)]
mod testes_cartas {
    use super::*;

    #[test]
    fn every_chart_of_the_list_is_read_by_the_charts() {
        for c in CARTAS {
            assert!(utsim_quality::charts::ChartKind::parse(c).is_some(), "{c}");
            assert!(carta_valida(c));
        }
        assert!(carta_valida(""));
        assert!(!carta_valida("x-barra"));
    }
}
