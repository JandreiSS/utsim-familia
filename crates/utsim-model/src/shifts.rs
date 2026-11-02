//! The `shifts` field of an object: an inline shift table or a reference to one of the
//! model's named calendars (`calendars`).
//!
//! Accepted forms (RON):
//!
//! ```text
//! shifts: (period: 1440.0, off: [(start: 300.0, duration: 1140.0)])            // inline
//! shifts: (period: 1440.0, off: [...], on_end: Finish)                       // inline, finish
//! shifts: Calendar("turno_s1")                                               // calendar
//! shifts: Calendar("turno_s1", Finish)                                       // calendar, own on_end
//! shifts: (calendar: "turno_s1", on_end: Finish)                             // same, long form
//! ```
//!
//! `Calendario`, `calendario` and `ao_fim_do_turno` (`Pausar`, `Terminar`) are accepted
//! too. The parser cannot see the name in front of the parentheses (RON drops it in
//! self-describing reads), so any `Name("...")` is read as a calendar reference.

use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utsim_core::{OffWindow, ShiftEnd, ShiftTable};

/// Shifts of one object.
#[derive(Clone, Debug, PartialEq)]
pub enum ShiftSpec {
    /// The windows written in the object itself.
    Inline(ShiftTable),
    /// A calendar of the model (`Model::calendars`), optionally with its own `on_end`.
    Calendar {
        name: String,
        on_end: Option<ShiftEnd>,
    },
}

impl ShiftSpec {
    /// A reference to calendar `name` that keeps the calendar's `on_end`.
    pub fn calendar(name: impl Into<String>) -> ShiftSpec {
        ShiftSpec::Calendar {
            name: name.into(),
            on_end: None,
        }
    }

    /// Name of the referenced calendar, if any.
    pub fn calendar_name(&self) -> Option<&str> {
        match self {
            ShiftSpec::Inline(_) => None,
            ShiftSpec::Calendar { name, .. } => Some(name),
        }
    }
}

impl From<ShiftTable> for ShiftSpec {
    fn from(table: ShiftTable) -> Self {
        ShiftSpec::Inline(table)
    }
}

impl Serialize for ShiftSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ShiftSpec::Inline(table) => table.serialize(serializer),
            ShiftSpec::Calendar { name, on_end: None } => {
                serializer.serialize_newtype_variant("ShiftSpec", 1, "Calendar", name)
            }
            ShiftSpec::Calendar {
                name,
                on_end: Some(end),
            } => {
                let mut s = serializer.serialize_struct("ShiftCalendar", 2)?;
                s.serialize_field("calendar", name)?;
                s.serialize_field("on_end", end)?;
                s.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ShiftSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SpecVisitor)
    }
}

struct SpecVisitor;

/// Keys of the struct forms. Read as identifiers (RON refuses struct keys read as
/// strings).
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
    Period,
    Off,
    #[serde(alias = "ao_fim_do_turno")]
    OnEnd,
    /// `calendar: "x"` (long form) or `{"Calendar": "x"}` (JSON of the variant).
    #[serde(alias = "calendario", alias = "Calendar", alias = "Calendario")]
    Calendar,
}

const EXPECTED: &str = "uma tabela de turnos (period: ..., off: [...]) ou Calendar(\"nome\")";

impl<'de> Visitor<'de> for SpecVisitor {
    type Value = ShiftSpec;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(EXPECTED)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<ShiftSpec, E> {
        Ok(ShiftSpec::calendar(v))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<ShiftSpec, D::Error> {
        Ok(ShiftSpec::calendar(String::deserialize(d)?))
    }

    /// `Calendar("name")` or `Calendar("name", Finish)`.
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ShiftSpec, A::Error> {
        let name: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::custom("Calendar(...) precisa do nome do calendário"))?;
        let on_end: Option<ShiftEnd> = seq.next_element()?;
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::custom(
                "Calendar aceita no máximo o nome e o comportamento no fim do turno",
            ));
        }
        Ok(ShiftSpec::Calendar { name, on_end })
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ShiftSpec, A::Error> {
        let mut period: Option<f64> = None;
        let mut off: Option<Vec<OffWindow>> = None;
        let mut on_end: Option<ShiftEnd> = None;
        let mut calendar: Option<String> = None;
        while let Some(key) = map.next_key::<Field>()? {
            match key {
                Field::Period => period = Some(map.next_value()?),
                Field::Off => off = Some(map.next_value()?),
                Field::OnEnd => on_end = Some(map.next_value()?),
                Field::Calendar => calendar = Some(map.next_value()?),
            }
        }
        if let Some(name) = calendar {
            if period.is_some() || off.is_some() {
                return Err(de::Error::custom(
                    "turnos: use `calendar` ou `period`/`off`, não os dois",
                ));
            }
            return Ok(ShiftSpec::Calendar { name, on_end });
        }
        Ok(ShiftSpec::Inline(ShiftTable {
            period: period.ok_or_else(|| de::Error::missing_field("period"))?,
            off: off.ok_or_else(|| de::Error::missing_field("off"))?,
            on_end: on_end.unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ShiftSpec {
        ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
            .from_str(text)
            .unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn table(on_end: ShiftEnd) -> ShiftTable {
        ShiftTable {
            period: 10.0,
            off: vec![OffWindow {
                start: 4.0,
                duration: 2.0,
            }],
            on_end,
        }
    }

    #[test]
    fn every_form_parses() {
        let inline = ShiftSpec::Inline(table(ShiftEnd::Pause));
        assert_eq!(
            parse("(period: 10.0, off: [(start: 4.0, duration: 2.0)])"),
            inline
        );
        assert_eq!(
            parse("(period: 10, off: [(start: 4, duration: 2)])"),
            inline
        );
        assert_eq!(
            parse("ShiftTable(period: 10.0, off: [(start: 4.0, duration: 2.0)])"),
            inline
        );
        let finish = ShiftSpec::Inline(table(ShiftEnd::Finish));
        assert_eq!(
            parse("(period: 10.0, off: [(start: 4.0, duration: 2.0)], on_end: Finish)"),
            finish
        );
        assert_eq!(
            parse("(period: 10.0, off: [(start: 4.0, duration: 2.0)], ao_fim_do_turno: Terminar)"),
            finish
        );
        assert_eq!(parse("Calendar(\"adm\")"), ShiftSpec::calendar("adm"));
        assert_eq!(parse("Calendario(\"adm\")"), ShiftSpec::calendar("adm"));
        let own = ShiftSpec::Calendar {
            name: "adm".into(),
            on_end: Some(ShiftEnd::Finish),
        };
        assert_eq!(parse("Calendar(\"adm\", Finish)"), own);
        assert_eq!(parse("(calendar: \"adm\", on_end: Finish)"), own);
        assert_eq!(
            parse("(calendario: \"adm\", ao_fim_do_turno: Terminar)"),
            own
        );
    }

    #[test]
    fn mixed_or_incomplete_forms_are_rejected() {
        let options = ron::Options::default();
        for bad in [
            "(calendar: \"a\", period: 10.0)",
            "(period: 10.0)",
            "(off: [])",
            "(period: 10.0, off: [], extra: 1)",
        ] {
            assert!(options.from_str::<ShiftSpec>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn writing_and_reading_back_gives_the_same_value() {
        for spec in [
            ShiftSpec::Inline(table(ShiftEnd::Pause)),
            ShiftSpec::Inline(table(ShiftEnd::Finish)),
            ShiftSpec::calendar("turno \"a\""),
            ShiftSpec::Calendar {
                name: "b".into(),
                on_end: Some(ShiftEnd::Pause),
            },
        ] {
            for names in [false, true] {
                let config = ron::ser::PrettyConfig::new().struct_names(names);
                let text = ron::ser::to_string_pretty(&spec, config).unwrap();
                assert_eq!(parse(&text), spec, "{text}");
            }
            let json = serde_json::to_string(&spec).unwrap();
            assert_eq!(
                serde_json::from_str::<ShiftSpec>(&json).unwrap(),
                spec,
                "{json}"
            );
        }
        // Without `on_end` the inline text is exactly the old one.
        let text = ron::ser::to_string(&ShiftSpec::Inline(table(ShiftEnd::Pause))).unwrap();
        assert_eq!(text, "(period:10.0,off:[(start:4.0,duration:2.0)])");
    }
}
