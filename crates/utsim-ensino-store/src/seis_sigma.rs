//! The way to UTSim Seis Sigma (`docs/plano_ensino_e_jogo.md`, 7.1): the student's data of
//! the Lean Six Sigma case become a DMAIC training project (`.lss` folder, marked as
//! training, out of every indicator of the program), each data set registered with the
//! origin `Simulado`, and the model of the factory copied in. The student analyses there
//! with the full tools and types the answers back in the game, which grades them from the
//! same data.

use std::path::{Path, PathBuf};

use utsim_ensino_core::pacote::MissaoCarregada;
use utsim_lss_core::dados::{DataOrigin, DatasetInput, register, shape};
use utsim_lss_core::domain::Approach;
use utsim_lss_core::{Edit, EntityRef, Kind};
use utsim_lss_store::user::Identity;
use utsim_lss_store::{Clock, ProjectFolder, SystemClock};

use crate::{Erro, Result};

/// Creates the training project of `aluno` for mission `m` under `pasta` and returns its
/// path. The project gets one data set per data set of the mission.
pub fn exportar(
    pasta: &Path,
    m: &MissaoCarregada,
    aluno: &str,
    nome_aluno: &str,
) -> Result<PathBuf> {
    let e = |x: utsim_lss_store::Error| Erro::Invalido(x.to_string());
    let clock = SystemClock;
    let me = Identity {
        person: clock.new_id(),
        name: if nome_aluno.trim().is_empty() {
            "Aluno".into()
        } else {
            nome_aluno.trim().into()
        },
        email: String::new(),
    };
    std::fs::create_dir_all(pasta)?;
    let titulo = format!("{} (treino)", m.missao.titulo);
    let nome: String = m
        .missao
        .id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let caminho = pasta.join(format!("{nome}_{aluno}.lss"));
    if caminho.exists() {
        return Err(Erro::Invalido(format!(
            "{} já existe: abra esse projeto no UTSim Seis Sigma ou escolha outra pasta",
            caminho.display()
        )));
    }
    let mut pf =
        ProjectFolder::create(&caminho, &titulo, Approach::Dmaic, &me, &clock, None).map_err(e)?;
    let projeto = pf.project_id();
    let mut edits = vec![Edit::new(
        EntityRef::new(Kind::Project, projeto),
        utsim_lss_core::programa::fields::TRAINING,
        true,
    )];
    for cj in &m.missao.dados {
        let c = cj.gerar(&m.missao.id, aluno);
        let csv = c.csv();
        let (file, hash) = pf.add_data(csv.as_bytes(), "csv").map_err(e)?;
        let (columns, rows) = shape(&csv);
        let input = DatasetInput {
            name: cj.nome.clone(),
            origin: DataOrigin::Simulated,
            file,
            hash,
            columns,
            rows,
            note: format!(
                "Gerado pelo UTSim Ensino para {aluno} (missão {}).",
                m.missao.id
            ),
            ..Default::default()
        };
        edits.extend(register(projeto, clock.new_id(), &input));
    }
    pf.commit(edits, &clock).map_err(e)?;
    let modelo = ron::ser::to_string_pretty(
        &m.modelo,
        ron::ser::PrettyConfig::default().struct_names(true),
    )
    .map_err(|x| Erro::Invalido(x.to_string()))?;
    pf.copy_model(Path::new("modelo_da_fabrica.ron"), &modelo)
        .map_err(e)?;
    Ok(caminho)
}

/// What [`registrar_competencias`] did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegistroCompetencias {
    /// `ferramenta (missão)` of each entry sent (an entry already there is not repeated).
    pub gravadas: Vec<String>,
    pub erros: Vec<String>,
}

/// Records in the company program (decision N2 of the Seis Sigma plan) the practice of the
/// solved missions: `praticou`, origin `ensino`, in the tools the package ties to their
/// concepts, by the student themself (`me`, the identity of UTSim Seis Sigma on this
/// computer). `so_missao` limits it to one mission (the one just solved).
pub fn registrar_competencias(
    programa: &Path,
    me: &Identity,
    clock: &dyn Clock,
    p: &utsim_ensino_core::pacote::PacoteCarregado,
    progresso: &utsim_ensino_core::progresso::Progresso,
    so_missao: Option<&str>,
) -> std::result::Result<RegistroCompetencias, String> {
    use utsim_lss_store::program::{record_teaching, teaching_check};
    teaching_check(programa, me).map_err(|e| e.to_string())?;
    let resolvidas: std::collections::BTreeSet<String> = p
        .missoes
        .iter()
        .map(|m| m.missao.id.clone())
        .filter(|id| progresso.resolvida(id))
        .filter(|id| so_missao.is_none_or(|s| s == id))
        .collect();
    let mut por = std::collections::BTreeMap::new();
    por.insert(me.person.to_string(), resolvidas);
    let mut r = RegistroCompetencias::default();
    for c in utsim_ensino_core::competencias::competencias(p, &por) {
        let nota = progresso.registro(&c.missao).map_or(0.0, |x| x.melhor_nota);
        let evidencia = format!(
            "{}, melhor nota {}",
            c.evidencia,
            utsim_ensino_core::nota::fmt_num(nota, 1)
        );
        match record_teaching(
            programa,
            me,
            clock,
            &c.ferramenta,
            utsim_lss_core::mentoria::Level::Practiced,
            &evidencia,
        ) {
            Ok(_) => r.gravadas.push(format!("{} ({})", c.ferramenta, c.missao)),
            Err(e) => r
                .erros
                .push(format!("{} ({}): {e}", c.ferramenta, c.missao)),
        }
    }
    Ok(r)
}

/// The identity of UTSim Seis Sigma on this computer, or why there is none.
pub fn identidade_local() -> std::result::Result<Identity, String> {
    Identity::local().ok_or_else(|| {
        "abra o UTSim Seis Sigma uma vez neste computador e preencha nome e e-mail".to_string()
    })
}
