//! Process FMEA, the rules without any project state: the row (step, failure mode, effect,
//! severity S, cause, occurrence O, prevention and detection controls, detection D, the
//! recommended action and the S, O and D revised after it), `RPN = S × O × D`, the action
//! priority from an [`ApTable`], the revision before and after, the ranking for action and
//! the critical characteristics not covered by a control plan. The field names are the ones
//! saved by the apps (Seis Sigma, Qualidade, Segurança).

use crate::ap::{Ap, ApTable};

/// Item kind of a row in the apps' files.
pub const ITEM: &str = "linha_fmea";

pub mod fields {
    pub const STEP: &str = "etapa";
    pub const MODE: &str = "modo";
    pub const EFFECT: &str = "efeito";
    pub const S: &str = "s";
    pub const CAUSE: &str = "causa";
    pub const O: &str = "o";
    pub const PREVENTION: &str = "prevencao";
    pub const DETECTION: &str = "deteccao";
    pub const D: &str = "d";
    pub const ACTION: &str = "acao";
    /// The board card made from the action.
    pub const CARD: &str = "cartao";
    pub const ACTION_TAKEN: &str = "acao_tomada";
    pub const S2: &str = "s_revista";
    pub const O2: &str = "o_revista";
    pub const D2: &str = "d_revista";
    /// Of the FMEA: severity from which a row is a critical characteristic (default 9).
    pub const CRITICAL_FROM: &str = "severidade_critica";
}

/// Severity from which a characteristic is critical when the FMEA does not say.
pub const CRITICAL_FROM: u8 = 9;

/// The three company scales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    S,
    O,
    D,
}

impl Scale {
    pub const ALL: [Scale; 3] = [Scale::S, Scale::O, Scale::D];

    /// Field holding the description of `level` (1 to 10): `escala_s_7`.
    pub fn field(self, level: u8) -> String {
        let k = match self {
            Scale::S => 's',
            Scale::O => 'o',
            Scale::D => 'd',
        };
        format!("escala_{k}_{level}")
    }
}

/// A rating from 1 to 10 (anything else is no rating).
pub fn rating(v: Option<i64>) -> Option<u8> {
    v.filter(|v| (1..=10).contains(v)).map(|v| v as u8)
}

/// A row of the FMEA; `I` is the identifier of the app.
#[derive(Debug, Clone, PartialEq)]
pub struct Row<I> {
    pub id: I,
    pub step: String,
    pub mode: String,
    pub effect: String,
    pub s: Option<u8>,
    pub cause: String,
    pub o: Option<u8>,
    pub prevention: String,
    pub detection: String,
    pub d: Option<u8>,
    pub action: String,
    pub card: Option<I>,
    pub action_taken: String,
    pub s2: Option<u8>,
    pub o2: Option<u8>,
    pub d2: Option<u8>,
}

/// RPN and AP before and after the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision {
    pub rpn_before: u32,
    pub rpn_after: u32,
    pub ap_before: Ap,
    pub ap_after: Ap,
}

impl Revision {
    pub fn improved(&self) -> bool {
        self.rpn_after < self.rpn_before || self.ap_after < self.ap_before
    }
}

impl<I> Row<I> {
    /// S, O and D when all three are rated.
    pub fn sod(&self) -> Option<(u8, u8, u8)> {
        Some((self.s?, self.o?, self.d?))
    }

    /// S, O and D after the action: each revised value, or the original where not revised.
    /// `None` while nothing was revised.
    pub fn sod_after(&self) -> Option<(u8, u8, u8)> {
        if self.s2.is_none() && self.o2.is_none() && self.d2.is_none() {
            return None;
        }
        let (s, o, d) = self.sod()?;
        Some((
            self.s2.unwrap_or(s),
            self.o2.unwrap_or(o),
            self.d2.unwrap_or(d),
        ))
    }

    pub fn rpn(&self) -> Option<u32> {
        self.sod().map(rpn)
    }

    pub fn ap(&self, table: &ApTable) -> Option<Ap> {
        self.sod().and_then(|(s, o, d)| table.get(s, o, d))
    }

    pub fn revision(&self, table: &ApTable) -> Option<Revision> {
        let (s, o, d) = self.sod()?;
        let (s2, o2, d2) = self.sod_after()?;
        Some(Revision {
            rpn_before: rpn((s, o, d)),
            rpn_after: rpn((s2, o2, d2)),
            ap_before: table.get(s, o, d)?,
            ap_after: table.get(s2, o2, d2)?,
        })
    }

    /// Severity that decides whether the characteristic is critical (the revised one when
    /// there is one: a design change can lower it).
    pub fn current_s(&self) -> Option<u8> {
        self.s2.or(self.s)
    }

    pub fn is_critical(&self, from: u8) -> bool {
        self.current_s().is_some_and(|s| s >= from)
    }
}

/// `S × O × D` (1 to 1000).
pub fn rpn((s, o, d): (u8, u8, u8)) -> u32 {
    u32::from(s) * u32::from(o) * u32::from(d)
}

/// Rows sorted for action: AP (high first), then RPN, then S.
pub fn ranked<I: Clone>(rows: &[Row<I>], table: &ApTable) -> Vec<Row<I>> {
    let mut v = rows.to_vec();
    v.sort_by(|a, b| {
        b.ap(table)
            .cmp(&a.ap(table))
            .then(b.rpn().cmp(&a.rpn()))
            .then(b.s.cmp(&a.s))
    });
    v
}

/// Where the AP table comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableSource {
    BuiltIn,
    /// Imported by the company, with the name given at the import.
    Company(String),
}

/// The critical rows (from `from` up) whose id is not in `covered` (the rows a control
/// plan line links to).
pub fn uncovered<'a, I: PartialEq>(
    rows: &'a [Row<I>],
    from: u8,
    covered: &[I],
) -> impl Iterator<Item = &'a Row<I>> {
    rows.iter()
        .filter(move |r| r.is_critical(from) && !covered.contains(&r.id))
}

/// The title of the card made from a row: the recommended action, or "FMEA: mode".
pub fn card_title<I>(r: &Row<I>) -> String {
    if r.action.trim().is_empty() {
        format!("FMEA: {}", r.mode.trim())
    } else {
        r.action.trim().to_string()
    }
}

/// The reason of the card: "mode / cause" (what is written of the two).
pub fn card_reason<I>(r: &Row<I>) -> String {
    [r.mode.trim(), r.cause.trim()]
        .iter()
        .filter(|x| !x.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u32, s: u8, o: u8, d: u8) -> Row<u32> {
        Row {
            id,
            step: String::new(),
            mode: "Furo fora de posição".into(),
            effect: String::new(),
            s: Some(s),
            cause: "Gabarito gasto".into(),
            o: Some(o),
            prevention: String::new(),
            detection: String::new(),
            d: Some(d),
            action: String::new(),
            card: None,
            action_taken: String::new(),
            s2: None,
            o2: None,
            d2: None,
        }
    }

    #[test]
    fn rpn_ap_ranking_and_revision() {
        let t = ApTable::builtin();
        let a = row(1, 5, 5, 5);
        let b = row(2, 10, 4, 2);
        assert_eq!((a.rpn(), b.rpn()), (Some(125), Some(80)));
        assert_eq!(b.ap(&t), Some(Ap::High));
        let order: Vec<u32> = ranked(&[a.clone(), b.clone()], &t)
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(order, [2, 1]);
        let mut r = row(3, 9, 6, 6);
        assert_eq!(r.revision(&t), None);
        r.o2 = Some(2);
        r.d2 = Some(3);
        let rev = r.revision(&t).unwrap();
        assert_eq!((rev.rpn_before, rev.rpn_after), (324, 54));
        assert_eq!((rev.ap_before, rev.ap_after), (Ap::High, Ap::Medium));
        assert!(rev.improved());
        assert_eq!(rpn((10, 10, 10)), 1000);
        assert_eq!(rating(Some(11)), None);
        assert_eq!(rating(Some(7)), Some(7));
    }

    #[test]
    fn critical_rows_and_card_text() {
        let rows = [row(1, 9, 2, 2), row(2, 6, 9, 9)];
        let gaps: Vec<u32> = uncovered(&rows, CRITICAL_FROM, &[]).map(|r| r.id).collect();
        assert_eq!(gaps, [1]);
        assert_eq!(uncovered(&rows, CRITICAL_FROM, &[1]).count(), 0);
        assert_eq!(uncovered(&rows, 6, &[1]).count(), 1);
        assert_eq!(card_title(&rows[0]), "FMEA: Furo fora de posição");
        assert_eq!(
            card_reason(&rows[0]),
            "Furo fora de posição / Gabarito gasto"
        );
        assert_eq!(Scale::S.field(10), "escala_s_10");
    }
}
