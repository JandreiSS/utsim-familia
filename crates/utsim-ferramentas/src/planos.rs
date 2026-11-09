//! The plans, without any project state: data collection plan (a measure and what it lacks
//! to be collected the same way by everyone), control plan (a characteristic under
//! control, linked to an FMEA row) and response or reaction plan (a signal with the
//! immediate action, the containment and who to call, turned into a task). Field names as
//! saved by the apps.

/// A measure of the data collection plan.
pub const COLLECTION_ITEM: &str = "medida";
/// A characteristic of the control plan.
pub const CONTROL_ITEM: &str = "caracteristica";
/// A signal of the response plan.
pub const RESPONSE_ITEM: &str = "sinal";

pub mod fields {
    // Data collection plan.
    /// `y` or `x`.
    pub const ROLE: &str = "papel";
    pub const NAME: &str = "nome";
    pub const DEFINITION: &str = "definicao_operacional";
    /// `continuo`, `atributo` or `contagem`.
    pub const DATA_TYPE: &str = "tipo_dado";
    pub const UNIT: &str = "unidade";
    pub const STRATIFY: &str = "estratificacao";
    pub const SAMPLE_SIZE: &str = "tamanho_amostra";
    pub const FREQUENCY: &str = "frequencia";
    pub const SUBGROUP: &str = "subgrupo";
    pub const WHO: &str = "quem";
    pub const WHERE: &str = "onde";
    pub const FORM: &str = "formulario";

    // Control plan.
    pub const PROCESS: &str = "processo";
    pub const CHARACTERISTIC: &str = "caracteristica";
    /// `produto` or `processo`.
    pub const CHAR_KIND: &str = "tipo_caracteristica";
    pub const SPEC: &str = "especificacao";
    pub const METHOD: &str = "metodo_medicao";
    pub const CONTROL: &str = "metodo_controle";
    pub const OWNER: &str = "responsavel";
    pub const REACTION: &str = "plano_reacao";
    /// The FMEA row this line controls.
    pub const FMEA_ROW: &str = "linha_fmea";
    pub const CRITICAL: &str = "critica";
    /// Where the characteristic is read in operation: `kpi:<id>` (a KPI of the twin
    /// panel, per window) or `tag:<tag>` (an observation of the twin).
    pub const SOURCE: &str = "fonte";
    /// Frozen phase II limits: centre, lower and upper.
    pub const CENTER: &str = "lc";
    pub const LCL: &str = "lic";
    pub const UCL: &str = "lsc";
    /// Where the limits came from: `dados`, `modelo` or `manual`.
    pub const LIMITS_FROM: &str = "origem_limites";
    /// The response plan signal fired when the chart signals.
    pub const RESPONSE: &str = "sinal_resposta";
    /// The saving whose realised value the characteristic measures.
    pub const SAVING: &str = "ganho";
    /// Value of the characteristic before the project.
    pub const BASELINE: &str = "linha_base";
    /// Money per month for each unit of improvement over the baseline.
    pub const PER_UNIT: &str = "valor_por_unidade";
    /// `maior` (higher is better) or `menor`.
    pub const BETTER: &str = "melhor";
    /// Twin time of the last point already checked (signals before it opened their cards).
    pub const CHECKED: &str = "conferido_ate";
    /// Control plan (deliverable): address of the twin panel or folder of recordings.
    pub const TWIN: &str = "endereco_gemeo";

    // Response plan.
    pub const SIGNAL: &str = "sinal";
    pub const IMMEDIATE: &str = "acao_imediata";
    pub const CONTAINMENT: &str = "contencao";
    pub const CALL: &str = "acionar";
    pub const DEADLINE: &str = "prazo";
    /// Cards made from this signal (the last one).
    pub const CARD: &str = "cartao";
}

/// Data types of a measure.
pub const DATA_TYPES: [&str; 3] = ["continuo", "atributo", "contagem"];

/// A control plan line.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlLine<I> {
    pub id: I,
    pub process: String,
    pub characteristic: String,
    pub fmea_row: Option<I>,
    pub critical: bool,
}

/// A problem in a data collection plan line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionIssue {
    NoName,
    NoDefinition,
    NoUnit,
    NoSampling,
}

/// What a measure lacks to be collected the same way by everyone: name, operational
/// definition, unit (continuous data) and the sampling. `text` reads a field of the line.
pub fn collection_issues(text: impl Fn(&str) -> String) -> Vec<CollectionIssue> {
    let mut out = Vec::new();
    let empty = |f: &str| text(f).trim().is_empty();
    if empty(fields::NAME) {
        out.push(CollectionIssue::NoName);
    }
    if empty(fields::DEFINITION) {
        out.push(CollectionIssue::NoDefinition);
    }
    let continuous = matches!(text(fields::DATA_TYPE).as_str(), "" | "continuo");
    if continuous && empty(fields::UNIT) {
        out.push(CollectionIssue::NoUnit);
    }
    if empty(fields::SAMPLE_SIZE) || empty(fields::FREQUENCY) {
        out.push(CollectionIssue::NoSampling);
    }
    out
}

/// The task opened by a signal: the title "signal: immediate action", the containment
/// (how), "Acionar: who" (where) and the deadline as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalTask {
    pub title: String,
    pub how: Option<String>,
    pub call: Option<String>,
    pub deadline: String,
}

/// The task of a signal from its fields (`text` reads a field of the signal).
pub fn signal_task(text: impl Fn(&str) -> String) -> SignalTask {
    let what = text(fields::SIGNAL);
    let act = text(fields::IMMEDIATE);
    let title = match (what.trim(), act.trim()) {
        ("", "") => "Plano de resposta".to_string(),
        (w, "") => w.to_string(),
        ("", a) => a.to_string(),
        (w, a) => format!("{w}: {a}"),
    };
    let some = |s: String| {
        let t = s.trim().to_string();
        (!t.is_empty()).then_some(t)
    };
    SignalTask {
        title,
        how: some(text(fields::CONTAINMENT)),
        call: some(text(fields::CALL)).map(|c| format!("Acionar: {c}")),
        deadline: text(fields::DEADLINE).trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn issues_and_signal_task() {
        let m: HashMap<&str, &str> = [(fields::NAME, "Diâmetro"), (fields::DATA_TYPE, "continuo")]
            .into_iter()
            .collect();
        let get = |f: &str| m.get(f).copied().unwrap_or_default().to_string();
        assert_eq!(
            collection_issues(get),
            [
                CollectionIssue::NoDefinition,
                CollectionIssue::NoUnit,
                CollectionIssue::NoSampling
            ]
        );
        let s: HashMap<&str, &str> = [
            (fields::SIGNAL, "Ponto fora"),
            (fields::IMMEDIATE, "Parar"),
            (fields::CALL, "Supervisor"),
        ]
        .into_iter()
        .collect();
        let t = signal_task(|f| s.get(f).copied().unwrap_or_default().to_string());
        assert_eq!(t.title, "Ponto fora: Parar");
        assert_eq!(t.call.as_deref(), Some("Acionar: Supervisor"));
        assert_eq!(t.how, None);
        assert_eq!(signal_task(|_| String::new()).title, "Plano de resposta");
    }
}
