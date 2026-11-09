//! Estudos: as ferramentas da qualidade e da segurança aplicadas a um assunto (um nó do
//! modelo, um posto ou um problema), com ou sem simulação (seção 5.1 de
//! `docs/plano_integracao_familia.md`).
//!
//! - [`Estudo`]: o assunto, o autor e as aplicações das ferramentas;
//! - [`Aplicacao`]: uma ferramenta com os dados e a origem deles (Simulado, Manual ou
//!   Importado, com o arquivo e o hash);
//! - [`tabela`]: a tabela de dados de uma aplicação e a importação de CSV;
//! - [`calculo`]: o resultado de cada ferramenta (indicadores, gráficos, tabelas, avisos),
//!   o mesmo na tela e no relatório;
//! - [`relatorio`]: o relatório de uma ferramenta e o consolidado do estudo, pelo gerador
//!   comum (`utsim_ferramentas::relatorios`), em PDF e HTML.
//!
//! O estudo é um registro da coleção [`ESTUDOS`] da pasta de unidade: a pasta o grava
//! inteiro a cada mudança e a última gravação vale.

pub mod calculo;
pub mod exemplo;
pub mod relatorio;
pub mod tabela;

use serde::{Deserialize, Serialize};

pub use tabela::Tabela;

/// Coleção dos estudos na pasta de unidade.
pub const ESTUDOS: &str = "estudos";

/// As ferramentas que um estudo aplica.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ferramenta {
    FolhaVerificacao,
    Estratificacao,
    Pareto,
    Histograma,
    Dispersao,
    CartaControle,
    Capacidade,
    Msa,
    Ishikawa,
    CincoPorques,
    Fmea,
    A3,
}

impl Ferramenta {
    pub const TODAS: [Ferramenta; 12] = [
        Ferramenta::FolhaVerificacao,
        Ferramenta::Estratificacao,
        Ferramenta::Pareto,
        Ferramenta::Histograma,
        Ferramenta::Dispersao,
        Ferramenta::CartaControle,
        Ferramenta::Capacidade,
        Ferramenta::Msa,
        Ferramenta::Ishikawa,
        Ferramenta::CincoPorques,
        Ferramenta::Fmea,
        Ferramenta::A3,
    ];

    /// As ferramentas que o app oferece: o Qualidade tem todas, o Segurança do Trabalho as
    /// de análise de causa e de dados que fazem sentido para acidentes e exposições.
    pub fn do_app(app: App) -> &'static [Ferramenta] {
        match app {
            App::Qualidade => &Ferramenta::TODAS,
            App::Seguranca => &[
                Ferramenta::FolhaVerificacao,
                Ferramenta::Estratificacao,
                Ferramenta::Pareto,
                Ferramenta::Histograma,
                Ferramenta::Dispersao,
                Ferramenta::Ishikawa,
                Ferramenta::CincoPorques,
                Ferramenta::Fmea,
                Ferramenta::A3,
            ],
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Ferramenta::FolhaVerificacao => "Folha de verificação",
            Ferramenta::Estratificacao => "Estratificação",
            Ferramenta::Pareto => "Pareto",
            Ferramenta::Histograma => "Histograma",
            Ferramenta::Dispersao => "Diagrama de dispersão",
            Ferramenta::CartaControle => "Carta de controle",
            Ferramenta::Capacidade => "Capacidade do processo",
            Ferramenta::Msa => "MSA (repetibilidade e reprodutibilidade)",
            Ferramenta::Ishikawa => "Ishikawa (causa e efeito)",
            Ferramenta::CincoPorques => "5 porquês",
            Ferramenta::Fmea => "FMEA",
            Ferramenta::A3 => "A3",
        }
    }

    /// Uma linha sobre o que a ferramenta faz, para a tela de escolha.
    pub fn ajuda(self) -> &'static str {
        match self {
            Ferramenta::FolhaVerificacao => {
                "Conta as ocorrências por categoria e por período, como a folha do chão de fábrica."
            }
            Ferramenta::Estratificacao => {
                "Separa os valores por estrato (turno, máquina, lote) e compara média e dispersão."
            }
            Ferramenta::Pareto => "Ordena as categorias e mostra as poucas que somam 80 %.",
            Ferramenta::Histograma => {
                "Mostra a distribuição dos valores contra os limites de especificação."
            }
            Ferramenta::Dispersao => "Relaciona duas variáveis, com a reta e a correlação.",
            Ferramenta::CartaControle => {
                "Separa causas comuns de causas especiais (I-MR, X̄-R, X̄-S, p, np, c, u)."
            }
            Ferramenta::Capacidade => "Cp, Cpk, Pp, Ppk e PPM contra a especificação.",
            Ferramenta::Msa => {
                "Avalia o sistema de medição pelo estudo de R&R com peças, operadores e repetições."
            }
            Ferramenta::Ishikawa => "Organiza as causas possíveis nos 6M.",
            Ferramenta::CincoPorques => "Desce do problema até a causa raiz pergunta a pergunta.",
            Ferramenta::Fmea => "Modos de falha, efeitos e causas, com NPR e prioridade de ação.",
            Ferramenta::A3 => "O problema e a solução em uma folha, do contexto ao acompanhamento.",
        }
    }

    /// Se a ferramenta trabalha sobre uma tabela de dados (as outras são de texto).
    pub fn usa_tabela(self) -> bool {
        !matches!(
            self,
            Ferramenta::Ishikawa | Ferramenta::CincoPorques | Ferramenta::A3
        )
    }

    /// Se a ferramenta pode puxar dados de uma simulação do modelo.
    pub fn simulavel(self) -> bool {
        matches!(
            self,
            Ferramenta::FolhaVerificacao
                | Ferramenta::Estratificacao
                | Ferramenta::Pareto
                | Ferramenta::Histograma
                | Ferramenta::Dispersao
                | Ferramenta::CartaControle
                | Ferramenta::Capacidade
        )
    }

    /// A tabela de partida, com as colunas que a ferramenta lê e um exemplo de linha vazia.
    pub fn tabela_inicial(self) -> Tabela {
        let cols: &[&str] = match self {
            Ferramenta::FolhaVerificacao => &["Categoria", "Seg", "Ter", "Qua", "Qui", "Sex"],
            Ferramenta::Estratificacao => &["Estrato", "Valor"],
            Ferramenta::Pareto => &["Categoria", "Quantidade", "Custo unitário"],
            Ferramenta::Histograma | Ferramenta::Capacidade => &["Valor"],
            Ferramenta::Dispersao => &["X", "Y"],
            Ferramenta::CartaControle => &["Subgrupo", "Valor"],
            Ferramenta::Msa => &["Peça", "Operador", "Valor"],
            Ferramenta::Fmea => &calculo::COLUNAS_FMEA,
            Ferramenta::Ishikawa | Ferramenta::CincoPorques | Ferramenta::A3 => &[],
        };
        Tabela::nova(cols)
    }
}

/// O app que criou o estudo (cada app mostra os seus e lê os dos outros).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum App {
    #[default]
    Qualidade,
    Seguranca,
}

impl App {
    pub fn nome(self) -> &'static str {
        match self {
            App::Qualidade => "UTSim Qualidade",
            App::Seguranca => "UTSim Segurança do Trabalho",
        }
    }
}

/// De onde vieram os dados de uma aplicação.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Origem {
    /// Digitados na tela.
    #[default]
    Manual,
    /// De um arquivo: o nome, o SHA-256 do conteúdo e a aba (planilhas).
    Importado {
        arquivo: String,
        hash: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        aba: String,
    },
    /// De uma simulação do modelo: o arquivo, o nó, o que foi lido, as replicações e quando.
    Simulado {
        modelo: String,
        #[serde(default)]
        objeto: String,
        #[serde(default)]
        leitura: String,
        #[serde(default)]
        replicacoes: u32,
        #[serde(default)]
        quando: String,
    },
}

impl Origem {
    /// O rótulo curto (`Simulado`, `Manual`, `Importado`).
    pub fn rotulo(&self) -> &'static str {
        match self {
            Origem::Manual => "Manual",
            Origem::Importado { .. } => "Importado",
            Origem::Simulado { .. } => "Simulado",
        }
    }

    /// A origem por extenso, para a capa e o rodapé da ferramenta no relatório.
    pub fn descricao(&self) -> String {
        match self {
            Origem::Manual => "Manual (dados digitados)".into(),
            Origem::Importado { arquivo, hash, aba } => {
                let h: String = hash.chars().take(12).collect();
                if aba.is_empty() {
                    format!("Importado de {arquivo} (SHA-256 {h}…)")
                } else {
                    format!("Importado de {arquivo}, aba {aba} (SHA-256 {h}…)")
                }
            }
            Origem::Simulado {
                modelo,
                objeto,
                leitura,
                replicacoes,
                quando,
            } => {
                let mut s = format!("Simulado no modelo {modelo}");
                if !objeto.is_empty() {
                    s.push_str(&format!(", nó {objeto}"));
                }
                if !leitura.is_empty() {
                    s.push_str(&format!(", {leitura}"));
                }
                if *replicacoes > 0 {
                    s.push_str(&format!(
                        ", {replicacoes} {}",
                        if *replicacoes == 1 {
                            "replicação"
                        } else {
                            "replicações"
                        }
                    ));
                }
                if !quando.is_empty() {
                    s.push_str(&format!(", em {quando}"));
                }
                s
            }
        }
    }
}

/// Do que trata o estudo. Tudo opcional: um estudo sem modelo nem posto é um problema solto.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Assunto {
    /// O problema em uma frase (o efeito do Ishikawa, o problema dos 5 porquês).
    #[serde(default)]
    pub problema: String,
    /// Id do posto na pasta de unidade.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub posto: String,
    /// Modelo (relativo à pasta de unidade ou absoluto) e nó.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub modelo: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub objeto: String,
}

impl Assunto {
    /// O assunto em uma linha (`nó torno de linha.ron`, `posto Prensa 3`, o problema).
    pub fn resumo(&self, nome_do_posto: Option<&str>) -> String {
        let mut partes = Vec::new();
        if !self.objeto.is_empty() {
            let arquivo = std::path::Path::new(&self.modelo)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.modelo.clone());
            partes.push(format!("nó {} de {arquivo}", self.objeto));
        }
        if !self.posto.is_empty() {
            partes.push(format!("posto {}", nome_do_posto.unwrap_or(&self.posto)));
        }
        if partes.is_empty() {
            if self.problema.trim().is_empty() {
                "sem assunto".into()
            } else {
                self.problema.trim().into()
            }
        } else {
            partes.join(", ")
        }
    }

    /// Se o estudo é deste nó.
    pub fn eh_do_no(&self, modelo: &str, objeto: &str) -> bool {
        !objeto.is_empty() && self.objeto == objeto && mesmo_modelo(&self.modelo, modelo)
    }
}

/// Compara dois caminhos de modelo pelo nome do arquivo quando um deles é relativo (o
/// mesmo modelo aberto pela pasta de unidade ou pelo caminho absoluto).
fn mesmo_modelo(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_lowercase();
    let (a, b) = (norm(a), norm(b));
    !a.is_empty()
        && !b.is_empty()
        && (a == b || a.ends_with(&format!("/{b}")) || b.ends_with(&format!("/{a}")))
}

/// Parâmetros de leitura da tabela (quais colunas, limites, tipo de carta).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parametros {
    /// Colunas lidas, por índice: categoria, valor, X e Y, peça e operador, conforme a
    /// ferramenta (ver [`calculo`]).
    #[serde(default)]
    pub colunas: Vec<usize>,
    #[serde(default)]
    pub lie: Option<f64>,
    #[serde(default)]
    pub lse: Option<f64>,
    #[serde(default)]
    pub alvo: Option<f64>,
    /// Tamanho do subgrupo (carta e capacidade). 1 = valores individuais.
    #[serde(default = "um")]
    pub subgrupo: usize,
    /// Carta: `imr`, `xbarr`, `xbars`, `p`, `np`, `c`, `u`. Vazia escolhe pelo subgrupo.
    #[serde(default)]
    pub carta: String,
    /// Regras de sinal da carta: `basicas`, `western`, `nelson`.
    #[serde(default)]
    pub regras: String,
    /// Tolerância da MSA (LSE − LIE) para o %Tolerância.
    #[serde(default)]
    pub tolerancia: Option<f64>,
    /// Pareto: categorias abaixo deste percentual vão para "Outros" (0 = nenhuma).
    #[serde(default)]
    pub outros_abaixo: f64,
    /// FMEA: a partir de que severidade a linha é crítica.
    #[serde(default = "nove")]
    pub severidade_critica: u8,
}

fn um() -> usize {
    1
}

fn nove() -> u8 {
    9
}

impl Default for Parametros {
    fn default() -> Self {
        Parametros {
            colunas: Vec::new(),
            lie: None,
            lse: None,
            alvo: None,
            subgrupo: 1,
            carta: String::new(),
            regras: String::new(),
            tolerancia: None,
            outros_abaixo: 0.0,
            severidade_critica: 9,
        }
    }
}

/// Uma categoria do Ishikawa com as causas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Categoria {
    pub nome: String,
    #[serde(default)]
    pub causas: Vec<String>,
}

/// Uma cadeia de porquês: cada resposta leva à pergunta seguinte, a última é a causa raiz.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cadeia {
    #[serde(default)]
    pub porques: Vec<String>,
    /// A ação sobre a causa raiz.
    #[serde(default)]
    pub acao: String,
}

/// Uma seção de texto do A3.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Secao {
    pub titulo: String,
    #[serde(default)]
    pub texto: String,
}

/// As seções do A3, na ordem da folha.
pub const SECOES_A3: [&str; 7] = [
    "Contexto",
    "Condição atual",
    "Objetivo",
    "Análise",
    "Contramedidas",
    "Plano",
    "Acompanhamento",
];

/// As categorias de partida do Ishikawa (6M).
pub const SEIS_M: [&str; 6] = [
    "Máquina",
    "Método",
    "Material",
    "Mão de obra",
    "Medição",
    "Meio ambiente",
];

/// Uma ferramenta aplicada no estudo.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aplicacao {
    pub id: String,
    pub ferramenta: Ferramenta,
    pub titulo: String,
    #[serde(default)]
    pub origem: Origem,
    /// Os dados das ferramentas de tabela (e da FMEA).
    #[serde(default)]
    pub tabela: Tabela,
    #[serde(default)]
    pub parametros: Parametros,
    /// O efeito do Ishikawa ou o problema dos 5 porquês (vazio usa o do estudo).
    #[serde(default)]
    pub efeito: String,
    #[serde(default)]
    pub categorias: Vec<Categoria>,
    #[serde(default)]
    pub cadeias: Vec<Cadeia>,
    #[serde(default)]
    pub secoes: Vec<Secao>,
    /// A leitura da equipe: o que o resultado mostra e o que fazer.
    #[serde(default)]
    pub conclusao: String,
}

impl Aplicacao {
    /// Uma aplicação nova, com a tabela, as categorias ou as seções de partida.
    pub fn nova(id: &str, ferramenta: Ferramenta) -> Aplicacao {
        let mut a = Aplicacao {
            id: id.into(),
            ferramenta,
            titulo: ferramenta.nome().into(),
            origem: Origem::Manual,
            tabela: ferramenta.tabela_inicial(),
            parametros: Parametros::default(),
            efeito: String::new(),
            categorias: Vec::new(),
            cadeias: Vec::new(),
            secoes: Vec::new(),
            conclusao: String::new(),
        };
        match ferramenta {
            Ferramenta::Ishikawa => {
                a.categorias = SEIS_M
                    .iter()
                    .map(|n| Categoria {
                        nome: (*n).into(),
                        causas: Vec::new(),
                    })
                    .collect();
            }
            Ferramenta::CincoPorques => a.cadeias = vec![Cadeia::default()],
            Ferramenta::A3 => {
                a.secoes = SECOES_A3
                    .iter()
                    .map(|t| Secao {
                        titulo: (*t).into(),
                        texto: String::new(),
                    })
                    .collect();
            }
            _ => {}
        }
        a
    }
}

/// Um estudo: o assunto e as ferramentas aplicadas a ele.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Estudo {
    pub id: String,
    pub titulo: String,
    #[serde(default)]
    pub app: App,
    #[serde(default)]
    pub assunto: Assunto,
    #[serde(default)]
    pub autor: String,
    /// Data de criação como é mostrada (`09/11/2026`).
    #[serde(default)]
    pub criado: String,
    #[serde(default)]
    pub aplicacoes: Vec<Aplicacao>,
    /// A conclusão do estudo, no relatório consolidado.
    #[serde(default)]
    pub conclusao: String,
}

impl Estudo {
    pub fn aplicacao(&self, id: &str) -> Option<&Aplicacao> {
        self.aplicacoes.iter().find(|a| a.id == id)
    }

    pub fn aplicacao_mut(&mut self, id: &str) -> Option<&mut Aplicacao> {
        self.aplicacoes.iter_mut().find(|a| a.id == id)
    }

    /// As origens dos dados das aplicações, sem repetir (para a capa).
    pub fn origens(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for a in &self.aplicacoes {
            if a.ferramenta.usa_tabela() {
                let d = a.origem.descricao();
                if !v.contains(&d) {
                    v.push(d);
                }
            }
        }
        v
    }

    /// Um id para a próxima aplicação (`a1`, `a2`...), sem repetir os que existem.
    pub fn novo_id_aplicacao(&self) -> String {
        (1..)
            .map(|i| format!("a{i}"))
            .find(|id| self.aplicacao(id).is_none())
            .expect("ids infinitos")
    }
}

/// Formata um número com vírgula decimal e no máximo `casas` casas, sem zeros à direita.
pub fn num(v: f64, casas: usize) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let s = format!("{v:.casas$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    let s = if s == "-0" { "0".into() } else { s };
    s.replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn study_round_trips_and_names_origins() {
        let mut e = Estudo {
            id: "e1".into(),
            titulo: "Furo fora de posição".into(),
            assunto: Assunto {
                modelo: "modelos/linha.ron".into(),
                objeto: "torno".into(),
                ..Assunto::default()
            },
            ..Estudo::default()
        };
        let id = e.novo_id_aplicacao();
        e.aplicacoes.push(Aplicacao::nova(&id, Ferramenta::Pareto));
        let mut b = Aplicacao::nova(&e.novo_id_aplicacao(), Ferramenta::Ishikawa);
        b.origem = Origem::Importado {
            arquivo: "x.csv".into(),
            hash: "abc".into(),
            aba: String::new(),
        };
        e.aplicacoes.push(b);
        assert_eq!(e.aplicacoes[1].id, "a2");
        assert_eq!(e.aplicacoes[1].categorias.len(), 6);
        let back: Estudo = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
        // The Ishikawa has no table, so its origin stays out of the cover.
        assert_eq!(e.origens(), vec!["Manual (dados digitados)".to_string()]);
        assert!(e.assunto.eh_do_no("C:/u/modelos/linha.ron", "torno"));
        assert!(!e.assunto.eh_do_no("C:/u/modelos/linha.ron", "prensa"));
        assert_eq!(e.assunto.resumo(None), "nó torno de linha.ron");
    }

    #[test]
    fn numbers_in_portuguese() {
        assert_eq!(num(1.5, 2), "1,5");
        assert_eq!(num(2.0, 3), "2");
        assert_eq!(num(-0.0001, 2), "0");
        assert_eq!(num(f64::NAN, 2), "—");
    }
}
