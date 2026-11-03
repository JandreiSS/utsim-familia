//! Multitasking of a person (`docs/plano_linhas_especializadas.md`, section 2.2): the
//! `multitask` field of a transporter, read by the dispatcher of its team.

use crate::{Model, ObjectKind};

impl Model {
    /// Cases the team member `name` may hold at once: its `multitask.max_open`, or 1.
    pub(crate) fn open_limit(&self, name: &str) -> usize {
        self.objects
            .iter()
            .find(|o| o.name == name)
            .and_then(|o| match &o.kind {
                ObjectKind::Transporter {
                    multitask: Some(m), ..
                } => Some(m.max_open.max(1)),
                _ => None,
            })
            .unwrap_or(1)
    }
}
