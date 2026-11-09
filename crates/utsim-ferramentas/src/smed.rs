//! Quick changeover, SMED (`docs/plano_lean_seis_sigma.md`, 2.4): the activities of the
//! setup with their time and class (internal, done with the machine stopped, or external,
//! done while it runs), what each becomes after the study (internal, external or removed)
//! and its new time. The setup time is the sum of the internal activities, before and after.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Class {
    /// Done with the machine stopped.
    #[serde(rename = "interna")]
    Internal,
    /// Done while the machine runs.
    #[serde(rename = "externa")]
    External,
    /// Removed by the study (only after).
    #[serde(rename = "eliminada")]
    Removed,
}

impl Class {
    pub fn key(self) -> &'static str {
        match self {
            Class::Internal => "interna",
            Class::External => "externa",
            Class::Removed => "eliminada",
        }
    }

    pub fn from_key(k: &str) -> Option<Class> {
        [Class::Internal, Class::External, Class::Removed]
            .into_iter()
            .find(|c| c.key() == k)
    }
}

/// One activity of the setup. Times in minutes.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub name: String,
    pub time: Option<f64>,
    pub class: Option<Class>,
    /// The class after the study; absent keeps the class before.
    pub class_after: Option<Class>,
    /// The time after the study; absent keeps the time before.
    pub time_after: Option<f64>,
}

impl Activity {
    fn before(&self) -> (Option<Class>, f64) {
        (self.class, self.time.filter(|t| *t >= 0.0).unwrap_or(0.0))
    }

    fn after(&self) -> (Option<Class>, f64) {
        let class = self.class_after.or(self.class);
        let time = if class == Some(Class::Removed) {
            0.0
        } else {
            self.time_after
                .or(self.time)
                .filter(|t| *t >= 0.0)
                .unwrap_or(0.0)
        };
        (class, time)
    }

    /// Converted from internal to external (or removed): the heart of SMED.
    pub fn converted(&self) -> bool {
        self.class == Some(Class::Internal)
            && matches!(self.class_after, Some(Class::External | Class::Removed))
    }
}

/// Sums of one side (before or after), in minutes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Totals {
    /// The setup time: the machine is stopped for this.
    pub internal: f64,
    pub external: f64,
    pub removed: f64,
    /// Activities without class or time.
    pub incomplete: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Study {
    pub before: Totals,
    pub after: Totals,
    /// How many internal activities went external or were removed.
    pub converted: usize,
    /// Reduction of the setup time, in % of the time before (`None` when before is 0).
    pub reduction_pct: Option<f64>,
}

pub fn study(activities: &[Activity]) -> Study {
    let mut before = Totals::default();
    let mut after = Totals::default();
    for a in activities {
        for (side, (class, time), raw) in [
            (&mut before, a.before(), a.time),
            (&mut after, a.after(), a.time_after.or(a.time)),
        ] {
            match class {
                Some(Class::Internal) => side.internal += time,
                Some(Class::External) => side.external += time,
                Some(Class::Removed) => side.removed += a.time.unwrap_or(0.0).max(0.0),
                None => side.incomplete += 1,
            }
            if class.is_some() && class != Some(Class::Removed) && raw.is_none() {
                side.incomplete += 1;
            }
        }
    }
    let converted = activities.iter().filter(|a| a.converted()).count();
    Study {
        reduction_pct: (before.internal > 0.0)
            .then(|| (before.internal - after.internal) / before.internal * 100.0),
        before,
        after,
        converted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(
        name: &str,
        time: f64,
        class: Class,
        class_after: Option<Class>,
        time_after: Option<f64>,
    ) -> Activity {
        Activity {
            name: name.into(),
            time: Some(time),
            class: Some(class),
            class_after,
            time_after,
        }
    }

    #[test]
    fn worked_example_by_hand() {
        // Before: internal 10 + 25 + 15 + 8 = 58, external 5.
        // After: buscar ferramentas vai para externa (10), ajuste reduzido para 6, procurar
        // a chave eliminada (8): internal 25 + 6 = 31; external 5 + 10 = 15; removed 8.
        let s = study(&[
            act(
                "Buscar ferramentas",
                10.0,
                Class::Internal,
                Some(Class::External),
                None,
            ),
            act("Trocar a matriz", 25.0, Class::Internal, None, None),
            act("Ajustar", 15.0, Class::Internal, None, Some(6.0)),
            act(
                "Procurar a chave",
                8.0,
                Class::Internal,
                Some(Class::Removed),
                None,
            ),
            act("Separar a matéria-prima", 5.0, Class::External, None, None),
        ]);
        assert_eq!(s.before.internal, 58.0);
        assert_eq!(s.before.external, 5.0);
        assert_eq!(s.after.internal, 31.0);
        assert_eq!(s.after.external, 15.0);
        assert_eq!(s.after.removed, 8.0);
        assert_eq!(s.converted, 2);
        let r = s.reduction_pct.unwrap();
        assert!((r - 27.0 / 58.0 * 100.0).abs() < 1e-9);
        assert_eq!(s.before.incomplete, 0);
    }

    #[test]
    fn incomplete_rows_are_counted() {
        let s = study(&[Activity {
            name: "?".into(),
            time: None,
            class: Some(Class::Internal),
            class_after: None,
            time_after: None,
        }]);
        assert_eq!(s.before.incomplete, 1);
        assert_eq!(s.reduction_pct, None);
        assert_eq!(Class::from_key("externa"), Some(Class::External));
    }
}
