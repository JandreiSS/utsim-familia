//! Lessons learned and the knowledge base (`docs/plano_lean_seis_sigma.md`, 1.9): what
//! worked, what did not, what to repeat, with tags; a plain text search over the lessons of
//! every project of the portfolio (accents and case ignored, every word must appear), and
//! "similar projects" by process and tags.

use crate::domain::of_project;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};

pub mod fields {
    pub const PROJECT: &str = "projeto";
    pub const TITLE: &str = "titulo";
    pub const WORKED: &str = "funcionou";
    pub const DID_NOT_WORK: &str = "nao_funcionou";
    pub const REPEAT: &str = "repetir";
    /// Comma-separated: process, area, kind of problem, root cause, solution, tools.
    pub const TAGS: &str = "etiquetas";
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lesson {
    pub id: Id,
    pub title: String,
    pub worked: String,
    pub did_not_work: String,
    pub repeat: String,
    pub tags: Vec<String>,
}

impl Lesson {
    pub fn read(id: Id, e: &Entity) -> Self {
        use fields as f;
        Self {
            id,
            title: e.text(f::TITLE),
            worked: e.text(f::WORKED),
            did_not_work: e.text(f::DID_NOT_WORK),
            repeat: e.text(f::REPEAT),
            tags: parse_tags(&e.text(f::TAGS)),
        }
    }
}

pub fn parse_tags(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .split([',', ';'])
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    v.dedup();
    v
}

pub fn lessons(state: &State, project: Id) -> Vec<Lesson> {
    of_project(state, Kind::Lesson, project, Lesson::read)
}

pub fn new_lesson(project: Id, id: Id, title: &str) -> Vec<Edit> {
    let r = EntityRef::new(Kind::Lesson, id);
    vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::TITLE, title.trim()),
    ]
}

/// A lesson in the knowledge base, with the project it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    pub project: Id,
    pub project_title: String,
    pub process: String,
    pub area: String,
    pub lesson: Lesson,
}

/// The documents of one project (its lessons with its title, process and area).
pub fn docs_of(state: &State, project: Id) -> Vec<Doc> {
    let p = state
        .entity(project)
        .map(|e| crate::domain::Project::read(project, e));
    let (title, process, area) = p.map(|p| (p.title, p.process, p.area)).unwrap_or_default();
    lessons(state, project)
        .into_iter()
        .map(|lesson| Doc {
            project,
            project_title: title.clone(),
            process: process.clone(),
            area: area.clone(),
            lesson,
        })
        .collect()
}

/// Lower case without accents, for matching.
pub fn normalize(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

fn words(q: &str) -> Vec<String> {
    normalize(q)
        .split(|c: char| !c.is_alphanumeric() && c != '%')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// A search hit: index into the documents and its score.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub score: u32,
}

/// Documents containing every word of the query (in any field), best first. Score: each
/// occurrence counts 1, in the title 3, in a tag or the process 5. An empty query lists
/// nothing.
pub fn search(docs: &[Doc], query: &str) -> Vec<Hit> {
    let ws = words(query);
    if ws.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<Hit> = docs
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let title = normalize(&d.lesson.title);
            let strong = normalize(&format!(
                "{} {} {}",
                d.lesson.tags.join(" "),
                d.process,
                d.area
            ));
            let body = normalize(&format!(
                "{} {} {} {}",
                d.lesson.worked, d.lesson.did_not_work, d.lesson.repeat, d.project_title
            ));
            let mut score = 0;
            for w in &ws {
                let n = title.matches(w.as_str()).count() as u32 * 3
                    + strong.matches(w.as_str()).count() as u32 * 5
                    + body.matches(w.as_str()).count() as u32;
                if n == 0 {
                    return None;
                }
                score += n;
            }
            Some(Hit { index: i, score })
        })
        .collect();
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    hits
}

/// Projects that look like this one: same process, or tags in common. `(project, shared
/// tags + 3 when the process matches)`, best first; the project itself left out.
pub fn similar(docs: &[Doc], project: Id, process: &str, tags: &[String]) -> Vec<(Id, u32)> {
    let p = normalize(process.trim());
    let mine: Vec<String> = tags.iter().map(|t| normalize(t)).collect();
    let mut by: std::collections::BTreeMap<Id, u32> = Default::default();
    for d in docs.iter().filter(|d| d.project != project) {
        let mut s = 0;
        if !p.is_empty() && normalize(d.process.trim()) == p {
            s += 3;
        }
        s += d
            .lesson
            .tags
            .iter()
            .filter(|t| mine.contains(&normalize(t)))
            .count() as u32;
        if s > 0 {
            let e = by.entry(d.project).or_default();
            *e = (*e).max(s);
        }
    }
    let mut v: Vec<(Id, u32)> = by.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn doc(p: u8, title: &str, worked: &str, tags: &str, process: &str) -> Doc {
        Doc {
            project: Id::v7(u64::from(p), [p; 10]),
            project_title: format!("Projeto {p}"),
            process: process.into(),
            area: String::new(),
            lesson: Lesson {
                id: Id::v7(100 + u64::from(p), [p; 10]),
                title: title.into(),
                worked: worked.into(),
                did_not_work: String::new(),
                repeat: String::new(),
                tags: parse_tags(tags),
            },
        }
    }

    #[test]
    fn search_ignores_accents_and_needs_every_word() {
        let docs = vec![
            doc(
                1,
                "Setup rápido na prensa",
                "SMED com a equipe do turno",
                "smed, prensa",
                "Estamparia",
            ),
            doc(
                2,
                "Calibração antes da coleta",
                "MSA evitou retrabalho",
                "msa",
                "Usinagem",
            ),
            doc(
                3,
                "Prensa parada",
                "manutenção autônoma",
                "tpm",
                "Estamparia",
            ),
        ];
        let h = search(&docs, "PRENSA");
        assert_eq!(h.iter().map(|x| x.index).collect::<Vec<_>>(), [0, 2]);
        assert!(h[0].score > h[1].score, "tag beats title only");
        assert_eq!(search(&docs, "calibracao msa")[0].index, 1);
        assert!(search(&docs, "prensa msa").is_empty());
        assert!(search(&docs, "  ").is_empty());
        assert_eq!(search(&docs, "autonoma").len(), 1);
        let sim = similar(&docs, docs[0].project, "estamparia", &["SMED".into()]);
        assert_eq!(sim, vec![(docs[2].project, 3)]);
        assert_eq!(parse_tags(" a, b;; a "), vec!["a", "b", "a"]);
    }

    proptest! {
        #[test]
        fn a_word_of_the_title_always_finds_the_lesson(t in "[a-záéíóúãõç]{3,10}( [a-z]{3,8}){0,3}") {
            let docs = vec![doc(1, &t, "", "", "")];
            let w = t.split(' ').next().unwrap().to_string();
            prop_assert_eq!(search(&docs, &w).len(), 1);
            prop_assert_eq!(normalize(&normalize(&t)), normalize(&t));
        }
    }
}
