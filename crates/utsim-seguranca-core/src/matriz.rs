//! Risk matrix: probability by severity, with levels, colours and decision rules. The
//! default is the project's own 5x5 (decision S2), written by the project. A company that
//! uses another matrix imports it by CSV and it replaces the default in the whole unit.
//!
//! Every matrix accepted is **monotone**: more probability or more severity never lowers
//! the level.

use serde::{Deserialize, Serialize};

/// A step of the probability or the severity scale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Degrau {
    pub nome: String,
    /// Example criterion, editable.
    #[serde(default)]
    pub criterio: String,
}

/// A risk level, from the lowest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nivel {
    pub nome: String,
    /// Colour `#rrggbb`.
    pub cor: String,
    /// What the company does at this level.
    pub decisao: String,
}

/// The matrix: `celulas[p][s]` is the index of the level for probability `p` and
/// severity `s` (both from 0, lowest first).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matriz {
    pub nome: String,
    pub probabilidade: Vec<Degrau>,
    pub severidade: Vec<Degrau>,
    pub niveis: Vec<Nivel>,
    pub celulas: Vec<Vec<usize>>,
}

fn degraus(itens: &[(&str, &str)]) -> Vec<Degrau> {
    itens
        .iter()
        .map(|(n, c)| Degrau {
            nome: n.to_string(),
            criterio: c.to_string(),
        })
        .collect()
}

impl Default for Matriz {
    /// The project's own 5x5 matrix. The level comes from the product of the two steps
    /// (1 to 25): up to 4 low, 5 to 9 medium, 10 to 16 high, 17 or more critical.
    fn default() -> Self {
        let probabilidade = degraus(&[
            ("Rara", "não se espera que aconteça na vida útil do posto"),
            ("Improvável", "pode acontecer, mas não há registro no posto"),
            ("Possível", "já aconteceu em postos parecidos"),
            ("Provável", "acontece algumas vezes por ano no posto"),
            (
                "Quase certa",
                "acontece com frequência, a exposição é diária",
            ),
        ]);
        let severidade = degraus(&[
            ("Leve", "desconforto ou primeiros socorros, sem afastamento"),
            ("Menor", "lesão que pede atendimento, sem afastamento"),
            ("Moderada", "lesão com afastamento curto"),
            ("Grave", "lesão com afastamento longo ou sequela"),
            ("Gravíssima", "incapacidade permanente ou morte"),
        ]);
        let niveis = vec![
            Nivel {
                nome: "Baixo".into(),
                cor: "#4caf50".into(),
                decisao: "aceitável, manter os controles e acompanhar".into(),
            },
            Nivel {
                nome: "Médio".into(),
                cor: "#ffc107".into(),
                decisao: "ação planejada, com prazo e responsável".into(),
            },
            Nivel {
                nome: "Alto".into(),
                cor: "#ff7043".into(),
                decisao: "ação prioritária, controles provisórios até concluir".into(),
            },
            Nivel {
                nome: "Crítico".into(),
                cor: "#c62828".into(),
                decisao: "ação imediata, a atividade não segue sem controle".into(),
            },
        ];
        let celulas = (1..=5)
            .map(|p| {
                (1..=5)
                    .map(|s| match p * s {
                        0..=4 => 0,
                        5..=9 => 1,
                        10..=16 => 2,
                        _ => 3,
                    })
                    .collect()
            })
            .collect();
        Matriz {
            nome: "Matriz própria do UTSim 5x5".into(),
            probabilidade,
            severidade,
            niveis,
            celulas,
        }
    }
}

impl Matriz {
    /// Level index of a probability and a severity (both from 0). Out of range is clamped.
    pub fn nivel(&self, p: usize, s: usize) -> usize {
        let p = p.min(self.probabilidade.len().saturating_sub(1));
        let s = s.min(self.severidade.len().saturating_sub(1));
        self.celulas
            .get(p)
            .and_then(|l| l.get(s))
            .copied()
            .unwrap_or(0)
    }

    pub fn nivel_de(&self, p: usize, s: usize) -> &Nivel {
        &self.niveis[self.nivel(p, s).min(self.niveis.len() - 1)]
    }

    /// Problems of the matrix: sizes from 3x3 to 5x5, every cell a valid level,
    /// monotone in both directions.
    pub fn problemas(&self) -> Vec<String> {
        let mut out = Vec::new();
        let (np, ns) = (self.probabilidade.len(), self.severidade.len());
        if !(3..=5).contains(&np) || !(3..=5).contains(&ns) {
            out.push(format!("a matriz deve ter de 3x3 a 5x5 (tem {np}x{ns})"));
        }
        if self.niveis.len() < 2 {
            out.push("a matriz precisa de ao menos 2 níveis de risco".into());
        }
        if self.celulas.len() != np || self.celulas.iter().any(|l| l.len() != ns) {
            out.push("as células não batem com as escalas".into());
            return out;
        }
        for p in 0..np {
            for s in 0..ns {
                let n = self.celulas[p][s];
                if n >= self.niveis.len() {
                    out.push(format!(
                        "célula ({}, {}) com nível inexistente",
                        p + 1,
                        s + 1
                    ));
                }
                if p + 1 < np && self.celulas[p + 1][s] < n {
                    out.push(format!(
                        "mais probabilidade baixa o nível em ({}, {})",
                        p + 2,
                        s + 1
                    ));
                }
                if s + 1 < ns && self.celulas[p][s + 1] < n {
                    out.push(format!(
                        "mais severidade baixa o nível em ({}, {})",
                        p + 1,
                        s + 2
                    ));
                }
            }
        }
        out
    }

    /// The matrix as CSV, in the format [`Matriz::de_csv`] reads.
    pub fn para_csv(&self) -> String {
        let mut s = String::from("niveis");
        for n in &self.niveis {
            s.push_str(&format!(";{}", n.nome));
        }
        s.push_str("\nprobabilidade\\severidade");
        for d in &self.severidade {
            s.push_str(&format!(";{}", d.nome));
        }
        s.push('\n');
        for (p, d) in self.probabilidade.iter().enumerate() {
            s.push_str(&d.nome);
            for k in &self.celulas[p] {
                s.push_str(&format!(";{}", self.niveis[*k].nome));
            }
            s.push('\n');
        }
        s
    }

    /// Reads a company matrix from CSV (`;` or `,`):
    ///
    /// ```text
    /// niveis;Baixo;Médio;Alto              levels, lowest first
    /// probabilidade\severidade;S1;S2;S3    severity steps, lowest first
    /// P1;Baixo;Baixo;Médio                 one line per probability step, lowest first
    /// P2;Baixo;Médio;Alto
    /// P3;Médio;Alto;Alto
    /// ```
    ///
    /// The colours and decisions of levels with the same name as the current matrix are
    /// kept; new levels get a colour by position. A matrix that is not monotone is refused.
    pub fn de_csv(texto: &str, atual: &Matriz) -> Result<Matriz, String> {
        let texto = texto.trim_start_matches('\u{feff}');
        let sep = if texto.lines().next().is_some_and(|l| l.contains(';')) {
            ';'
        } else {
            ','
        };
        let linhas: Vec<Vec<String>> = texto
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| l.split(sep).map(|c| c.trim().to_string()).collect())
            .collect();
        if linhas.len() < 3 {
            return Err("o CSV precisa da linha dos níveis, da linha das severidades e das linhas de probabilidade".into());
        }
        if !linhas[0][0].eq_ignore_ascii_case("niveis")
            && !linhas[0][0].eq_ignore_ascii_case("níveis")
        {
            return Err("a primeira linha deve começar com 'niveis'".into());
        }
        let nomes: Vec<String> = linhas[0][1..]
            .iter()
            .filter(|c| !c.is_empty())
            .cloned()
            .collect();
        let paleta = [
            "#4caf50", "#cddc39", "#ffc107", "#ff7043", "#c62828", "#6a1b9a",
        ];
        let niveis: Vec<Nivel> = nomes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                atual
                    .niveis
                    .iter()
                    .find(|x| &x.nome == n)
                    .cloned()
                    .unwrap_or(Nivel {
                        nome: n.clone(),
                        cor: paleta[((i * (paleta.len() - 1)) / (nomes.len().max(2) - 1))
                            .min(paleta.len() - 1)]
                        .to_string(),
                        decisao: String::new(),
                    })
            })
            .collect();
        let severidade: Vec<Degrau> = linhas[1][1..]
            .iter()
            .map(|n| Degrau {
                nome: n.clone(),
                criterio: String::new(),
            })
            .collect();
        let mut probabilidade = Vec::new();
        let mut celulas = Vec::new();
        for l in &linhas[2..] {
            probabilidade.push(Degrau {
                nome: l[0].clone(),
                criterio: String::new(),
            });
            let mut linha = Vec::new();
            for c in &l[1..] {
                let k = niveis
                    .iter()
                    .position(|n| &n.nome == c)
                    .ok_or_else(|| format!("nível '{c}' não está na linha dos níveis"))?;
                linha.push(k);
            }
            celulas.push(linha);
        }
        let m = Matriz {
            nome: "Matriz da empresa (importada)".into(),
            probabilidade,
            severidade,
            niveis,
            celulas,
        };
        let p = m.problemas();
        if p.is_empty() {
            Ok(m)
        } else {
            Err(p.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matrix_is_monotone_in_every_combination() {
        let m = Matriz::default();
        assert!(m.problemas().is_empty(), "{:?}", m.problemas());
        for p in 0..5 {
            for s in 0..5 {
                for p2 in p..5 {
                    for s2 in s..5 {
                        assert!(m.nivel(p2, s2) >= m.nivel(p, s));
                    }
                }
            }
        }
        assert_eq!(m.nivel_de(0, 0).nome, "Baixo");
        assert_eq!(m.nivel_de(4, 4).nome, "Crítico");
    }

    #[test]
    fn csv_round_trip_and_company_matrix_replaces() {
        let m = Matriz::default();
        let back = Matriz::de_csv(&m.para_csv(), &m).unwrap();
        assert_eq!(back.celulas, m.celulas);
        assert_eq!(back.niveis, m.niveis);
        let empresa = "niveis,Tolerável,Moderado,Intolerável\n\
                       p\\s,Leve,Séria,Fatal\n\
                       Baixa,Tolerável,Tolerável,Moderado\n\
                       Média,Tolerável,Moderado,Intolerável\n\
                       Alta,Moderado,Intolerável,Intolerável\n";
        let e = Matriz::de_csv(empresa, &m).unwrap();
        assert_eq!(e.probabilidade.len(), 3);
        assert_eq!(e.nivel_de(2, 2).nome, "Intolerável");
        let quebrada = empresa
            .replace("Alta,Moderado", "Alta,Tolerável")
            .replace("Média,Tolerável,Moderado", "Média,Moderado,Moderado");
        let err = Matriz::de_csv(&quebrada, &m).unwrap_err();
        assert!(err.contains("baixa o nível"), "{err}");
    }
}
