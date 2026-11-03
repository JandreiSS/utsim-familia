//! Calendar dates (`AAAA-MM-DD` in the files, `DD/MM/AAAA` on screen). The working-day
//! calendar of the schedule (`docs/plano_lean_seis_sigma.md`, 1.6) will grow from here.

use std::fmt;
use std::str::FromStr;

/// A proleptic Gregorian date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("data inválida '{0}' (use AAAA-MM-DD ou DD/MM/AAAA)")]
pub struct DateError(pub String);

impl Date {
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        if (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month) {
            Some(Date { year, month, day })
        } else {
            None
        }
    }

    /// Days since 1970-01-01 (negative before).
    pub fn days_since_epoch(self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub fn from_days_since_epoch(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Date { year, month, day }
    }

    /// The date of a Unix instant in milliseconds, at a UTC offset in minutes.
    pub fn from_unix_ms(ms: i64, offset_minutes: i32) -> Date {
        let local = ms + i64::from(offset_minutes) * 60_000;
        Date::from_days_since_epoch(local.div_euclid(86_400_000))
    }

    pub fn add_days(self, n: i64) -> Date {
        Date::from_days_since_epoch(self.days_since_epoch() + n)
    }

    /// Days from `self` to `other` (positive when `other` is later).
    pub fn days_until(self, other: Date) -> i64 {
        other.days_since_epoch() - self.days_since_epoch()
    }

    /// `DD/MM/AAAA`.
    pub fn format_br(self) -> String {
        format!("{:02}/{:02}/{:04}", self.day, self.month, self.year)
    }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

impl fmt::Display for Date {
    /// `AAAA-MM-DD`, the file form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl FromStr for Date {
    type Err = DateError;

    /// `AAAA-MM-DD` or `DD/MM/AAAA`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || DateError(s.chars().take(40).collect());
        let t = s.trim();
        let parts: Vec<&str>;
        let (y, m, d) = if t.contains('-') {
            parts = t.split('-').collect();
            match parts.as_slice() {
                [y, m, d] if y.len() == 4 => (*y, *m, *d),
                _ => return Err(err()),
            }
        } else {
            parts = t.split('/').collect();
            match parts.as_slice() {
                [d, m, y] if y.len() == 4 => (*y, *m, *d),
                _ => return Err(err()),
            }
        };
        let num = |x: &str| {
            (!x.is_empty() && x.len() <= 4 && x.bytes().all(|b| b.is_ascii_digit()))
                .then(|| x.parse::<u32>().ok())
                .flatten()
        };
        let (y, m, d) = (
            num(y).ok_or_else(err)?,
            num(m).ok_or_else(err)?,
            num(d).ok_or_else(err)?,
        );
        let (m, d) = (
            u8::try_from(m).map_err(|_| err())?,
            u8::try_from(d).map_err(|_| err())?,
        );
        Date::new(y as i32, m, d).ok_or_else(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parse_and_format() {
        let d: Date = "2026-11-03".parse().unwrap();
        assert_eq!(d, Date::new(2026, 11, 3).unwrap());
        assert_eq!(d.to_string(), "2026-11-03");
        assert_eq!(d.format_br(), "03/11/2026");
        assert_eq!("03/11/2026".parse::<Date>().unwrap(), d);
        for bad in [
            "2026-02-29",
            "2026-13-01",
            "26-01-01",
            "x",
            "2026-1-",
            "+202-01-01",
        ] {
            assert!(bad.parse::<Date>().is_err(), "{bad}");
        }
        assert!("2024-02-29".parse::<Date>().is_ok());
    }

    #[test]
    fn epoch_and_instants() {
        assert_eq!(Date::new(1970, 1, 1).unwrap().days_since_epoch(), 0);
        assert_eq!(
            Date::from_days_since_epoch(-1),
            Date::new(1969, 12, 31).unwrap()
        );
        // 2026-11-03 02:00 UTC is still 2 November in Brasília (UTC-3).
        let ms = Date::new(2026, 11, 3).unwrap().days_since_epoch() * 86_400_000 + 2 * 3_600_000;
        assert_eq!(Date::from_unix_ms(ms, 0), Date::new(2026, 11, 3).unwrap());
        assert_eq!(
            Date::from_unix_ms(ms, -180),
            Date::new(2026, 11, 2).unwrap()
        );
        let d = Date::new(2026, 12, 31).unwrap();
        assert_eq!(d.add_days(1), Date::new(2027, 1, 1).unwrap());
        assert_eq!(d.days_until(d.add_days(-10)), -10);
    }

    proptest! {
        #[test]
        fn days_round_trip(days in -300_000i64..2_900_000) {
            let d = Date::from_days_since_epoch(days);
            prop_assert_eq!(d.days_since_epoch(), days);
            prop_assert_eq!(d.to_string().parse::<Date>().unwrap(), d);
            prop_assert_eq!(d.format_br().parse::<Date>().unwrap(), d);
            prop_assert_eq!(d.add_days(1).days_since_epoch(), days + 1);
        }
    }
}
