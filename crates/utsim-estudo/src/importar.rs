//! Dados que chegam de fora para uma ferramenta: os arquivos que outro app manda abrir
//! (`--utsim-abrir=<.csv|.xlsx>` com `--utsim-ferramenta=<chave>`, seção 2.2 de
//! `docs/plano_integracao_familia.md`). A tabela é ajustada ao que a ferramenta lê:
//!
//! - carta e capacidade aceitam subgrupos em formato largo (`subgrupo;x1;...;xn`), que
//!   viram uma linha por valor com o subgrupo de tamanho n;
//! - carta com uma coluna só ganha a coluna do subgrupo;
//! - capacidade e histograma leem a última coluna numérica;
//! - MSA acha peça, operador e medida pelo cabeçalho (com ou sem acento) e ignora o resto;
//! - estratificação lê o grupo (ou estrato) e a última coluna numérica.

use crate::{Aplicacao, Ferramenta, Origem, Tabela, num, tabela::numero};

/// A chave da ferramenta na linha de comando.
pub fn chave(f: Ferramenta) -> &'static str {
    match f {
        Ferramenta::FolhaVerificacao => "folha",
        Ferramenta::Estratificacao => "estratificacao",
        Ferramenta::Pareto => "pareto",
        Ferramenta::Histograma => "histograma",
        Ferramenta::Dispersao => "dispersao",
        Ferramenta::CartaControle => "carta",
        Ferramenta::Capacidade => "capacidade",
        Ferramenta::Msa => "msa",
        Ferramenta::Ishikawa => "ishikawa",
        Ferramenta::CincoPorques => "cinco_porques",
        Ferramenta::Fmea => "fmea",
        Ferramenta::A3 => "a3",
        Ferramenta::Qfd => "qfd",
        Ferramenta::Tolerancias => "tolerancias",
        Ferramenta::PokaYoke => "poka_yoke",
    }
}

/// A ferramenta de uma chave (sem diferença de maiúsculas e acentos).
pub fn de_chave(texto: &str) -> Option<Ferramenta> {
    let t = normalizar(texto).replace([' ', '-'], "_");
    Ferramenta::TODAS.iter().copied().find(|f| chave(*f) == t)
}

/// Minúsculas, sem acento e sem espaços nas pontas.
pub fn normalizar(texto: &str) -> String {
    texto
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

fn achar(t: &Tabela, nomes: &[&str]) -> Option<usize> {
    t.colunas
        .iter()
        .position(|c| nomes.contains(&normalizar(c).as_str()))
}

/// Coluna numérica: ao menos um número e nenhuma célula de texto.
fn numerica(t: &Tabela, c: usize) -> bool {
    !t.numeros(c).is_empty() && t.invalidas(c).is_empty()
}

fn ultima_numerica(t: &Tabela) -> Option<usize> {
    (0..t.colunas.len()).rev().find(|&c| numerica(t, c))
}

/// As colunas `x1..xn` do formato largo (ao menos duas), na ordem da tabela.
fn colunas_largas(t: &Tabela) -> Vec<usize> {
    let xs: Vec<usize> = (0..t.colunas.len())
        .filter(|&c| {
            let n = normalizar(&t.colunas[c]);
            n.len() > 1 && n.starts_with('x') && n[1..].chars().all(|d| d.is_ascii_digit())
        })
        .collect();
    if xs.len() >= 2 { xs } else { Vec::new() }
}

/// O formato largo em longo: (subgrupo, valor) por célula, e o tamanho do subgrupo.
fn alongar(t: &Tabela, xs: &[usize]) -> (Tabela, usize) {
    let rotulo = achar(t, &["subgrupo", "amostra", "lote"]).filter(|c| !xs.contains(c));
    let mut out = Tabela::nova(&["Subgrupo", "Valor"]);
    for (i, l) in t.linhas.iter().enumerate() {
        let nome = rotulo
            .and_then(|c| l.get(c))
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| (i + 1).to_string());
        for &c in xs {
            if let Some(v) = l.get(c).and_then(|s| numero(s)) {
                out.linhas.push(vec![nome.clone(), num(v, 10)]);
            }
        }
    }
    (out, xs.len())
}

/// Põe a tabela e a origem na aplicação e ajusta as colunas lidas e o subgrupo ao que a
/// ferramenta espera. O subgrupo já escolhido (`a.parametros.subgrupo`) vale quando a
/// tabela não diz outro.
pub fn aplicar(a: &mut Aplicacao, t: Tabela, origem: Origem) {
    a.origem = origem;
    let p = &mut a.parametros;
    match a.ferramenta {
        Ferramenta::CartaControle | Ferramenta::Capacidade => {
            let xs = colunas_largas(&t);
            let (t, n) = if xs.is_empty() {
                (t, p.subgrupo.max(1))
            } else {
                alongar(&t, &xs)
            };
            p.subgrupo = n;
            let valor = ultima_numerica(&t).unwrap_or(0);
            if a.ferramenta == Ferramenta::Capacidade {
                p.colunas = vec![valor];
                a.tabela = t;
            } else if t.colunas.len() == 1 {
                // Uma coluna só: o subgrupo é a posição do valor.
                let mut c = Tabela::nova(&["Subgrupo", t.colunas[0].as_str()]);
                c.linhas = t
                    .linhas
                    .iter()
                    .enumerate()
                    .map(|(i, l)| vec![(i / n + 1).to_string(), l[0].clone()])
                    .collect();
                p.colunas = vec![0, 1];
                a.tabela = c;
            } else {
                let rotulo = achar(&t, &["subgrupo", "amostra", "lote", "peca", "hora"])
                    .filter(|&c| c != valor)
                    .unwrap_or(if valor == 0 { 1 } else { 0 });
                p.colunas = vec![rotulo, valor];
                a.tabela = t;
            }
        }
        Ferramenta::Histograma => {
            p.colunas = vec![ultima_numerica(&t).unwrap_or(0)];
            a.tabela = t;
        }
        Ferramenta::Msa => {
            let peca = achar(&t, &["peca", "pecas", "item", "parte"]).unwrap_or(0);
            let op = achar(&t, &["operador", "avaliador", "operadora"]).unwrap_or(1);
            let medida = achar(&t, &["medida", "medicao", "valor", "leitura"])
                .or_else(|| ultima_numerica(&t))
                .unwrap_or(2);
            p.colunas = vec![peca, op, medida];
            a.tabela = t;
        }
        Ferramenta::Estratificacao => {
            let grupo = achar(&t, &["grupo", "estrato", "turno", "maquina"]).unwrap_or(0);
            let valor = ultima_numerica(&t)
                .filter(|&c| c != grupo)
                .unwrap_or(if grupo == 0 { 1 } else { 0 });
            p.colunas = vec![grupo, valor];
            a.tabela = t;
        }
        _ => a.tabela = t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculo::calcular;

    fn importar(f: Ferramenta, csv: &str) -> Aplicacao {
        let (t, o) = Tabela::importar_csv("dados.csv", csv.as_bytes());
        let mut a = Aplicacao::nova("a1", f);
        aplicar(&mut a, t, o);
        a
    }

    #[test]
    fn keys_round_trip_and_accept_accents() {
        for f in Ferramenta::TODAS {
            assert_eq!(de_chave(chave(f)), Some(f));
        }
        assert_eq!(de_chave("Estratificação"), Some(Ferramenta::Estratificacao));
        assert_eq!(de_chave("CARTA"), Some(Ferramenta::CartaControle));
        assert_eq!(de_chave("nada"), None);
    }

    #[test]
    fn wide_subgroups_for_chart_and_capability() {
        let csv = "subgrupo;x1;x2;x3;x4;x5\n\
                   1;25,001;24,998;25,003;25,000;24,999\n\
                   2;25,002;25,001;24,997;25,000;25,004\n\
                   3;24,996;25,003;25,001;24,999;25,000\n\
                   4;25,000;25,002;24,998;25,001;25,003\n";
        let a = importar(Ferramenta::CartaControle, csv);
        assert_eq!(a.parametros.subgrupo, 5);
        assert_eq!(a.tabela.linhas.len(), 20);
        assert_eq!(a.tabela.celula(6, 0), "2");
        assert_eq!(a.parametros.colunas, [0, 1]);
        assert!(!calcular(&a, "").vazio());

        let mut c = importar(Ferramenta::Capacidade, csv);
        assert_eq!(c.parametros.subgrupo, 5);
        assert_eq!(c.tabela.numeros(c.parametros.colunas[0]).len(), 20);
        c.parametros.lie = Some(24.985);
        c.parametros.lse = Some(25.015);
        let r = calcular(&c, "");
        assert!(r.avisos.is_empty(), "{:?}", r.avisos);
    }

    #[test]
    fn single_column_chart_gets_subgroup_labels() {
        let mut a = Aplicacao::nova("a1", Ferramenta::CartaControle);
        a.parametros.subgrupo = 2;
        let (t, o) = Tabela::importar_csv("v.csv", b"medida\n1\n2\n3\n4\n");
        aplicar(&mut a, t, o);
        assert_eq!(a.tabela.colunas, ["Subgrupo", "medida"]);
        assert_eq!(a.tabela.celula(3, 0), "2");
        assert_eq!(a.parametros.subgrupo, 2);
    }

    #[test]
    fn msa_columns_by_header_without_accents() {
        let mut csv = String::from("peca;operador;repeticao;medida\n");
        for p in 1..=5 {
            for (o, d) in [("A", 0.0), ("B", 0.01)] {
                for r in 1..=2 {
                    let v = 10.0 + p as f64 * 0.1 + d + r as f64 * 0.002;
                    csv.push_str(&format!("{p};{o};{r};{}\n", num(v, 3)));
                }
            }
        }
        let a = importar(Ferramenta::Msa, &csv);
        assert_eq!(a.parametros.colunas, [0, 1, 3]);
        assert!(!calcular(&a, "").vazio());
    }

    #[test]
    fn last_numeric_column_for_histogram_and_groups_for_stratification() {
        let csv = "grupo;medida\nA;1,5\nA;1,7\nB;2,1\nB;2,4\n";
        let h = importar(Ferramenta::Histograma, csv);
        assert_eq!(h.parametros.colunas, [1]);
        let c = importar(Ferramenta::Capacidade, csv);
        assert_eq!(c.parametros.colunas, [1]);
        let e = importar(Ferramenta::Estratificacao, csv);
        assert_eq!(e.parametros.colunas, [0, 1]);
        assert!(!calcular(&e, "").vazio());
    }
}
