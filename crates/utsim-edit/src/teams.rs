//! Team and request links (`docs/plano_chegadas_e_equipes_3d.md`, E1a): who is in a
//! dispatcher's team and who asks whom for transport or operators. Each helper checks
//! the request against the model and returns one [`Command`] (one undo step) that edits
//! the existing fields (`team`, `transport`, `operators`): the links need no new format.

use utsim_model::{Model, ObjectKind, is_mobile};
use utsim_objects::OperatorUse;

use crate::Command;

fn kind_of<'a>(model: &'a Model, name: &str) -> Result<&'a ObjectKind, String> {
    model
        .objects
        .iter()
        .find(|o| o.name == name)
        .map(|o| &o.kind)
        .ok_or_else(|| format!("objeto '{name}' não existe"))
}

fn team_of<'a>(model: &'a Model, dispatcher: &str) -> Result<&'a Vec<String>, String> {
    match kind_of(model, dispatcher)? {
        ObjectKind::Dispatcher { team, .. } => Ok(team),
        _ => Err(format!("'{dispatcher}' não é um despachante")),
    }
}

fn set_team(dispatcher: &str, team: &[String]) -> Command {
    Command::Set {
        key: format!("{dispatcher}.team"),
        value: ron::ser::to_string(team).expect("a list of names serializes"),
    }
}

/// Puts `member` (a transporter) in the team of `dispatcher`. A transporter is in one
/// team only: when it was in another one, it leaves it in the same step.
pub fn team_add(model: &Model, dispatcher: &str, member: &str) -> Result<Command, String> {
    let team = team_of(model, dispatcher)?;
    if !matches!(kind_of(model, member)?, ObjectKind::Transporter { .. }) {
        return Err(format!("'{member}' não é um transportador"));
    }
    if team.iter().any(|m| m == member) {
        return Err(format!("'{member}' já está na equipe de '{dispatcher}'"));
    }
    let mut commands = Vec::new();
    for o in &model.objects {
        if let ObjectKind::Dispatcher { team: other, .. } = &o.kind {
            if o.name != dispatcher && other.iter().any(|m| m == member) {
                let rest: Vec<String> = other.iter().filter(|m| *m != member).cloned().collect();
                commands.push(set_team(&o.name, &rest));
            }
        }
    }
    let mut new_team = team.clone();
    new_team.push(member.to_string());
    commands.push(set_team(dispatcher, &new_team));
    Ok(if commands.len() == 1 {
        commands.remove(0)
    } else {
        Command::Batch(commands)
    })
}

/// Takes `member` out of the team of `dispatcher`.
pub fn team_remove(model: &Model, dispatcher: &str, member: &str) -> Result<Command, String> {
    let team = team_of(model, dispatcher)?;
    if !team.iter().any(|m| m == member) {
        return Err(format!("'{member}' não está na equipe de '{dispatcher}'"));
    }
    let rest: Vec<String> = team.iter().filter(|m| *m != member).cloned().collect();
    Ok(set_team(dispatcher, &rest))
}

/// The drag of the editor: adds `member` to the team, or removes it when it is there.
pub fn team_toggle(model: &Model, dispatcher: &str, member: &str) -> Result<Command, String> {
    if team_of(model, dispatcher)?.iter().any(|m| m == member) {
        team_remove(model, dispatcher, member)
    } else {
        team_add(model, dispatcher, member)
    }
}

/// Sets (or, with `None`, clears) who carries the items out of `object`: a transporter
/// or a dispatcher.
pub fn set_transport(
    model: &Model,
    object: &str,
    carrier: Option<&str>,
) -> Result<Command, String> {
    if is_mobile(kind_of(model, object)?) {
        return Err(format!(
            "'{object}' é um recurso móvel e não pede transporte"
        ));
    }
    if let Some(c) = carrier {
        if !is_mobile(kind_of(model, c)?) {
            return Err(format!("'{c}' precisa ser um transportador ou despachante"));
        }
    }
    Ok(Command::Set {
        key: format!("{object}.transport"),
        value: match carrier {
            Some(c) => ron::ser::to_string(c).expect("a name serializes"),
            None => "None".to_string(),
        },
    })
}

/// Sets (or, with `None`, clears) the operators a processor asks for.
pub fn set_operators(
    model: &Model,
    processor: &str,
    operators: Option<&OperatorUse>,
) -> Result<Command, String> {
    if !matches!(kind_of(model, processor)?, ObjectKind::Processor { .. }) {
        return Err(format!("'{processor}' não é um processo"));
    }
    if let Some(op) = operators {
        if !is_mobile(kind_of(model, &op.team)?) {
            return Err(format!(
                "'{}' precisa ser um transportador ou despachante",
                op.team
            ));
        }
    }
    Ok(Command::Set {
        key: format!("{processor}.operators"),
        value: match operators {
            Some(op) => ron::ser::to_string(op).expect("operators serialize"),
            None => "None".to_string(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    const TEXT: &str = r#"Model(
        name: "equipes",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Exp(mean: 5.0))),
            (name: "q", kind: Queue()),
            (name: "m", kind: Processor(process: Exp(mean: 2.0))),
            (name: "out", kind: Sink),
            (name: "a", kind: Transporter(speed: 1.0)),
            (name: "b", kind: Transporter(speed: 1.0)),
            (name: "eq1", kind: Dispatcher(team: ["a"])),
            (name: "eq2", kind: Dispatcher(team: ["b"])),
        ],
        connections: [("src", "q"), ("q", "m"), ("m", "out")],
    )"#;

    fn doc() -> Document {
        let mut d = Document::new("x");
        for o in Model::parse(TEXT).unwrap().objects {
            d.apply(Command::Add(Box::new(o))).unwrap();
        }
        for (a, b) in [("src", "q"), ("q", "m"), ("m", "out")] {
            d.apply(Command::Connect {
                from: a.into(),
                to: b.into(),
            })
            .unwrap();
        }
        d
    }

    fn team(d: &Document, name: &str) -> Vec<String> {
        match &d
            .model()
            .objects
            .iter()
            .find(|o| o.name == name)
            .unwrap()
            .kind
        {
            ObjectKind::Dispatcher { team, .. } => team.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn add_moves_a_member_between_teams_in_one_step() {
        let mut d = doc();
        let c = team_add(d.model(), "eq1", "b").unwrap();
        d.apply(c).unwrap();
        assert_eq!(team(&d, "eq1"), ["a", "b"]);
        assert!(team(&d, "eq2").is_empty());
        assert!(d.undo());
        assert_eq!(team(&d, "eq1"), ["a"]);
        assert_eq!(team(&d, "eq2"), ["b"]);
        assert!(d.redo());
        assert_eq!(team(&d, "eq1"), ["a", "b"]);
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut d = doc();
        let c = team_toggle(d.model(), "eq1", "a").unwrap();
        d.apply(c).unwrap();
        assert!(team(&d, "eq1").is_empty());
        let c = team_toggle(d.model(), "eq1", "a").unwrap();
        d.apply(c).unwrap();
        assert_eq!(team(&d, "eq1"), ["a"]);
        assert!(
            team_add(d.model(), "eq1", "q")
                .unwrap_err()
                .contains("transportador")
        );
        assert!(
            team_remove(d.model(), "eq2", "a")
                .unwrap_err()
                .contains("não está")
        );
        assert!(
            team_add(d.model(), "q", "a")
                .unwrap_err()
                .contains("despachante")
        );
    }

    #[test]
    fn transport_and_operators_links() {
        let mut d = doc();
        let c = set_transport(d.model(), "q", Some("eq1")).unwrap();
        d.apply(c).unwrap();
        let q = d.model().objects.iter().find(|o| o.name == "q").unwrap();
        assert_eq!(q.transport.as_deref(), Some("eq1"));
        let c = set_transport(d.model(), "q", None).unwrap();
        d.apply(c).unwrap();
        let q = d.model().objects.iter().find(|o| o.name == "q").unwrap();
        assert_eq!(q.transport, None);
        assert!(set_transport(d.model(), "a", Some("eq1")).is_err());
        assert!(set_transport(d.model(), "q", Some("m")).is_err());

        let op: OperatorUse = ron::from_str(r#"(team: "eq2", count: 2, process: false)"#).unwrap();
        let c = set_operators(d.model(), "m", Some(&op)).unwrap();
        d.apply(c).unwrap();
        let m = d.model().objects.iter().find(|o| o.name == "m").unwrap();
        let ObjectKind::Processor { operators, .. } = &m.kind else {
            unreachable!()
        };
        assert_eq!(operators.as_ref(), Some(&op));
        assert!(d.undo());
        let m = d.model().objects.iter().find(|o| o.name == "m").unwrap();
        assert!(matches!(
            m.kind,
            ObjectKind::Processor {
                operators: None,
                ..
            }
        ));
        assert!(set_operators(d.model(), "q", Some(&op)).is_err());
    }
}
