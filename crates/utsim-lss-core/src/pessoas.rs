//! Periodic import of the program's people from a CSV exported by HR (P9 of
//! `docs/plano_lean_seis_sigma.md`): name and e-mail, and optionally the business unit (by
//! name), the area, the belt level and the hours per week. A person is found by the e-mail
//! (case and spaces ignored): an existing one has the changed fields updated, a new one is
//! created; nothing is removed (a person who left is handled by the anonymisation). Each
//! line that cannot be read is reported with its number and reason, and the rest goes in.

use crate::domain::{enum_value, fields as dfields};
use crate::id::Id;
use crate::op::Edit;
use crate::programa::{self, BeltLevel, Member, fields as pf};
use crate::state::State;

/// What happened to each line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub created: Vec<String>,
    /// Name and the fields changed.
    pub updated: Vec<(String, Vec<&'static str>)>,
    pub unchanged: usize,
    /// Line number (1 is the header) and the reason.
    pub errors: Vec<(usize, String)>,
}

fn fold(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace(['á', 'à', 'ã', 'â'], "a")
        .replace(['é', 'ê'], "e")
        .replace('í', "i")
        .replace(['ó', 'õ', 'ô'], "o")
        .replace('ú', "u")
        .replace('ç', "c")
}

/// A belt level as HR writes it: `green belt`, `verde`, `GB`, `black`, `MBB`...
pub fn parse_level(s: &str) -> Option<BeltLevel> {
    let t = fold(s).replace(['-', '_'], " ");
    let t = t.trim();
    match t {
        "yb" | "yellow" | "yellow belt" | "amarela" | "faixa amarela" => Some(BeltLevel::Yellow),
        "gb" | "green" | "green belt" | "verde" | "faixa verde" => Some(BeltLevel::Green),
        "bb" | "black" | "black belt" | "preta" | "faixa preta" => Some(BeltLevel::Black),
        "mbb" | "master" | "master black belt" | "master black" => Some(BeltLevel::MasterBlack),
        _ => None,
    }
}

/// The edits that bring the program's people in line with the CSV (`;` or `,`, a header
/// with `nome` and `email` and, optionally, `unidade`, `area`, `nivel`, `horas_semana`).
pub fn import(
    state: &State,
    program: Id,
    csv: &str,
    mut new_id: impl FnMut() -> Id,
) -> (Vec<Edit>, Report) {
    let mut report = Report::default();
    let mut edits = Vec::new();
    let mut lines = csv
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let Some((_, head)) = lines.next() else {
        report.errors.push((1, "arquivo vazio".into()));
        return (edits, report);
    };
    let sep = if head.contains(';') { ';' } else { ',' };
    let cols: Vec<String> = head.split(sep).map(fold).collect();
    let col = |names: &[&str]| cols.iter().position(|c| names.contains(&c.as_str()));
    let (Some(c_name), Some(c_mail)) = (col(&["nome", "name"]), col(&["email", "e-mail"])) else {
        report
            .errors
            .push((1, "o cabeçalho precisa das colunas nome e email".into()));
        return (edits, report);
    };
    let c_unit = col(&["unidade", "unidade de negocio", "planta"]);
    let c_area = col(&["area", "setor", "departamento"]);
    let c_level = col(&["nivel", "faixa", "belt"]);
    let c_hours = col(&["horas_semana", "horas por semana", "horas"]);
    let units = programa::units(state, program);
    let members = programa::members(state);
    let mut seen: Vec<String> = Vec::new();
    for (i, line) in lines {
        let n = i + 1;
        let cells: Vec<String> = line
            .split(sep)
            .map(|c| c.trim().trim_matches('"').to_string())
            .collect();
        let get = |c: Option<usize>| c.and_then(|c| cells.get(c)).cloned().unwrap_or_default();
        let name = get(Some(c_name));
        let mail = get(Some(c_mail));
        let key = mail.trim().to_lowercase();
        if name.trim().is_empty() || !key.contains('@') {
            report.errors.push((n, "falta o nome ou o e-mail".into()));
            continue;
        }
        if seen.contains(&key) {
            report
                .errors
                .push((n, format!("{key} repetido no arquivo")));
            continue;
        }
        seen.push(key.clone());
        let unit_txt = get(c_unit);
        let unit = if unit_txt.trim().is_empty() {
            None
        } else {
            match units.iter().find(|u| fold(&u.name) == fold(&unit_txt)) {
                Some(u) => Some(u.id),
                None => {
                    report
                        .errors
                        .push((n, format!("unidade desconhecida: {}", unit_txt.trim())));
                    continue;
                }
            }
        };
        let level_txt = get(c_level);
        let level = if level_txt.trim().is_empty() {
            None
        } else {
            match parse_level(&level_txt) {
                Some(l) => Some(l),
                None => {
                    report
                        .errors
                        .push((n, format!("nível desconhecido: {}", level_txt.trim())));
                    continue;
                }
            }
        };
        let hours_txt = get(c_hours);
        let hours = if hours_txt.trim().is_empty() {
            None
        } else {
            match hours_txt.trim().replace(',', ".").parse::<f64>() {
                Ok(h) if (0.0..=60.0).contains(&h) => Some(h),
                _ => {
                    report.errors.push((
                        n,
                        format!("horas por semana inválidas: {}", hours_txt.trim()),
                    ));
                    continue;
                }
            }
        };
        let area = get(c_area);
        let existing = members
            .iter()
            .find(|m| m.email.trim().to_lowercase() == key);
        let (id, old) = match existing {
            Some(m) => (m.id, Some(m)),
            None => (new_id(), None),
        };
        let r = Member::entity_ref(id);
        let mut changed: Vec<&'static str> = Vec::new();
        let mut push =
            |field: &'static str, label: &'static str, v: serde_json::Value, same: bool| {
                if !same {
                    edits.push(Edit::new(r, field, v));
                    changed.push(label);
                }
            };
        push(
            dfields::person::NAME,
            "nome",
            name.trim().into(),
            old.is_some_and(|m| m.name == name.trim()),
        );
        push(
            dfields::person::EMAIL,
            "e-mail",
            mail.trim().into(),
            old.is_some_and(|m| m.email.trim().eq_ignore_ascii_case(mail.trim())),
        );
        if let Some(u) = unit {
            push(
                pf::UNIT,
                "unidade",
                u.to_string().into(),
                old.is_some_and(|m| m.unit == Some(u)),
            );
        }
        if !area.trim().is_empty() {
            push(
                pf::AREA,
                "área",
                area.trim().into(),
                old.is_some_and(|m| m.area == area.trim()),
            );
        }
        if let Some(l) = level {
            push(
                pf::LEVEL,
                "nível",
                enum_value(l),
                old.is_some_and(|m| m.level == Some(l)),
            );
        }
        if let Some(h) = hours {
            push(
                pf::HOURS_WEEK,
                "horas",
                h.into(),
                old.is_some_and(|m| m.hours_week == Some(h)),
            );
        }
        match (old, changed.is_empty()) {
            (None, _) => report.created.push(name.trim().to_string()),
            (Some(_), true) => report.unchanged += 1,
            (Some(m), false) => report.updated.push((m.name.clone(), changed)),
        }
    }
    (edits, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::programa::testkit::*;

    #[test]
    fn create_update_and_report() {
        let mut t = T::new();
        let p = t.program;
        let u = t.id();
        t.commit(programa::add_unit(p, u, "Planta Sul"));
        let bia = t.person("Bia Reis", Some(BeltLevel::Yellow), None);
        t.set(
            crate::op::Kind::Person,
            bia,
            dfields::person::EMAIL,
            "bia@empresa.com",
        );
        let csv = "Nome;E-mail;Unidade;Área;Nível;Horas por semana\n\
                   Bia Reis;BIA@empresa.com ;planta sul;Usinagem;Green Belt;8\n\
                   Caio Lima;caio@empresa.com;Planta Sul;Montagem;BB;12,5\n\
                   Dora Melo;dora@empresa.com;Planta Norte;;;\n\
                   ;sem@nome.com;;;;\n\
                   Caio L.;caio@empresa.com;;;;\n\
                   Eva Dias;eva@empresa.com;;;faixa roxa;\n";
        let mut n = 70_000;
        let (e, r) = import(&t.s, p, csv, || {
            n += 1;
            Id::v7(n, [7; 10])
        });
        t.commit(e);
        assert_eq!(r.created, vec!["Caio Lima".to_string()]);
        assert_eq!(r.updated.len(), 1);
        assert_eq!(r.updated[0].0, "Bia Reis");
        assert!(r.updated[0].1.contains(&"nível") && r.updated[0].1.contains(&"unidade"));
        assert_eq!(
            r.errors.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
            vec![4, 5, 6, 7]
        );
        let ms = programa::members(&t.s);
        let b = ms.iter().find(|m| m.id == bia).unwrap();
        assert_eq!(
            (b.level, b.unit, b.area.as_str()),
            (Some(BeltLevel::Green), Some(u), "Usinagem")
        );
        let c = ms.iter().find(|m| m.name == "Caio Lima").unwrap();
        assert_eq!(
            (c.level, c.hours_week),
            (Some(BeltLevel::Black), Some(12.5))
        );
        // The same file again changes nothing.
        let (e2, r2) = import(&t.s, p, csv, || Id::v7(1, [1; 10]));
        assert!(e2.is_empty());
        assert_eq!((r2.unchanged, r2.created.len()), (2, 0));
        assert_eq!(
            import(&t.s, p, "a;b\n1;2\n", || Id::v7(1, [1; 10]))
                .1
                .errors[0]
                .0,
            1
        );
        assert_eq!(
            parse_level("Master Black Belt"),
            Some(BeltLevel::MasterBlack)
        );
    }
}
