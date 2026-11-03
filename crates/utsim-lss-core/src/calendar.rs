//! Working-day calendar of the schedule (`docs/plano_lean_seis_sigma.md`, 1.6): which
//! weekdays are worked, the holidays, and the conversion between a count of working days
//! (what `utsim-pert` computes with) and calendar dates.
//!
//! Convention: working day index 0 is the first working day on or after the schedule
//! start; an activity with early start `es` and early finish `ef` (in working days) starts
//! on index `floor(es)` and ends on index `ceil(ef) - 1` (inclusive), and a zero-length
//! milestone falls on index `floor(es)`.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::date::Date;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::State;

/// Field names.
pub mod fields {
    /// On the project: worked weekdays, array of 1 (Monday) to 7 (Sunday).
    pub const WORKDAYS: &str = "dias_uteis";
    /// Holiday entity.
    pub const PROJECT: &str = "projeto";
    pub const DATE: &str = "data";
    pub const NAME: &str = "nome";
}

/// ISO weekday of a date: 1 = Monday ... 7 = Sunday.
pub fn weekday(d: Date) -> u8 {
    // 1970-01-01 was a Thursday.
    ((d.days_since_epoch() + 3).rem_euclid(7) + 1) as u8
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    /// `workdays[0]` is Monday.
    pub workdays: [bool; 7],
    pub holidays: BTreeSet<Date>,
}

impl Default for Calendar {
    /// Monday to Friday, no holidays.
    fn default() -> Self {
        Self {
            workdays: [true, true, true, true, true, false, false],
            holidays: BTreeSet::new(),
        }
    }
}

/// Safety bound of the day-by-day walks (about 27 years).
const MAX_WALK: i64 = 10_000;

impl Calendar {
    pub fn has_workdays(&self) -> bool {
        self.workdays.iter().any(|w| *w)
    }

    pub fn is_working(&self, d: Date) -> bool {
        self.workdays[usize::from(weekday(d) - 1)] && !self.holidays.contains(&d)
    }

    /// The first working day on or after `d` (`d` itself when nothing is worked).
    pub fn next_working(&self, d: Date) -> Date {
        if !self.has_workdays() {
            return d;
        }
        let mut x = d;
        for _ in 0..MAX_WALK {
            if self.is_working(x) {
                return x;
            }
            x = x.add_days(1);
        }
        d
    }

    /// The date of working day `index` counted from `start` (index 0 is the first working
    /// day on or after `start`). Negative indices count backwards.
    pub fn date_at(&self, start: Date, index: i64) -> Date {
        let mut d = self.next_working(start);
        if !self.has_workdays() {
            return d.add_days(index);
        }
        let step = if index >= 0 { 1 } else { -1 };
        let mut left = index.abs();
        let mut guard = 0;
        while left > 0 && guard < MAX_WALK * 2 {
            d = d.add_days(step);
            if self.is_working(d) {
                left -= 1;
            }
            guard += 1;
        }
        d
    }

    /// Working days in `[from, to)` (negative when `to` is before `from`).
    pub fn working_days_between(&self, from: Date, to: Date) -> i64 {
        if to < from {
            return -self.working_days_between(to, from);
        }
        let span = from.days_until(to);
        if span > MAX_WALK * 4 {
            // Far away: whole weeks plus the rest, holidays ignored beyond the bound.
            let per_week = self.workdays.iter().filter(|w| **w).count() as i64;
            return span / 7 * per_week;
        }
        (0..span)
            .filter(|i| self.is_working(from.add_days(*i)))
            .count() as i64
    }

    /// Working-day index of `d` counted from `start` (see [`Calendar::date_at`]). A day off
    /// gets the index of the next working day.
    pub fn index_of(&self, start: Date, d: Date) -> i64 {
        self.working_days_between(self.next_working(start), d)
    }
}

/// The calendar of a project: its worked weekdays and holidays.
pub fn of_project(state: &State, project: Id) -> Calendar {
    let mut cal = Calendar::default();
    if let Some(days) = state
        .entity(project)
        .and_then(|e| e.get(fields::WORKDAYS))
        .and_then(Value::as_array)
    {
        let mut w = [false; 7];
        for d in days.iter().filter_map(Value::as_u64) {
            if (1..=7).contains(&d) {
                w[(d - 1) as usize] = true;
            }
        }
        cal.workdays = w;
    }
    cal.holidays = holidays(state, project).into_iter().map(|h| h.1).collect();
    cal
}

/// Holidays of a project: `(id, date, name)`, by date.
pub fn holidays(state: &State, project: Id) -> Vec<(Id, Date, String)> {
    let mut v: Vec<(Id, Date, String)> =
        crate::domain::of_project(state, Kind::Holiday, project, |id, e| {
            (id, e.opt_text(fields::DATE), e.text(fields::NAME))
        })
        .into_iter()
        .filter_map(|(id, d, n)| Some((id, d?.parse().ok()?, n)))
        .collect();
    v.sort_by_key(|h| (h.1, h.0));
    v
}

/// Edits that add a holiday.
pub fn add_holiday(project: Id, id: Id, date: Date, name: &str) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Holiday, id);
    vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::DATE, date.to_string()),
        Edit::new(r, fields::NAME, name),
    ]
}

/// Edit that sets the worked weekdays (1 = Monday ... 7 = Sunday).
pub fn set_workdays(project: Id, days: &[u8]) -> Edit {
    let mut d: Vec<u8> = days
        .iter()
        .copied()
        .filter(|x| (1..=7).contains(x))
        .collect();
    d.sort_unstable();
    d.dedup();
    Edit::new(
        crate::domain::Project::entity_ref(project),
        fields::WORKDAYS,
        Value::from(d),
    )
}

/// Easter Sunday (anonymous Gregorian algorithm).
pub fn easter(year: i32) -> Date {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    Date::new(year, month as u8, day as u8).expect("valid Easter date")
}

/// The national holidays of Brazil in a year (fixed dates and Good Friday), with their
/// names. Optional days (carnival, Corpus Christi) and local holidays are added by hand.
pub fn national_holidays_br(year: i32) -> Vec<(Date, &'static str)> {
    let fixed: [(u8, u8, &str); 9] = [
        (1, 1, "Confraternização universal"),
        (4, 21, "Tiradentes"),
        (5, 1, "Dia do Trabalho"),
        (9, 7, "Independência"),
        (10, 12, "Nossa Senhora Aparecida"),
        (11, 2, "Finados"),
        (11, 15, "Proclamação da República"),
        (11, 20, "Dia Nacional de Zumbi e da Consciência Negra"),
        (12, 25, "Natal"),
    ];
    let mut v: Vec<(Date, &str)> = fixed
        .iter()
        .filter(|(m, d, _)| !(year < 2024 && *m == 11 && *d == 20))
        .filter_map(|(m, d, n)| Date::new(year, *m, *d).map(|x| (x, *n)))
        .collect();
    v.push((easter(year).add_days(-2), "Sexta-feira da Paixão"));
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    #[test]
    fn weekdays() {
        assert_eq!(weekday(d(1970, 1, 1)), 4);
        assert_eq!(weekday(d(2026, 11, 3)), 2, "a Tuesday");
        assert_eq!(weekday(d(2026, 11, 8)), 7, "a Sunday");
    }

    #[test]
    fn working_days_skip_weekends_and_holidays() {
        let mut cal = Calendar::default();
        // Friday 2026-11-13: the next working day is Monday 16; then Tuesday 17 is off.
        let fri = d(2026, 11, 13);
        assert_eq!(cal.date_at(fri, 1), d(2026, 11, 16));
        assert_eq!(
            cal.date_at(d(2026, 11, 14), 0),
            d(2026, 11, 16),
            "Saturday start"
        );
        cal.holidays.insert(d(2026, 11, 17));
        assert_eq!(cal.date_at(fri, 2), d(2026, 11, 18));
        assert_eq!(cal.working_days_between(fri, d(2026, 11, 18)), 2);
        assert_eq!(cal.index_of(fri, d(2026, 11, 18)), 2);
        assert_eq!(cal.date_at(d(2026, 11, 18), -2), fri);
        assert_eq!(cal.working_days_between(d(2026, 11, 18), fri), -2);
        // Nothing worked: plain days.
        let none = Calendar {
            workdays: [false; 7],
            holidays: BTreeSet::new(),
        };
        assert_eq!(none.date_at(fri, 3), d(2026, 11, 16));
    }

    #[test]
    fn brazilian_holidays() {
        assert_eq!(easter(2026), d(2026, 4, 5));
        assert_eq!(easter(2024), d(2024, 3, 31));
        assert_eq!(easter(2000), d(2000, 4, 23));
        let h = national_holidays_br(2026);
        assert_eq!(h.len(), 10);
        assert!(h.contains(&(d(2026, 4, 3), "Sexta-feira da Paixão")));
        assert!(h.iter().any(|x| x.0 == d(2026, 11, 20)));
        assert_eq!(
            national_holidays_br(2023).len(),
            9,
            "20/11 national from 2024"
        );
    }

    #[test]
    fn project_calendar_from_the_journal() {
        let mut n = 0u64;
        let mut ids = || {
            n += 1;
            Id::v7(n, [n as u8; 10])
        };
        let p = ids();
        let mut edits = vec![set_workdays(p, &[6, 1, 2, 3, 4, 5, 9])];
        edits.extend(add_holiday(p, ids(), d(2026, 12, 25), "Natal"));
        let me = ids();
        let s = State::replay(&State::new().prepare(me, 0, edits, &mut ids));
        let cal = of_project(&s, p);
        assert_eq!(cal.workdays, [true, true, true, true, true, true, false]);
        assert!(!cal.is_working(d(2026, 12, 25)));
        assert!(cal.is_working(d(2026, 12, 26)), "Saturday is worked here");
        assert_eq!(holidays(&s, p)[0].2, "Natal");
    }

    proptest! {
        #[test]
        fn index_and_date_round_trip(start in 0i64..30_000, k in 0i64..400, hol in proptest::collection::vec(0i64..500, 0..20)) {
            let s = Date::from_days_since_epoch(start);
            let mut cal = Calendar::default();
            for h in hol {
                cal.holidays.insert(s.add_days(h));
            }
            let dk = cal.date_at(s, k);
            prop_assert!(cal.is_working(dk));
            prop_assert_eq!(cal.index_of(s, dk), k);
            prop_assert!(cal.date_at(s, k + 1) > dk);
        }
    }
}
