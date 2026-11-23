//! The way to UTSim LSS (`docs/plano_ensino_e_jogo.md`, 7.1): the student's data of
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
            "{} já existe: abra esse projeto no UTSim LSS ou escolha outra pasta",
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
        let c = cj.gerar_com(&m.missao.id, aluno, &m.modelo);
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
/// concepts, by the student themself (`me`, the identity of UTSim LSS on this
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

/// What [`registrar_turma`] did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegistroTurma {
    /// Entries sent (one already in the program is not repeated).
    pub enviadas: usize,
    /// Students with at least one entry sent.
    pub alunos: usize,
    /// `nome (registro)` of the students with solved missions but no person of the
    /// program with their e-mail (or no e-mail in the class list).
    pub sem_pessoa: Vec<String>,
    /// Records of the students left out for a warning in the delivery.
    pub com_avisos: Vec<String>,
}

/// Records in the company program, as the teacher (`me`, an instructor or the MBB of the
/// program), the practice of the class graded in `matriz`: each solved mission becomes
/// `praticou`, origin `exercicio`, in the tools the package ties to its concepts, for the
/// person of the program with the student's e-mail. Names and records of the class never
/// go to the program. The exams do not record anything (they are graded apart).
pub fn registrar_turma(
    programa: &Path,
    me: &Identity,
    clock: &dyn Clock,
    p: &utsim_ensino_core::pacote::PacoteCarregado,
    matriz: &utsim_ensino_core::turma::Matriz,
) -> std::result::Result<RegistroTurma, String> {
    use std::collections::{BTreeMap, BTreeSet};
    use utsim_ensino_core::turma::email_chave;
    use utsim_lss_store::program::{ExerciseItem, members, record_exercises};
    let pessoas: BTreeMap<String, utsim_lss_core::Id> = members(programa, me)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(id, email)| (email, id))
        .collect();
    let (resolvidas, com_avisos) = utsim_ensino_core::competencias::resolvidas_da_turma(matriz);
    let mut r = RegistroTurma {
        com_avisos,
        ..Default::default()
    };
    let mut pessoa_de = BTreeMap::new();
    let mut por = BTreeMap::new();
    for l in matriz
        .linhas
        .iter()
        .filter(|l| resolvidas.contains_key(&l.aluno.registro))
    {
        match pessoas.get(&email_chave(&l.aluno.email)) {
            Some(id) if !l.aluno.email.trim().is_empty() => {
                pessoa_de.insert(l.aluno.registro.clone(), *id);
                por.insert(
                    l.aluno.registro.clone(),
                    resolvidas[&l.aluno.registro].clone(),
                );
            }
            _ => r
                .sem_pessoa
                .push(format!("{} ({})", l.aluno.nome, l.aluno.registro)),
        }
    }
    let nota = |registro: &str, missao: &str| {
        let i = matriz.missoes.iter().position(|m| m == missao)?;
        let l = matriz
            .linhas
            .iter()
            .find(|l| l.aluno.registro == registro)?;
        l.celulas.get(i)?.as_ref().map(|c| c.nota)
    };
    let mut itens = Vec::new();
    let mut com_item = BTreeSet::new();
    for c in utsim_ensino_core::competencias::competencias(p, &por) {
        let n = nota(&c.registro, &c.missao).unwrap_or(0.0);
        itens.push(ExerciseItem {
            person: pessoa_de[&c.registro],
            tool: c.ferramenta,
            evidence: format!(
                "{}, nota {}",
                c.evidencia,
                utsim_ensino_core::nota::fmt_num(n, 1)
            ),
        });
        com_item.insert(c.registro);
    }
    record_exercises(programa, me, clock, &itens).map_err(|e| e.to_string())?;
    r.enviadas = itens.len();
    r.alunos = com_item.len();
    Ok(r)
}

/// The identity of UTSim LSS on this computer, or why there is none.
pub fn identidade_local() -> std::result::Result<Identity, String> {
    Identity::local().ok_or_else(|| {
        "abra o UTSim LSS uma vez neste computador e preencha nome e e-mail".to_string()
    })
}
