//! The A3: the sections of the page in reading order (left column, then right) and the
//! field of each team note. The content of each section comes from the other tools of the
//! app (in the Seis Sigma, `utsim_lss_core::tools::a3::compose`).

/// The sections of the page, in reading order (left column, then right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Background,
    Current,
    Goal,
    Analysis,
    Countermeasures,
    Plan,
    FollowUp,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Background,
        Section::Current,
        Section::Goal,
        Section::Analysis,
        Section::Countermeasures,
        Section::Plan,
        Section::FollowUp,
    ];

    /// Field of the A3 with the team's note for the section.
    pub fn note_field(self) -> &'static str {
        match self {
            Section::Background => "notas_contexto",
            Section::Current => "notas_condicao_atual",
            Section::Goal => "notas_objetivo",
            Section::Analysis => "notas_analise",
            Section::Countermeasures => "notas_contramedidas",
            Section::Plan => "notas_plano",
            Section::FollowUp => "notas_acompanhamento",
        }
    }
}
