//! The class: manifest, student list (CSV), the student-by-mission matrix and the most
//! common mistakes. Files and signatures live in `utsim-ensino-store`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::missao::Missao;
use crate::nota::{Resultado, Tentativa, fmt_num};
use crate::pacote::Registro;

/// Class format read by this version.
pub const FORMATO_TURMA: u32 = 1;

fn um() -> u32 {
    1
}

/// The class manifest (`turma.ron`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Turma {
    pub id: String,
    #[serde(default = "um")]
    pub formato: u32,
    pub nome: String,
    #[serde(default)]
    pub periodo: String,
    /// Why personal data is kept (LGPD), recorded when the class is created.
    #[serde(default)]
    pub finalidade: String,
    /// The teacher's public key (ed25519, hex).
    #[serde(default)]
    pub chave_professor: String,
    /// Published packages with their deadlines.
    #[serde(default)]
    pub pacotes: Vec<PacotePublicado>,
    #[serde(default)]
    pub registro: Registro,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PacotePublicado {
    /// File name inside `pacotes/` (`campanha.utse`), or `campanha` for the built-in one.
    pub arquivo: String,
    #[serde(default)]
    pub prazo: Option<String>,
}

/// A student of the class.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Aluno {
    pub nome: String,
    /// Academic record: the student's id in the class and the folder name.
    pub registro: String,
    /// Shown on the leaderboard, never the name.
    #[serde(default)]
    pub apelido: String,
}

/// Reads the student list: a header with `nome`, `registro` and optionally `apelido`,
/// separated by `;` or `,`.
pub fn importar_alunos(csv: &str) -> Result<Vec<Aluno>, Vec<String>> {
    let mut linhas = csv.lines().filter(|l| !l.trim().is_empty());
    let Some(cab) = linhas.next() else {
        return Err(vec!["arquivo vazio".into()]);
    };
    let sep = if cab.contains(';') { ';' } else { ',' };
    let campos: Vec<String> = cab
        .trim_start_matches('\u{feff}')
        .split(sep)
        .map(|c| c.trim().trim_matches('"').to_lowercase())
        .collect();
    let col = |n: &str| campos.iter().position(|c| c == n);
    let (Some(cn), Some(cr)) = (col("nome"), col("registro")) else {
        return Err(vec![
            "o cabeçalho precisa das colunas nome e registro".into(),
        ]);
    };
    let ca = col("apelido");
    let mut out: Vec<Aluno> = Vec::new();
    let mut erros = Vec::new();
    for (i, l) in linhas.enumerate() {
        let v: Vec<&str> = l.split(sep).map(|c| c.trim().trim_matches('"')).collect();
        let pega = |c: usize| v.get(c).map_or("", |s| s).to_string();
        let a = Aluno {
            nome: pega(cn),
            registro: pega(cr),
            apelido: ca.map(pega).unwrap_or_default(),
        };
        if a.nome.is_empty() || a.registro.is_empty() {
            erros.push(format!("linha {}: nome ou registro vazio", i + 2));
        } else if !registro_valido(&a.registro) {
            erros.push(format!(
                "linha {}: registro '{}' com caracteres não permitidos",
                i + 2,
                a.registro
            ));
        } else if out.iter().any(|x| x.registro == a.registro) {
            erros.push(format!(
                "linha {}: registro '{}' repetido",
                i + 2,
                a.registro
            ));
        } else {
            out.push(a);
        }
    }
    if erros.is_empty() {
        Ok(out)
    } else {
        Err(erros)
    }
}

/// A record names a folder: letters, digits, `-` and `_` only.
pub fn registro_valido(r: &str) -> bool {
    !r.is_empty()
        && r.len() <= 40
        && r.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The student list as CSV (`;`), the format [`importar_alunos`] reads.
pub fn alunos_csv(alunos: &[Aluno]) -> String {
    let mut s = String::from("nome;registro;apelido\n");
    for a in alunos {
        s.push_str(&format!("{};{};{}\n", a.nome, a.registro, a.apelido));
    }
    s
}

/// One cell of the matrix: a student in a mission.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Celula {
    pub nota: f64,
    pub estrelas: u8,
    pub tentativas: u32,
    pub dicas: u8,
    pub resolvida: bool,
}

impl Celula {
    /// The cell from the graded attempts of a mission, in order.
    pub fn de(resultados: &[(Tentativa, Resultado)], registro: Registro) -> Option<Celula> {
        let ultimo = resultados.last()?;
        let melhor = resultados
            .iter()
            .max_by(|a, b| a.1.nota.total_cmp(&b.1.nota))
            .expect("not empty");
        let conta = match registro {
            Registro::Melhor => melhor,
            Registro::Ultima => ultimo,
        };
        Some(Celula {
            nota: conta.1.nota,
            estrelas: conta.1.estrelas,
            tentativas: resultados.len() as u32,
            dicas: resultados.iter().map(|r| r.0.dicas).max().unwrap_or(0),
            resolvida: resultados.iter().any(|r| r.1.resolvida),
        })
    }
}

/// A row of the matrix.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LinhaMatriz {
    pub aluno: Aluno,
    pub celulas: Vec<Option<Celula>>,
    /// Problems found in the delivery (broken chain, bad signature).
    pub avisos: Vec<String>,
}

impl LinhaMatriz {
    /// Mean grade over every mission (missing ones count as zero).
    pub fn media(&self) -> f64 {
        if self.celulas.is_empty() {
            return 0.0;
        }
        self.celulas
            .iter()
            .map(|c| c.as_ref().map_or(0.0, |c| c.nota))
            .sum::<f64>()
            / self.celulas.len() as f64
    }
}

/// Student by mission.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Matriz {
    pub missoes: Vec<String>,
    pub titulos: Vec<String>,
    pub linhas: Vec<LinhaMatriz>,
}

impl Matriz {
    /// Header and rows (grades with the decimal comma), for CSV and spreadsheets.
    pub fn tabela(&self) -> (Vec<String>, Vec<Vec<String>>) {
        let mut cab = vec!["registro".to_string(), "nome".into(), "apelido".into()];
        cab.extend(self.missoes.iter().cloned());
        cab.push("media".into());
        cab.push("avisos".into());
        let rows = self
            .linhas
            .iter()
            .map(|l| {
                let mut r = vec![
                    l.aluno.registro.clone(),
                    l.aluno.nome.clone(),
                    l.aluno.apelido.clone(),
                ];
                r.extend(
                    l.celulas
                        .iter()
                        .map(|c| c.as_ref().map_or_else(String::new, |c| fmt_num(c.nota, 1))),
                );
                r.push(fmt_num(l.media(), 1));
                r.push(l.avisos.join(" | "));
                r
            })
            .collect();
        (cab, rows)
    }

    /// The matrix as CSV with `;` (opens in spreadsheets set to Portuguese).
    pub fn csv(&self) -> String {
        let (cab, rows) = self.tabela();
        let esc = |s: &str| {
            if s.contains(';') || s.contains('"') || s.contains('\n') {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_string()
            }
        };
        let mut out = String::new();
        out.push_str(&cab.iter().map(|c| esc(c)).collect::<Vec<_>>().join(";"));
        out.push('\n');
        for r in rows {
            out.push_str(&r.iter().map(|c| esc(c)).collect::<Vec<_>>().join(";"));
            out.push('\n');
        }
        out
    }
}

/// A frequent mistake in a mission.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ErroComum {
    pub texto: String,
    /// Fraction of the students considered (0 to 1).
    pub fracao: f64,
}

/// The most frequent decisions among students who did not solve the mission (their last
/// attempt), and the predictions most often wrong among everyone.
pub fn erros_comuns(missao: &Missao, por_aluno: &[Vec<(Tentativa, Resultado)>]) -> Vec<ErroComum> {
    let mut out = Vec::new();
    let nao: Vec<&(Tentativa, Resultado)> = por_aluno
        .iter()
        .filter(|v| !v.iter().any(|r| r.1.resolvida))
        .filter_map(|v| v.last())
        .collect();
    if !nao.is_empty() {
        for a in &missao.alavancas {
            let mut conta: BTreeMap<String, usize> = BTreeMap::new();
            for (t, _) in &nao {
                let v = t.decisoes.get(a.id()).copied().unwrap_or(a.inicial());
                *conta.entry(a.descrever(v)).or_default() += 1;
            }
            if let Some((valor, n)) = conta.into_iter().max_by_key(|(_, n)| *n) {
                let f = n as f64 / nao.len() as f64;
                out.push(ErroComum {
                    texto: format!(
                        "{} % dos que não resolveram deixaram \"{}\" em {}",
                        (f * 100.0).round(),
                        a.rotulo(),
                        valor
                    ),
                    fracao: f,
                });
            }
        }
    }
    let todos: Vec<&(Tentativa, Resultado)> = por_aluno.iter().filter_map(|v| v.first()).collect();
    if !todos.is_empty() {
        for (i, p) in missao.previsao.iter().enumerate() {
            let erradas = todos
                .iter()
                .filter(|(_, r)| r.previsoes.get(i).is_some_and(|x| !x.certa))
                .count();
            if erradas > 0 {
                let f = erradas as f64 / todos.len() as f64;
                out.push(ErroComum {
                    texto: format!(
                        "{} % erraram a previsão \"{}\" na primeira tentativa",
                        (f * 100.0).round(),
                        p.pergunta()
                    ),
                    fracao: f,
                });
            }
        }
    }
    out.sort_by(|a, b| b.fracao.total_cmp(&a.fracao));
    out
}
