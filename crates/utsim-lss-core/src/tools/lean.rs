//! The Lean tools and the choice of solutions of the Improve phase and of the DMADV
//! (`docs/plano_lean_seis_sigma.md`, 2.4, etapa P6): selection matrix (weighted or Pugh,
//! with effort × impact), 5S audits, SMED and standardised work with line balancing. The
//! numbers come from `utsim_ferramentas` (`selecao`, `cinco_s`, `smed`, `trabalho_padrao`);
//! this module reads the items of the deliverable and writes the edits.
//!
//! A score of a matrix cell is a field of the row, `nota_<id of the column>` (as in the
//! cause and effect matrix), so two people scoring different cells never collide.

use serde_json::Value;

pub use utsim_ferramentas::cinco_s::{self, AuditScore, Sense};
pub use utsim_ferramentas::selecao::{self, Method, Quadrant, Selection};
pub use utsim_ferramentas::smed::{self, Class, Study};
pub use utsim_ferramentas::trabalho_padrao::{self, Balance};

use super::{ItemRef, item_ref, items, new_item, rating};
use crate::date::Date;
use crate::id::Id;
use crate::op::Edit;
use crate::state::State;

/// Field of the score of a row against the column `col` (criterion or question).
pub fn score_field(col: Id) -> String {
    let hex: String = col.to_string().chars().filter(|c| *c != '-').collect();
    format!("nota_{hex}")
}

fn cell(row: &ItemRef, col: Id) -> Option<i64> {
    row.entity.integer(&score_field(col))
}

/// Edit that scores a cell (`None` clears it).
pub fn set_score(row: Id, col: Id, v: Option<i64>) -> Edit {
    Edit::new(
        item_ref(row),
        score_field(col),
        v.map_or(Value::from(""), Value::from),
    )
}

fn number(i: &ItemRef, field: &str) -> Option<f64> {
    i.entity.number(field)
}

// ---- Selection of solutions ------------------------------------------------------------

pub mod selection {
    use super::*;

    pub const CRITERION: &str = "criterio";
    pub const OPTION: &str = "opcao";

    pub mod fields {
        /// On the deliverable: `ponderada` or `pugh`.
        pub const METHOD: &str = "metodo";
        pub const NAME: &str = "nome";
        /// Criterion weight, 1 to 10.
        pub const WEIGHT: &str = "peso";
        /// The reference concept of the Pugh matrix.
        pub const REFERENCE: &str = "referencia";
        /// Impact and effort, 1 to 10.
        pub const IMPACT: &str = "impacto";
        pub const EFFORT: &str = "esforco";
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Row {
        pub id: Id,
        pub name: String,
        pub reference: bool,
        pub impact: Option<u8>,
        pub effort: Option<u8>,
        pub quadrant: Option<Quadrant>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Matrix {
        pub method: Method,
        /// `(id, name, weight)`.
        pub criteria: Vec<(Id, String, u8)>,
        pub rows: Vec<Row>,
        /// Same order as `rows`.
        pub result: Selection,
    }

    pub fn method(state: &State, del: Id) -> Method {
        state
            .entity(del)
            .and_then(|e| e.typed(fields::METHOD))
            .unwrap_or_default()
    }

    pub fn matrix(state: &State, del: Id) -> Matrix {
        let method = method(state, del);
        let criteria: Vec<(Id, String, u8)> = items(state, del, CRITERION)
            .iter()
            .map(|c| {
                (
                    c.id,
                    c.text(fields::NAME),
                    rating(c.entity, fields::WEIGHT).unwrap_or(1),
                )
            })
            .collect();
        let opts = items(state, del, OPTION);
        let rows: Vec<Row> = opts
            .iter()
            .map(|o| {
                let impact = rating(o.entity, fields::IMPACT);
                let effort = rating(o.entity, fields::EFFORT);
                Row {
                    id: o.id,
                    name: o.text(fields::NAME),
                    reference: o.entity.boolean(fields::REFERENCE),
                    impact,
                    effort,
                    quadrant: impact
                        .zip(effort)
                        .and_then(|(i, e)| selecao::quadrant(i, e)),
                }
            })
            .collect();
        let crit: Vec<selecao::Criterion> = criteria
            .iter()
            .map(|(_, n, w)| selecao::Criterion {
                name: n.clone(),
                weight: *w,
            })
            .collect();
        let input: Vec<selecao::OptionIn> = opts
            .iter()
            .zip(&rows)
            .map(|(o, r)| selecao::OptionIn {
                name: r.name.clone(),
                reference: r.reference,
                scores: criteria
                    .iter()
                    .map(|(c, _, _)| cell(o, *c).and_then(|v| i8::try_from(v).ok()))
                    .collect(),
            })
            .collect();
        Matrix {
            method,
            result: selecao::compute(method, &crit, &input),
            criteria,
            rows,
        }
    }

    /// Marks `option` as the reference and clears the mark of the others.
    pub fn set_reference(state: &State, del: Id, option: Id) -> Vec<Edit> {
        items(state, del, OPTION)
            .iter()
            .filter(|o| (o.id == option) != o.entity.boolean(fields::REFERENCE))
            .map(|o| Edit::new(item_ref(o.id), fields::REFERENCE, o.id == option))
            .collect()
    }
}

// ---- 5S --------------------------------------------------------------------------------

pub mod five_s {
    use super::*;

    pub const QUESTION: &str = "pergunta";
    pub const AUDIT: &str = "auditoria";

    pub mod fields {
        pub const SENSE: &str = "senso";
        pub const TEXT: &str = "texto";
        pub const DATE: &str = "data";
        pub const AUDITOR: &str = "auditor";
        pub const NOTE: &str = "observacao";
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Question {
        pub id: Id,
        pub sense: Option<Sense>,
        pub text: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Audit {
        pub id: Id,
        pub date: Option<Date>,
        pub auditor: String,
        pub score: AuditScore,
    }

    pub fn questions(state: &State, del: Id) -> Vec<Question> {
        let mut v: Vec<Question> = items(state, del, QUESTION)
            .iter()
            .map(|q| Question {
                id: q.id,
                sense: Sense::from_key(&q.text(fields::SENSE)),
                text: q.text(fields::TEXT),
            })
            .collect();
        // By sense, keeping the entry order inside each sense.
        v.sort_by_key(|q| q.sense.map_or(Sense::ALL.len(), |s| s as usize));
        v
    }

    /// Audits in date order (undated last), each scored against the current checklist.
    pub fn audits(state: &State, del: Id) -> Vec<Audit> {
        let qs = questions(state, del);
        let mut v: Vec<Audit> = items(state, del, AUDIT)
            .iter()
            .map(|a| {
                let answers: Vec<cinco_s::Answer> = qs
                    .iter()
                    .filter_map(|q| {
                        Some(cinco_s::Answer {
                            sense: q.sense?,
                            score: cell(a, q.id).and_then(|v| u8::try_from(v).ok()),
                        })
                    })
                    .collect();
                Audit {
                    id: a.id,
                    date: a.text(fields::DATE).parse().ok(),
                    auditor: a.text(fields::AUDITOR),
                    score: cinco_s::score(&answers),
                }
            })
            .collect();
        v.sort_by_key(|a| (a.date.is_none(), a.date));
        v
    }

    /// The questions of [`cinco_s::STARTER`] that are not in the checklist yet (same text).
    pub fn starter_edits(
        state: &State,
        project: Id,
        del: Id,
        mut new_id: impl FnMut() -> Id,
    ) -> Vec<Edit> {
        let have: Vec<String> = questions(state, del)
            .iter()
            .map(|q| q.text.trim().to_lowercase())
            .collect();
        let mut edits = Vec::new();
        let mut n = 0.0;
        for (sense, text) in cinco_s::STARTER {
            if have.contains(&text.to_lowercase()) {
                continue;
            }
            let mut e = new_item(
                state,
                project,
                del,
                QUESTION,
                new_id(),
                None,
                vec![
                    (fields::SENSE, Value::from(sense.key())),
                    (fields::TEXT, Value::from(text)),
                ],
            );
            // `new_item` reads the order from the state, which does not see these yet.
            if let Some(o) = e
                .iter_mut()
                .find(|e| e.field == super::super::fields::item::ORDER)
            {
                o.value = Value::from(o.value.as_f64().unwrap_or(1.0) + n);
            }
            n += 1.0;
            edits.extend(e);
        }
        edits
    }
}

// ---- SMED ------------------------------------------------------------------------------

pub mod changeover {
    use super::*;

    pub const ACTIVITY: &str = "atividade";

    pub mod fields {
        pub const NAME: &str = "nome";
        /// Minutes.
        pub const TIME: &str = "tempo";
        pub const CLASS: &str = "classe";
        pub const CLASS_AFTER: &str = "classe_depois";
        pub const TIME_AFTER: &str = "tempo_depois";
        pub const HOW: &str = "como";
    }

    pub fn activities(state: &State, del: Id) -> Vec<smed::Activity> {
        items(state, del, ACTIVITY)
            .iter()
            .map(|a| smed::Activity {
                name: a.text(fields::NAME),
                time: number(a, fields::TIME),
                class: Class::from_key(&a.text(fields::CLASS)),
                class_after: Class::from_key(&a.text(fields::CLASS_AFTER)),
                time_after: number(a, fields::TIME_AFTER),
            })
            .collect()
    }

    pub fn study(state: &State, del: Id) -> Study {
        smed::study(&activities(state, del))
    }
}

// ---- Standardised work -----------------------------------------------------------------

pub mod standard_work {
    use super::*;

    pub const ELEMENT: &str = "elemento";

    pub mod fields {
        /// On the deliverable, seconds; when absent, available time over demand.
        pub const TAKT: &str = "takt";
        pub const AVAILABLE: &str = "tempo_disponivel";
        pub const DEMAND: &str = "demanda";
        pub const OPERATOR: &str = "operador";
        pub const NAME: &str = "nome";
        pub const MANUAL: &str = "manual";
        pub const AUTO: &str = "automatico";
        pub const WALK: &str = "deslocamento";
        pub const WIP: &str = "estoque_padrao";
    }

    /// The takt typed, or available time over demand.
    pub fn takt(state: &State, del: Id) -> Option<f64> {
        let e = state.entity(del)?;
        e.number(fields::TAKT).filter(|t| *t > 0.0).or_else(|| {
            trabalho_padrao::takt(e.number(fields::AVAILABLE)?, e.number(fields::DEMAND)?)
        })
    }

    pub fn elements(state: &State, del: Id) -> Vec<trabalho_padrao::Element> {
        items(state, del, ELEMENT)
            .iter()
            .map(|e| trabalho_padrao::Element {
                operator: e.text(fields::OPERATOR),
                name: e.text(fields::NAME),
                manual: number(e, fields::MANUAL).unwrap_or(0.0),
                auto: number(e, fields::AUTO).unwrap_or(0.0),
                walk: number(e, fields::WALK).unwrap_or(0.0),
                wip: number(e, fields::WIP).unwrap_or(0.0),
            })
            .collect()
    }

    pub fn balance(state: &State, del: Id) -> Balance {
        trabalho_padrao::balance(&elements(state, del), takt(state, del))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn selection_matrix_from_the_journal() {
        use selection::{fields as f, *};
        let mut fx = Fixture::new();
        let d = fx.deliverable(Tool::Selection);
        let custo = fx.item(
            d,
            CRITERION,
            None,
            vec![(f::NAME, "Custo".into()), (f::WEIGHT, 5.into())],
        );
        let prazo = fx.item(
            d,
            CRITERION,
            None,
            vec![(f::NAME, "Prazo".into()), (f::WEIGHT, 3.into())],
        );
        let a = fx.item(
            d,
            OPTION,
            None,
            vec![
                (f::NAME, "Gabarito".into()),
                (f::IMPACT, 8.into()),
                (f::EFFORT, 3.into()),
            ],
        );
        let b = fx.item(d, OPTION, None, vec![(f::NAME, "Robô".into())]);
        fx.commit(vec![
            set_score(a, custo, Some(4)),
            set_score(a, prazo, Some(5)),
            set_score(b, custo, Some(2)),
            set_score(b, prazo, Some(5)),
        ]);
        let m = matrix(&fx.state, d);
        assert_eq!(m.method, Method::Weighted);
        assert_eq!(m.result.options[0].total, 35.0);
        assert_eq!(m.result.options[1].total, 25.0);
        assert_eq!(m.result.best().unwrap().name, "Gabarito");
        assert_eq!(m.rows[0].quadrant, Some(Quadrant::QuickWin));
        // Pugh: the reference is unique.
        fx.set(d, f::METHOD, "pugh");
        let e = set_reference(&fx.state, d, b);
        fx.commit(e);
        fx.commit(vec![
            set_score(a, custo, Some(1)),
            set_score(a, prazo, Some(-1)),
        ]);
        let m = matrix(&fx.state, d);
        assert_eq!(m.method, Method::Pugh);
        assert!(!m.result.reference_issue);
        assert_eq!(m.result.options[0].total, 2.0, "5 − 3");
        let e = set_reference(&fx.state, d, a);
        assert_eq!(e.len(), 2, "mark a, unmark b");
    }

    #[test]
    fn five_s_audits_from_the_journal() {
        use five_s::{fields as f, *};
        let mut fx = Fixture::new();
        let d = fx.deliverable(Tool::FiveS);
        let mut n = 5000;
        let e = starter_edits(&fx.state, fx.project, d, || {
            n += 1;
            Id::v7(n, [7; 10])
        });
        fx.commit(e);
        let qs = questions(&fx.state, d);
        assert_eq!(qs.len(), 10);
        assert_eq!(qs[0].sense, Some(Sense::Sort));
        assert_eq!(qs[9].sense, Some(Sense::Sustain));
        let again = starter_edits(&fx.state, fx.project, d, || Id::v7(1, [8; 10]));
        assert!(again.is_empty(), "no duplicates");
        let late = fx.item(d, AUDIT, None, vec![(f::DATE, "2026-11-20".into())]);
        let early = fx.item(d, AUDIT, None, vec![(f::DATE, "2026-10-20".into())]);
        fx.commit(qs.iter().map(|q| set_score(early, q.id, Some(2))).collect());
        fx.commit(qs.iter().map(|q| set_score(late, q.id, Some(3))).collect());
        let au = audits(&fx.state, d);
        assert_eq!(au[0].id, early);
        assert_eq!(au[0].score.pct, Some(50.0));
        assert_eq!(au[1].score.pct, Some(75.0));
        assert!(au[1].score.complete());
    }

    #[test]
    fn smed_and_standard_work_from_the_journal() {
        let mut fx = Fixture::new();
        {
            use changeover::{fields as f, *};
            let d = fx.deliverable(Tool::Smed);
            fx.item(
                d,
                ACTIVITY,
                None,
                vec![
                    (f::NAME, "Buscar ferramentas".into()),
                    (f::TIME, 10.into()),
                    (f::CLASS, "interna".into()),
                    (f::CLASS_AFTER, "externa".into()),
                ],
            );
            fx.item(
                d,
                ACTIVITY,
                None,
                vec![(f::TIME, Value::from(20.5)), (f::CLASS, "interna".into())],
            );
            let s = study(&fx.state, d);
            assert_eq!(s.before.internal, 30.5);
            assert_eq!(s.after.internal, 20.5);
            assert_eq!(s.converted, 1);
        }
        {
            use standard_work::{fields as f, *};
            let d = fx.deliverable(Tool::StandardWork);
            fx.set(d, f::AVAILABLE, 27_000);
            fx.set(d, f::DEMAND, 450);
            assert_eq!(takt(&fx.state, d), Some(60.0));
            for (op, m) in [("Ana", 40.0), ("Bia", 70.0)] {
                fx.item(
                    d,
                    ELEMENT,
                    None,
                    vec![(f::OPERATOR, op.into()), (f::MANUAL, Value::from(m))],
                );
            }
            let b = balance(&fx.state, d);
            assert_eq!(b.min_operators, Some(2));
            assert!(b.operators[1].over_takt);
            fx.set(d, f::TAKT, 80);
            assert_eq!(balance(&fx.state, d).takt, Some(80.0), "typed takt wins");
        }
    }
}
