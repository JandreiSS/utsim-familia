//! Ideas sent to a portfolio by the other apps of the family: the 8D of UTSim Qualidade
//! (QS13 of `docs/plano_qualidade_e_seguranca.md`) proposes a Lean Six Sigma project when
//! the problem needs one. The idea enters the bank of ideas like one typed in the
//! portfolio, with its origin (the app and the record), who sent it and when, and the
//! files attached (copied by content into the portfolio folder's `anexos/`).

use std::path::{Path, PathBuf};

use utsim_lss_core::portfolio::{self, Submission};
use utsim_lss_core::{Date, Id};

use crate::project::ProjectFolder;
use crate::user::Identity;

/// Why an idea was not sent.
#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    #[error("a pasta não é de uma carteira de projetos Lean Seis Sigma")]
    NotAPortfolio,
    #[error("dê um título à ideia")]
    NoTitle,
    #[error("diga de onde a ideia vem (app de origem)")]
    NoSource,
    #[error(transparent)]
    Store(#[from] crate::Error),
}

/// Sends an idea to the portfolio at `portfolio_root`, as `me`, with the files of
/// `attachments` (each copied once by content). Returns the id of the idea.
pub fn submit_idea(
    portfolio_root: &Path,
    me: &Identity,
    clock: &dyn crate::Clock,
    idea: &Submission,
    attachments: &[PathBuf],
) -> Result<Id, IntakeError> {
    if idea.title.trim().is_empty() {
        return Err(IntakeError::NoTitle);
    }
    if idea.source.trim().is_empty() {
        return Err(IntakeError::NoSource);
    }
    let mut pf = ProjectFolder::open(portfolio_root, me, None)?;
    if !pf.is_portfolio() {
        return Err(IntakeError::NotAPortfolio);
    }
    // Copy the files first: nothing is written to the journal if one cannot be read.
    let mut files = Vec::new();
    for a in attachments {
        let name = a
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        files.push((pf.add_attachment(a)?, name));
    }
    let id = clock.new_id();
    let today = Date::from_unix_ms(clock.now_ms(), clock.utc_offset_minutes());
    let mut edits = portfolio::submitted_idea(pf.project_id(), id, idea, me.person, today);
    for (file, name) in &files {
        edits.extend(portfolio::attach(id, clock.new_id(), file, name));
    }
    pf.commit(edits, clock)?;
    Ok(id)
}

/// The path of an attachment of a portfolio (or project) folder.
pub fn attachment_path(folder_root: &Path, file: &str) -> PathBuf {
    folder_root.join(crate::project::ATTACHMENTS).join(file)
}
