//! A tabela de dados de uma aplicação: colunas com nome e células de texto, como a pessoa
//! digita. Os números aceitam vírgula ou ponto decimal.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Origem;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tabela {
    #[serde(default)]
    pub colunas: Vec<String>,
    #[serde(default)]
    pub linhas: Vec<Vec<String>>,
}

/// Lê um número digitado: `1,5`, `1.5`, ` 2 `. `None` quando vazio ou não é número.
pub fn numero(texto: &str) -> Option<f64> {
    utsim_ferramentas::parse_number(texto)
}

/// O SHA-256 do conteúdo, em hexadecimal.
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Tabela {
    pub fn nova(colunas: &[&str]) -> Tabela {
        Tabela {
            colunas: colunas.iter().map(|c| c.to_string()).collect(),
            linhas: Vec::new(),
        }
    }

    /// Uma tabela a partir de linhas de texto; sem cabeçalho, as colunas viram
    /// `Coluna 1`, `Coluna 2`...
    pub fn de_linhas(cabecalho: Option<Vec<String>>, linhas: Vec<Vec<String>>) -> Tabela {
        let largura = linhas
            .iter()
            .map(Vec::len)
            .chain(cabecalho.iter().map(Vec::len))
            .max()
            .unwrap_or(0);
        let mut colunas = cabecalho.unwrap_or_default();
        for i in colunas.len()..largura {
            colunas.push(format!("Coluna {}", i + 1));
        }
        for (i, c) in colunas.iter_mut().enumerate() {
            if c.trim().is_empty() {
                *c = format!("Coluna {}", i + 1);
            }
        }
        let linhas = linhas
            .into_iter()
            .filter(|l| l.iter().any(|c| !c.trim().is_empty()))
            .map(|mut l| {
                l.resize(largura, String::new());
                l
            })
            .collect();
        Tabela { colunas, linhas }
    }

    /// Lê um CSV (`;` com vírgula decimal, tabulação ou `,`; cabeçalho quando a primeira
    /// linha tem texto). Os números ficam escritos com vírgula decimal.
    pub fn de_csv(bytes: &[u8]) -> Tabela {
        let (texto, _) = utsim_quality::data::decode(bytes);
        let t = utsim_quality::data::parse(&texto);
        let linhas = t
            .rows
            .into_iter()
            .map(|l| {
                l.into_iter()
                    .map(|c| match utsim_quality::data::number(&c, t.decimal_comma) {
                        Some(v) => crate::num(v, 10),
                        None => c,
                    })
                    .collect()
            })
            .collect();
        Tabela::de_linhas(t.header, linhas)
    }

    /// Importa um CSV e devolve a tabela com a origem (o nome do arquivo e o hash).
    pub fn importar_csv(arquivo: &str, bytes: &[u8]) -> (Tabela, Origem) {
        (
            Tabela::de_csv(bytes),
            Origem::Importado {
                arquivo: arquivo.into(),
                hash: sha256(bytes),
                aba: String::new(),
            },
        )
    }

    /// Importa uma aba de um xlsx (`None`: a primeira), com os números escritos com vírgula
    /// decimal, e devolve a tabela com a origem (arquivo, aba e hash).
    pub fn importar_xlsx(
        arquivo: &str,
        bytes: &[u8],
        aba: Option<&str>,
    ) -> Result<(Tabela, Origem), String> {
        let t = utsim_quality::data::xlsx::read(bytes, aba)?;
        let nome_aba = match aba {
            Some(a) => a.to_string(),
            None => utsim_quality::data::xlsx::sheets(bytes)?
                .into_iter()
                .next()
                .unwrap_or_default(),
        };
        let linhas = t
            .rows
            .into_iter()
            .map(|l| {
                l.into_iter()
                    .map(|c| match utsim_quality::data::number(&c, false) {
                        Some(v) => crate::num(v, 10),
                        None => c,
                    })
                    .collect()
            })
            .collect();
        Ok((
            Tabela::de_linhas(t.header, linhas),
            Origem::Importado {
                arquivo: arquivo.into(),
                hash: sha256(bytes),
                aba: nome_aba,
            },
        ))
    }

    /// As abas de um xlsx, para a pessoa escolher.
    pub fn abas_xlsx(bytes: &[u8]) -> Result<Vec<String>, String> {
        utsim_quality::data::xlsx::sheets(bytes)
    }

    /// Importa pelo nome do arquivo: `.xlsx` (com a aba) ou texto (CSV, TSV, TXT).
    pub fn importar(
        arquivo: &str,
        bytes: &[u8],
        aba: Option<&str>,
    ) -> Result<(Tabela, Origem), String> {
        if arquivo.to_lowercase().ends_with(".xlsx") {
            Tabela::importar_xlsx(arquivo, bytes, aba)
        } else {
            Ok(Tabela::importar_csv(arquivo, bytes))
        }
    }

    /// A tabela como CSV com `;` e vírgula decimal, para abrir na planilha.
    pub fn csv(&self) -> String {
        let cel = |c: &str| {
            if c.contains([';', '"', '\n']) {
                format!("\"{}\"", c.replace('"', "\"\""))
            } else {
                c.to_string()
            }
        };
        let mut s = String::new();
        for l in std::iter::once(&self.colunas).chain(self.linhas.iter()) {
            s.push_str(&l.iter().map(|c| cel(c)).collect::<Vec<_>>().join(";"));
            s.push('\n');
        }
        s
    }

    pub fn celula(&self, linha: usize, coluna: usize) -> &str {
        self.linhas
            .get(linha)
            .and_then(|l| l.get(coluna))
            .map_or("", String::as_str)
    }

    /// Os números da coluna, pulando as células vazias ou de texto.
    pub fn numeros(&self, coluna: usize) -> Vec<f64> {
        self.linhas
            .iter()
            .filter_map(|l| l.get(coluna).and_then(|c| numero(c)))
            .collect()
    }

    /// As células da coluna que não são número nem vazias (para avisar a pessoa).
    pub fn invalidas(&self, coluna: usize) -> Vec<usize> {
        self.linhas
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.get(coluna)
                    .is_some_and(|c| !c.trim().is_empty() && numero(c).is_none())
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// O índice da coluna com este nome (sem diferença de maiúsculas e espaços).
    pub fn coluna(&self, nome: &str) -> Option<usize> {
        let n = nome.trim().to_lowercase();
        self.colunas
            .iter()
            .position(|c| c.trim().to_lowercase() == n)
    }

    pub fn adicionar_linha(&mut self) {
        self.linhas.push(vec![String::new(); self.colunas.len()]);
    }

    pub fn remover_linha(&mut self, i: usize) {
        if i < self.linhas.len() {
            self.linhas.remove(i);
        }
    }

    pub fn adicionar_coluna(&mut self, nome: &str) {
        self.colunas.push(nome.into());
        for l in &mut self.linhas {
            l.push(String::new());
        }
    }

    pub fn remover_coluna(&mut self, i: usize) {
        if i < self.colunas.len() {
            self.colunas.remove(i);
            for l in &mut self.linhas {
                if i < l.len() {
                    l.remove(i);
                }
            }
        }
    }

    /// Linhas sem nenhuma célula preenchida não contam.
    pub fn vazia(&self) -> bool {
        self.linhas
            .iter()
            .all(|l| l.iter().all(|c| c.trim().is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_with_semicolon_and_decimal_comma() {
        let csv = "Categoria;Quantidade\nRebarba;48\nRisco;22,5\n\nMancha;1.234,5\n";
        let (t, o) = Tabela::importar_csv("defeitos.csv", csv.as_bytes());
        assert_eq!(t.colunas, ["Categoria", "Quantidade"]);
        assert_eq!(t.numeros(1), [48.0, 22.5, 1234.5]);
        assert_eq!(t.celula(1, 1), "22,5");
        let Origem::Importado { hash, .. } = o else {
            panic!()
        };
        assert_eq!(hash.len(), 64);
        let back = Tabela::de_csv(t.csv().as_bytes());
        assert_eq!(back, t);
    }

    #[test]
    fn xlsx_by_extension_with_the_sheet_in_the_origin() {
        let mut wb = rust_xlsxwriter::Workbook::new();
        let w = wb.add_worksheet();
        w.set_name("Semana 45").unwrap();
        w.write_string(0, 0, "Diâmetro").unwrap();
        w.write_number(1, 0, 10.25).unwrap();
        w.write_number(2, 0, 9.5).unwrap();
        let b = wb.save_to_buffer().unwrap();
        let (t, o) = Tabela::importar("medidas.XLSX", &b, None).unwrap();
        assert_eq!(t.colunas, ["Diâmetro"]);
        assert_eq!(t.celula(0, 0), "10,25");
        assert_eq!(t.numeros(0), [10.25, 9.5]);
        assert!(o.descricao().contains("aba Semana 45"), "{}", o.descricao());
        assert!(Tabela::importar("x.xlsx", b"nada", None).is_err());
        assert!(
            Tabela::importar(
                "x.csv",
                b"a;b
1;2
",
                None
            )
            .is_ok()
        );
    }

    #[test]
    fn headerless_rows_get_column_names_and_bad_cells_are_listed() {
        let t = Tabela::de_csv(b"1.5\t2\n3\t\nx\t4\n");
        assert_eq!(t.colunas, ["Coluna 1", "Coluna 2"]);
        assert_eq!(t.numeros(0), [1.5, 3.0]);
        assert_eq!(t.invalidas(0), [2]);
    }
}
