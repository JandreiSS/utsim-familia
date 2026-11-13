//! Editing the circulation aisles (`Model::aisles`), the floor marking, the access point of
//! the objects and the movement mode of the mobile resources.
//!
//! The commands live in [`crate::Command`] (`AddAisle`, `SetAisle`, `RemoveAisle`,
//! `SetAccess`, `SetMovement`, `SetClearance`, `SetRunMovement`, `SetFloorMarking`); this
//! module executes them on a model. An aisle is replaced whole (`SetAisle`), so moving a
//! vertex, changing the width, the kind, the direction or the profile and renaming it are
//! all the same command. Whether the aisles make a sensible network is the business of
//! [`utsim_model::Model::validate`] and of the warnings, not of the edit.

use utsim_model::{Aisle, FloorMarking, Model, Movement, ObjectKind};

/// Base of the names of new aisles (`corredor_1`, `corredor_2`...).
pub const AISLE_BASE: &str = "corredor";

fn not_found(name: &str) -> String {
    format!("corredor \"{name}\" não existe")
}

/// Whether the aisle can be stored (the same checks as the model's validation).
fn check(aisle: &Aisle) -> Result<(), String> {
    if aisle.name.trim().is_empty() {
        return Err("o corredor precisa de um nome".into());
    }
    match aisle.problem() {
        Some(p) => Err(p),
        None => Ok(()),
    }
}

pub(crate) fn add_aisle(model: &mut Model, aisle: &Aisle) -> Result<(), String> {
    check(aisle)?;
    if model.aisles.iter().any(|a| a.name == aisle.name) {
        return Err(format!("já existe um corredor chamado \"{}\"", aisle.name));
    }
    model.aisles.push(aisle.clone());
    Ok(())
}

pub(crate) fn set_aisle(model: &mut Model, name: &str, aisle: &Aisle) -> Result<(), String> {
    check(aisle)?;
    let i = model
        .aisles
        .iter()
        .position(|a| a.name == name)
        .ok_or_else(|| not_found(name))?;
    if aisle.name != name && model.aisles.iter().any(|a| a.name == aisle.name) {
        return Err(format!("já existe um corredor chamado \"{}\"", aisle.name));
    }
    model.aisles[i] = aisle.clone();
    Ok(())
}

pub(crate) fn remove_aisle(model: &mut Model, name: &str) -> Result<(), String> {
    let before = model.aisles.len();
    model.aisles.retain(|a| a.name != name);
    if model.aisles.len() == before {
        return Err(not_found(name));
    }
    Ok(())
}

pub(crate) fn set_access(
    model: &mut Model,
    name: &str,
    access: Option<[f64; 2]>,
) -> Result<(), String> {
    if access.is_some_and(|a| !a.iter().all(|v| v.is_finite())) {
        return Err("o ponto de acesso precisa de coordenadas finitas".into());
    }
    let obj = model
        .objects
        .iter_mut()
        .find(|o| o.name == name)
        .ok_or_else(|| format!("objeto \"{name}\" não existe"))?;
    obj.access = access;
    Ok(())
}

pub(crate) fn set_size(
    model: &mut Model,
    name: &str,
    size: Option<[f64; 3]>,
) -> Result<(), String> {
    if size.is_some_and(|d| !d.iter().all(|v| v.is_finite() && *v > 0.0)) {
        return Err("as dimensões precisam ser finitas e maiores que zero".into());
    }
    let obj = model
        .objects
        .iter_mut()
        .find(|o| o.name == name)
        .ok_or_else(|| format!("objeto \"{name}\" não existe"))?;
    if size.is_some() && utsim_model::is_mobile(&obj.kind) {
        return Err("recursos móveis não têm dimensões próprias".into());
    }
    if size.is_some()
        && matches!(
            obj.kind,
            ObjectKind::Conveyor { .. } | ObjectKind::Rack { .. }
        )
    {
        return Err("a esteira e o porta-paletes têm as dimensões nos seus campos".into());
    }
    obj.size = size;
    Ok(())
}

pub(crate) fn set_area_per_item(
    model: &mut Model,
    name: &str,
    area: Option<f64>,
) -> Result<(), String> {
    if area.is_some_and(|a| !(a.is_finite() && a > 0.0)) {
        return Err("a área por item precisa ser finita e maior que zero".into());
    }
    let obj = model
        .objects
        .iter_mut()
        .find(|o| o.name == name)
        .ok_or_else(|| format!("objeto \"{name}\" não existe"))?;
    if area.is_some() && !matches!(obj.kind, ObjectKind::Queue { .. }) {
        return Err("só a espera tem área por item".into());
    }
    obj.area_per_item = area;
    Ok(())
}

fn transporter<'a>(model: &'a mut Model, name: &str) -> Result<&'a mut ObjectKind, String> {
    let obj = model
        .objects
        .iter_mut()
        .find(|o| o.name == name)
        .ok_or_else(|| format!("objeto \"{name}\" não existe"))?;
    match obj.kind {
        ObjectKind::Transporter { .. } => Ok(&mut obj.kind),
        _ => Err(format!("\"{name}\" não é um recurso móvel (transportador)")),
    }
}

pub(crate) fn set_movement(
    model: &mut Model,
    name: &str,
    value: Option<Movement>,
) -> Result<(), String> {
    if let ObjectKind::Transporter { movement, .. } = transporter(model, name)? {
        *movement = value;
    }
    Ok(())
}

pub(crate) fn set_clearance(
    model: &mut Model,
    name: &str,
    value: Option<f64>,
) -> Result<(), String> {
    if value.is_some_and(|c| !(c.is_finite() && c >= 0.0)) {
        return Err("a folga deve ser um número >= 0 (metros)".into());
    }
    if let ObjectKind::Transporter { clearance, .. } = transporter(model, name)? {
        *clearance = value;
    }
    Ok(())
}

pub(crate) fn set_run_movement(model: &mut Model, value: Option<Movement>) {
    model.run.movement = value;
}

pub(crate) fn set_floor_marking(
    model: &mut Model,
    marking: &Option<Box<FloorMarking>>,
) -> Result<(), String> {
    if let Some(p) = marking.as_ref().and_then(|m| m.problem()) {
        return Err(p);
    }
    // The default marking is the same as none: the saved file stays as before.
    model.floor_marking = marking
        .as_ref()
        .filter(|m| ***m != FloorMarking::default())
        .cloned();
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Command, Document, from_palette};
    use utsim_model::{Aisle, AisleKind, FloorMarking, Movement, ObjectKind, Profile};

    fn aisle(name: &str, kind: AisleKind, points: &[[f64; 2]]) -> Box<Aisle> {
        Box::new(Aisle {
            name: name.into(),
            kind,
            points: points.to_vec(),
            width: 1.2,
            two_way: true,
            secondary: false,
            profile: None,
            heights: Vec::new(),
            radii: Vec::new(),
        })
    }

    #[test]
    fn aisles_are_added_replaced_renamed_and_removed_with_undo() {
        let mut doc = Document::new("c");
        let a = aisle("a", AisleKind::Pedestrian, &[[0.0, 0.0], [5.0, 0.0]]);
        doc.apply(Command::AddAisle(a.clone())).unwrap();
        assert!(doc.apply(Command::AddAisle(a.clone())).is_err(), "twice");
        assert!(
            doc.apply(Command::AddAisle(aisle(
                "b",
                AisleKind::Mixed,
                &[[1.0, 1.0]]
            )))
            .is_err(),
            "one point"
        );
        let mut moved = a.clone();
        moved.points[1] = [5.0, 3.0];
        moved.name = "z".into();
        moved.two_way = false;
        doc.apply(Command::SetAisle {
            name: "a".into(),
            aisle: moved,
        })
        .unwrap();
        assert_eq!(doc.model().aisles[0].name, "z");
        assert!(!doc.model().aisles[0].two_way);
        assert!(
            doc.apply(Command::SetAisle {
                name: "a".into(),
                aisle: a.clone(),
            })
            .is_err(),
            "the old name is gone"
        );
        doc.apply(Command::RemoveAisle { name: "z".into() })
            .unwrap();
        assert!(doc.model().aisles.is_empty());
        assert!(
            doc.apply(Command::RemoveAisle { name: "z".into() })
                .is_err()
        );
        doc.undo();
        assert_eq!(doc.model().aisles[0].points[1], [5.0, 3.0]);
        doc.undo();
        doc.undo();
        assert!(doc.model().aisles.is_empty());
        assert_eq!(doc.unique_aisle_name(), "corredor_1");
    }

    #[test]
    fn a_name_that_is_taken_cannot_be_used_by_a_rename() {
        let mut doc = Document::new("c");
        for n in ["a", "b"] {
            doc.apply(Command::AddAisle(aisle(
                n,
                AisleKind::Forklift,
                &[[0.0, 0.0], [4.0, 0.0]],
            )))
            .unwrap();
        }
        assert_eq!(doc.unique_aisle_name(), "corredor_1");
        let mut b = Box::new(doc.model().aisles[1].clone());
        b.name = "a".into();
        assert!(
            doc.apply(Command::SetAisle {
                name: "b".into(),
                aisle: b,
            })
            .is_err()
        );
    }

    #[test]
    fn movement_clearance_access_and_marking_are_commands() {
        let mut doc = Document::new("m");
        let emp = from_palette("Transporter", "emp", [0.0; 3]).unwrap();
        let name = emp.name.clone();
        doc.apply(Command::Add(Box::new(emp))).unwrap();
        let fila = from_palette("Queue", "fila", [5.0, 0.0, 0.0]).unwrap();
        let fila_name = fila.name.clone();
        doc.apply(Command::Add(Box::new(fila))).unwrap();
        doc.apply(Command::SetMovement {
            name: name.clone(),
            movement: Some(Movement::Aisles),
        })
        .unwrap();
        doc.apply(Command::SetClearance {
            name: name.clone(),
            clearance: Some(1.1),
        })
        .unwrap();
        let t = doc.model().objects.iter().find(|o| o.name == name).unwrap();
        assert!(matches!(
            t.kind,
            ObjectKind::Transporter {
                movement: Some(Movement::Aisles),
                clearance: Some(c),
                ..
            } if c == 1.1
        ));
        assert!(
            doc.apply(Command::SetClearance {
                name: name.clone(),
                clearance: Some(-1.0),
            })
            .is_err()
        );
        assert!(
            doc.apply(Command::SetMovement {
                name: fila_name.clone(),
                movement: Some(Movement::Free),
            })
            .is_err(),
            "a queue is not a mobile resource"
        );
        doc.apply(Command::SetAccess {
            name: fila_name.clone(),
            access: Some([1.5, 0.0]),
        })
        .unwrap();
        let f = doc
            .model()
            .objects
            .iter()
            .find(|o| o.name == fila_name)
            .unwrap();
        assert_eq!(f.access, Some([1.5, 0.0]));
        doc.apply(Command::SetRunMovement(Some(Movement::Network)))
            .unwrap();
        assert_eq!(doc.model().run.movement, Some(Movement::Network));
        let marking = FloorMarking {
            profile: Profile::Branco,
            ..FloorMarking::default()
        };
        doc.apply(Command::SetFloorMarking(Some(Box::new(marking.clone()))))
            .unwrap();
        assert_eq!(doc.model().marking(), marking);
        doc.apply(Command::SetFloorMarking(Some(Box::default())))
            .unwrap();
        assert!(doc.model().floor_marking.is_none(), "default is not saved");
        let bad = FloorMarking {
            line_width: 0.0,
            ..FloorMarking::default()
        };
        assert!(
            doc.apply(Command::SetFloorMarking(Some(Box::new(bad))))
                .is_err()
        );
        // Everything is undoable.
        for _ in 0..9 {
            doc.undo();
        }
        assert_eq!(doc.model().run.movement, None);
    }
}
