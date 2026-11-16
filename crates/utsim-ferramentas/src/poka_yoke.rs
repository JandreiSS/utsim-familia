//! Mistake-proofing devices, poka-yoke (`docs/plano_lean_seis_sigma.md`, 2.4): each device
//! with its kind (contact, fixed value, sequence), its function (control, which stops the
//! error from happening, or warning, which makes it seen), the FMEA failure mode it deals
//! with (by the text of the FMEA row) and the occurrence and detection before and after.
//! It is checked from time to time; [`compute`] says what each device changed in the
//! FMEA, whether that agrees with its function, and when the next check is due.
//!
//! The types are the exchange format of the tool (serde, `Default`); the screens live in
//! the quality app and draw the result. Dates are `AAAA-MM-DD`.

use crate::texto::{fx, tx, vx};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Kind {
    /// Shape, size or presence of the part (a guide pin, a sensor).
    #[default]
    #[serde(rename = "contato")]
    Contact,
    /// A count that must be reached (number of screws, of parts in the kit).
    #[serde(rename = "valor_fixo")]
    FixedValue,
    /// The steps must happen in order.
    #[serde(rename = "sequencia")]
    Sequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Function {
    /// Stops the process or makes the error impossible: lowers the occurrence.
    #[default]
    #[serde(rename = "controle")]
    Control,
    /// Signals the error so someone acts: lowers the detection rating.
    #[serde(rename = "alerta")]
    Warning,
}

/// One periodic check of the device.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Check {
    #[serde(rename = "data")]
    pub date: String,
    /// The device worked when tested (a bad part was stopped or signalled).
    #[serde(rename = "funcionou")]
    pub worked: bool,
    #[serde(rename = "quem")]
    pub who: String,
    #[serde(rename = "nota")]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    #[serde(rename = "nome")]
    pub name: String,
    #[serde(rename = "tipo")]
    pub kind: Kind,
    #[serde(rename = "funcao")]
    pub function: Function,
    /// The FMEA row it deals with: the step and the failure mode, as written in the FMEA.
    #[serde(rename = "etapa")]
    pub step: String,
    #[serde(rename = "modo_de_falha")]
    pub failure_mode: String,
    /// Occurrence and detection (1 to 10) before and after the device.
    #[serde(rename = "o_antes")]
    pub o_before: Option<u8>,
    #[serde(rename = "d_antes")]
    pub d_before: Option<u8>,
    #[serde(rename = "o_depois")]
    pub o_after: Option<u8>,
    #[serde(rename = "d_depois")]
    pub d_after: Option<u8>,
    /// Days between checks.
    #[serde(rename = "frequencia_dias")]
    pub every_days: Option<u32>,
    #[serde(rename = "responsavel")]
    pub owner: String,
    #[serde(rename = "verificacoes")]
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Devices {
    #[serde(rename = "dispositivos")]
    pub devices: Vec<Device>,
}

/// What a device did to the FMEA ratings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Effect {
    /// Ratings missing before or after.
    #[default]
    Unknown,
    /// Neither went down.
    None,
    Occurrence,
    Detection,
    Both,
}

/// A point the team should look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Finding {
    /// No FMEA failure mode named.
    NoFailureMode,
    /// A control device that did not lower the occurrence.
    ControlWithoutO,
    /// A warning device that did not lower the detection rating.
    WarningWithoutD,
    /// A rating went up after the device.
    Worse,
    /// No check frequency.
    NoFrequency,
    /// Never checked.
    NeverChecked,
    /// The check is late since this date.
    Late(String),
    /// The last check found it not working.
    LastFailed,
}

impl Finding {
    /// The text in the language of the moment.
    pub fn text(&self) -> String {
        match self {
            Finding::NoFailureMode => tx("poka-yoke-ligue_o_dispositivo_a_um").into(),
            Finding::ControlWithoutO => tx("poka-yoke-dispositivo_de_controle_que_nao").into(),
            Finding::WarningWithoutD => tx("poka-yoke-dispositivo_de_alerta_que_nao").into(),
            Finding::Worse => tx("poka-yoke-uma_nota_subiu_depois_do").into(),
            Finding::NoFrequency => tx("poka-yoke-diga_de_quantos_em_quantos").into(),
            Finding::NeverChecked => tx("poka-yoke-ainda_nao_foi_verificado").into(),
            Finding::Late(d) => fx("poka-yoke-verificacao_atrasada_desde", &[("d", vx(&d))]),
            Finding::LastFailed => tx("poka-yoke-na_ultima_verificacao_ele_nao").into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Status {
    pub effect: Effect,
    /// Occurrence and detection drops (before − after).
    pub o_drop: Option<i32>,
    pub d_drop: Option<i32>,
    pub last_check: Option<String>,
    pub next_check: Option<String>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Outcome {
    /// One per device, in input order.
    pub devices: Vec<Status>,
    /// Devices whose check is late or whose last check failed.
    pub needing_attention: usize,
}

/// Days since 1970-01-01 of an `AAAA-MM-DD` date.
fn days(date: &str) -> Option<i64> {
    let mut it = date.trim().splitn(3, '-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from civil (proleptic Gregorian).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn date_of(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn valid(r: Option<u8>) -> Option<i32> {
    r.filter(|v| (1..=10).contains(v)).map(i32::from)
}

/// The status of every device on `today` (`AAAA-MM-DD`). Pure.
pub fn compute(d: &Devices, today: &str) -> Outcome {
    let today = days(today);
    let devices: Vec<Status> = d
        .devices
        .iter()
        .map(|dev| {
            let mut findings = Vec::new();
            if dev.failure_mode.trim().is_empty() {
                findings.push(Finding::NoFailureMode);
            }
            let o_drop = valid(dev.o_before)
                .zip(valid(dev.o_after))
                .map(|(b, a)| b - a);
            let d_drop = valid(dev.d_before)
                .zip(valid(dev.d_after))
                .map(|(b, a)| b - a);
            let effect = match (o_drop, d_drop) {
                (None, None) => Effect::Unknown,
                (o, d) => match (o.unwrap_or(0) > 0, d.unwrap_or(0) > 0) {
                    (true, true) => Effect::Both,
                    (true, false) => Effect::Occurrence,
                    (false, true) => Effect::Detection,
                    (false, false) => Effect::None,
                },
            };
            if o_drop.is_some_and(|x| x < 0) || d_drop.is_some_and(|x| x < 0) {
                findings.push(Finding::Worse);
            }
            match dev.function {
                Function::Control if o_drop.is_some_and(|x| x <= 0) => {
                    findings.push(Finding::ControlWithoutO)
                }
                Function::Warning if d_drop.is_some_and(|x| x <= 0) => {
                    findings.push(Finding::WarningWithoutD)
                }
                _ => {}
            }
            let last = dev
                .checks
                .iter()
                .filter_map(|c| days(&c.date).map(|n| (n, c)))
                .max_by_key(|(n, _)| *n);
            let every = dev.every_days.filter(|n| *n > 0);
            if every.is_none() {
                findings.push(Finding::NoFrequency);
            }
            let next = match (last, every) {
                (Some((n, _)), Some(e)) => Some(n + i64::from(e)),
                _ => None,
            };
            match last {
                None => findings.push(Finding::NeverChecked),
                Some((_, c)) if !c.worked => findings.push(Finding::LastFailed),
                _ => {}
            }
            if let (Some(n), Some(t)) = (next, today)
                && n < t
            {
                findings.push(Finding::Late(date_of(n)));
            }
            Status {
                effect,
                o_drop,
                d_drop,
                last_check: last.map(|(n, _)| date_of(n)),
                next_check: next.map(date_of),
                findings,
            }
        })
        .collect();
    let needing_attention = devices
        .iter()
        .filter(|s| {
            s.findings
                .iter()
                .any(|f| matches!(f, Finding::Late(_) | Finding::LastFailed))
        })
        .count();
    Outcome {
        devices,
        needing_attention,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(date: &str, worked: bool) -> Check {
        Check {
            date: date.into(),
            worked,
            ..Check::default()
        }
    }

    fn pin() -> Device {
        Device {
            name: "Pino-guia no gabarito".into(),
            kind: Kind::Contact,
            function: Function::Control,
            step: "Furar".into(),
            failure_mode: "Furo fora de posição".into(),
            o_before: Some(6),
            d_before: Some(6),
            o_after: Some(2),
            d_after: Some(6),
            every_days: Some(30),
            checks: vec![check("2026-09-20", true), check("2026-10-20", true)],
            ..Device::default()
        }
    }

    #[test]
    fn dates_round_trip() {
        for d in ["1970-01-01", "2000-02-29", "2026-11-09", "2026-12-31"] {
            assert_eq!(date_of(days(d).unwrap()), d);
        }
        assert_eq!(days("1970-01-02"), Some(1));
        assert_eq!(days("2026-13-01"), None);
        assert_eq!(days("x"), None);
    }

    #[test]
    fn effect_function_and_checks() {
        let o = compute(
            &Devices {
                devices: vec![pin()],
            },
            "2026-11-09",
        );
        let s = &o.devices[0];
        assert_eq!(s.effect, Effect::Occurrence);
        assert_eq!((s.o_drop, s.d_drop), (Some(4), Some(0)));
        assert_eq!(s.last_check.as_deref(), Some("2026-10-20"));
        assert_eq!(s.next_check.as_deref(), Some("2026-11-19"));
        assert!(s.findings.is_empty(), "{:?}", s.findings);
        assert_eq!(o.needing_attention, 0);
        // A month later the check is late.
        let o = compute(
            &Devices {
                devices: vec![pin()],
            },
            "2026-12-01",
        );
        assert_eq!(o.devices[0].findings, [Finding::Late("2026-11-19".into())]);
        assert_eq!(o.needing_attention, 1);
    }

    #[test]
    fn findings_of_a_badly_filled_device() {
        let mut alarm = pin();
        alarm.function = Function::Warning;
        alarm.failure_mode = String::new();
        alarm.d_after = Some(7);
        alarm.every_days = None;
        alarm.checks = vec![check("2026-11-01", false)];
        let o = compute(
            &Devices {
                devices: vec![alarm, Device::default()],
            },
            "2026-11-09",
        );
        let f = &o.devices[0].findings;
        for want in [
            Finding::NoFailureMode,
            Finding::Worse,
            Finding::WarningWithoutD,
            Finding::NoFrequency,
            Finding::LastFailed,
        ] {
            assert!(f.contains(&want), "{want:?} in {f:?}");
        }
        assert_eq!(o.devices[1].effect, Effect::Unknown);
        assert!(o.devices[1].findings.contains(&Finding::NeverChecked));
        assert_eq!(o.needing_attention, 1);
        assert!(
            Finding::Late("2026-11-19".into())
                .text()
                .contains("2026-11-19")
        );
    }

    #[test]
    fn exchange_format() {
        let d = Devices {
            devices: vec![pin()],
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"modo_de_falha\":\"Furo fora de posição\""));
        assert!(json.contains("\"contato\"") && json.contains("\"controle\""));
        assert_eq!(serde_json::from_str::<Devices>(&json).unwrap(), d);
        assert_eq!(
            serde_json::from_str::<Devices>("{}").unwrap(),
            Devices::default()
        );
    }
}
