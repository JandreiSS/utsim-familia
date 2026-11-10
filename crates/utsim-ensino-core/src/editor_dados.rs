//! Stage C of the mission editor (`docs/plano_ensino_e_jogo.md`, 6.2 and 6.3): the data
//! sets of a mission, made by the seeded generators with a planted effect, the questions
//! about them and the apps of the family where the student analyses them. The screen is
//! in `utsim-ensino-ui`; here are the defaults of each generator, the questions that fit
//! it, the checks and the sample the teacher sees before writing. A simulation set runs
//! the mission's own model, so its checks and samples need the model of the edition.

use std::collections::BTreeSet;
use std::mem::discriminant;

use utsim_core::Dist;

use crate::dados::{Conjunto, Gerador, Padrao, erros_da_simulacao, estacoes_do_modelo};
use crate::editor::{Edicao, id_valido};
use crate::missao::{APPS_DE_ANALISE, Previsao, Tolerancia};
use crate::questoes::{ConjuntoMissao, QuestaoDados};

/// The most rows a generated set may have (the student exports and analyses it).
pub const MAX_LINHAS: usize = 5000;

/// The most factors of a factorial (2^k runs per replicate).
pub const MAX_FATORES: usize = 6;

/// How many students the check of the questions tries ([`Edicao::avisos_dos_dados`]).
pub const ALUNOS_DE_TESTE: usize = 30;

/// The tools of UTSim Qualidade that open a table (`--utsim-ferramenta`, section 2.2 of
/// `docs/plano_integracao_familia.md`).
pub const FERRAMENTAS: &[&str] = &[
    "folha",
    "estratificacao",
    "pareto",
    "histograma",
    "dispersao",
    "carta",
    "capacidade",
    "msa",
];

/// The kinds of generator, in the order of the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoGerador {
    Tempos,
    Msa,
    MediaDeslocada,
    Serie,
    Fatorial,
    Pareado,
    Subgrupos,
    Simulacao,
}

impl TipoGerador {
    pub const TODOS: [TipoGerador; 8] = [
        TipoGerador::Tempos,
        TipoGerador::Msa,
        TipoGerador::MediaDeslocada,
        TipoGerador::Serie,
        TipoGerador::Fatorial,
        TipoGerador::Pareado,
        TipoGerador::Subgrupos,
        TipoGerador::Simulacao,
    ];

    pub fn de(g: &Gerador) -> TipoGerador {
        match g {
            Gerador::Tempos { .. } => TipoGerador::Tempos,
            Gerador::Msa { .. } => TipoGerador::Msa,
            Gerador::MediaDeslocada { .. } => TipoGerador::MediaDeslocada,
            Gerador::Serie { .. } => TipoGerador::Serie,
            Gerador::Fatorial { .. } => TipoGerador::Fatorial,
            Gerador::Pareado { .. } => TipoGerador::Pareado,
            Gerador::Subgrupos { .. } => TipoGerador::Subgrupos,
            Gerador::Simulacao { .. } => TipoGerador::Simulacao,
        }
    }

    /// A generator of this kind with values that make a good first exercise.
    pub fn padrao(self) -> Gerador {
        match self {
            TipoGerador::Tempos => Gerador::Tempos {
                dist: Dist::Normal {
                    mean: 60.0,
                    sd: 5.0,
                },
                n: 30,
            },
            TipoGerador::Msa => Gerador::Msa {
                pecas: 10,
                operadores: 3,
                repeticoes: 3,
                media: 25.0,
                sd_peca: 0.01,
                sd_operador: 0.004,
                sd_repetibilidade: 0.0025,
            },
            TipoGerador::MediaDeslocada => Gerador::MediaDeslocada {
                grupos: vec!["Turno A".into(), "Turno B".into(), "Turno C".into()],
                n_por_grupo: 30,
                media: 100.0,
                sd: 1.0,
                afetado: 0,
                deslocamento: 2.0,
            },
            TipoGerador::Serie => Gerador::Serie {
                n: 50,
                media: 100.0,
                sd: 1.0,
                padrao: Padrao::Deslocamento {
                    inicio: 30,
                    tamanho: 2.0,
                },
            },
            TipoGerador::Fatorial => Gerador::Fatorial {
                fatores: vec!["Temperatura".into(), "Pressão".into()],
                replicas: 3,
                media: 50.0,
                sd: 1.0,
                efeitos: vec![4.0, 1.0],
                interacao_ab: 2.0,
            },
            TipoGerador::Pareado => Gerador::Pareado {
                n: 10,
                media: 100.0,
                sd: 10.0,
                diferenca: -3.0,
                sd_diferenca: 2.0,
            },
            TipoGerador::Subgrupos => Gerador::Subgrupos {
                k: 25,
                tamanho: 5,
                media: 10.0,
                sd: 0.1,
                padrao: Some(Padrao::Deslocamento {
                    inicio: 20,
                    tamanho: 2.0,
                }),
            },
            // The stations come from the model ([`Edicao::novo_conjunto`]).
            TipoGerador::Simulacao => Gerador::Simulacao {
                estacoes: Vec::new(),
                afetado: 0,
                processo: Dist::Normal { mean: 5.0, sd: 0.5 },
                duracao: 960.0,
            },
        }
    }

    /// The questions that fit a set of this kind, with first values for their parameters.
    pub fn questoes(self) -> Vec<QuestaoDados> {
        match self {
            TipoGerador::Tempos => vec![
                QuestaoDados::MeiaLargura { confianca: 0.95 },
                QuestaoDados::Replicacoes { meia_largura: 1.0 },
                QuestaoDados::Ppk {
                    lie: None,
                    lse: None,
                },
            ],
            TipoGerador::Msa => vec![
                QuestaoDados::RrPercentual,
                QuestaoDados::RrVeredito,
                QuestaoDados::FonteDominante,
            ],
            TipoGerador::MediaDeslocada => vec![
                QuestaoDados::GrupoDeslocado,
                QuestaoDados::Ppk {
                    lie: None,
                    lse: None,
                },
            ],
            TipoGerador::Serie => vec![
                QuestaoDados::PrimeiroSinal { base: 20 },
                QuestaoDados::MeiaLargura { confianca: 0.95 },
                QuestaoDados::Ppk {
                    lie: None,
                    lse: None,
                },
            ],
            TipoGerador::Fatorial => vec![QuestaoDados::Efeito { fator: 0 }],
            TipoGerador::Pareado => vec![
                QuestaoDados::DiferencaMedia,
                QuestaoDados::DiferencaSignificativa,
            ],
            TipoGerador::Subgrupos => vec![
                QuestaoDados::PrimeiroSubgrupoFora { base: 20 },
                QuestaoDados::Cpk {
                    lie: None,
                    lse: None,
                },
            ],
            TipoGerador::Simulacao => vec![
                QuestaoDados::GrupoMaiorMedia,
                QuestaoDados::GrupoMaisVariavel,
            ],
        }
    }

    /// Whether a question of the kind of `q` fits a set of this kind.
    pub fn aceita(self, q: &QuestaoDados) -> bool {
        self.questoes()
            .iter()
            .any(|x| discriminant(x) == discriminant(q))
    }

    /// The tool of UTSim Qualidade that opens a set of this kind.
    pub fn ferramenta(self) -> Option<&'static str> {
        match self {
            TipoGerador::Tempos => Some("histograma"),
            TipoGerador::Msa => Some("msa"),
            TipoGerador::MediaDeslocada | TipoGerador::Simulacao => Some("estratificacao"),
            TipoGerador::Serie | TipoGerador::Subgrupos => Some("carta"),
            TipoGerador::Fatorial | TipoGerador::Pareado => None,
        }
    }

    /// The first name of a new set of this kind.
    pub fn nome(self) -> &'static str {
        match self {
            TipoGerador::Tempos => "Tempos de processo",
            TipoGerador::Msa => "Estudo de R&R",
            TipoGerador::MediaDeslocada => "Medidas por grupo",
            TipoGerador::Serie => "Série de medidas",
            TipoGerador::Fatorial => "Experimento fatorial",
            TipoGerador::Pareado => "Comparação pareada",
            TipoGerador::Subgrupos => "Subgrupos de medidas",
            TipoGerador::Simulacao => "Registros da simulação",
        }
    }
}

/// The tolerance a new question of the kind of `q` starts with: none for a choice or a
/// position, a little for an estimate.
pub fn tolerancia_padrao(q: &QuestaoDados) -> Tolerancia {
    match q {
        QuestaoDados::RrPercentual => Tolerancia::Absoluta(1.5),
        QuestaoDados::Ppk { .. } | QuestaoDados::Cpk { .. } => Tolerancia::Absoluta(0.05),
        QuestaoDados::MeiaLargura { .. }
        | QuestaoDados::Efeito { .. }
        | QuestaoDados::DiferencaMedia => Tolerancia::Relativa(0.05),
        QuestaoDados::Replicacoes { .. } => Tolerancia::Relativa(0.1),
        _ => Tolerancia::Absoluta(0.0),
    }
}

/// The unit a new question of the kind of `q` starts with.
pub fn unidade_padrao(q: &QuestaoDados) -> &'static str {
    match q {
        QuestaoDados::RrPercentual => "%",
        _ => "",
    }
}

/// The rows a generator makes.
pub fn linhas(g: &Gerador) -> usize {
    let m = |a: usize, b: usize| a.saturating_mul(b);
    match g {
        Gerador::Tempos { n, .. } | Gerador::Serie { n, .. } | Gerador::Pareado { n, .. } => *n,
        Gerador::Msa {
            pecas,
            operadores,
            repeticoes,
            ..
        } => m(m(*pecas, *operadores), *repeticoes),
        Gerador::MediaDeslocada {
            grupos,
            n_por_grupo,
            ..
        } => m(grupos.len(), *n_por_grupo),
        Gerador::Fatorial {
            fatores, replicas, ..
        } => {
            if fatores.len() > MAX_FATORES {
                usize::MAX
            } else {
                m(1 << fatores.len(), *replicas)
            }
        }
        Gerador::Subgrupos { k, .. } => *k,
        // Capped by the generator itself (`dados::MAX_REGISTROS`).
        Gerador::Simulacao { .. } => 0,
    }
}

/// The points (or subgroups) of a generator with a pattern in time order.
fn pontos(g: &Gerador) -> usize {
    match g {
        Gerador::Serie { n, .. } => *n,
        Gerador::Subgrupos { k, .. } => *k,
        _ => 0,
    }
}

/// The problems of a generator (empty when it makes a usable set).
pub fn erros_do_gerador(g: &Gerador) -> Vec<String> {
    let mut e = Vec::new();
    let finito = |x: f64| x.is_finite();
    let sd_ok = |x: f64| x.is_finite() && x >= 0.0;
    match g {
        Gerador::Tempos { dist, n } => {
            if *n < 2 {
                e.push("são precisos ao menos 2 valores".into());
            }
            if let Err(x) = dist.validate() {
                e.push(format!("distribuição: {x}"));
            }
        }
        Gerador::Msa {
            pecas,
            operadores,
            repeticoes,
            media,
            sd_peca,
            sd_operador,
            sd_repetibilidade,
        } => {
            if *pecas < 2 || *operadores < 2 || *repeticoes < 2 {
                e.push("o estudo precisa de ao menos 2 peças, 2 operadores e 2 repetições".into());
            }
            if !finito(*media) {
                e.push("média inválida".into());
            }
            if ![*sd_peca, *sd_operador, *sd_repetibilidade]
                .iter()
                .all(|x| sd_ok(*x))
            {
                e.push("os desvios padrão não podem ser negativos".into());
            } else if *sd_peca <= 0.0 && *sd_operador <= 0.0 && *sd_repetibilidade <= 0.0 {
                e.push("ao menos um desvio padrão precisa ser maior que zero".into());
            }
        }
        Gerador::MediaDeslocada {
            grupos,
            n_por_grupo,
            media,
            sd,
            afetado,
            deslocamento,
        } => {
            if grupos.len() < 2 {
                e.push("são precisos ao menos 2 grupos".into());
            }
            if grupos.iter().any(|g| g.trim().is_empty()) {
                e.push("grupo sem nome".into());
            }
            let nomes: BTreeSet<&str> = grupos.iter().map(|g| g.trim()).collect();
            if nomes.len() != grupos.len() {
                e.push("dois grupos com o mesmo nome".into());
            }
            if *afetado >= grupos.len() {
                e.push("escolha o grupo deslocado".into());
            }
            if *n_por_grupo < 2 {
                e.push("são precisos ao menos 2 valores por grupo".into());
            }
            if !finito(*media) || !finito(*deslocamento) {
                e.push("média ou deslocamento inválido".into());
            }
            if !sd_ok(*sd) || *sd <= 0.0 {
                e.push("o desvio padrão precisa ser maior que zero".into());
            }
        }
        Gerador::Serie {
            n,
            media,
            sd,
            padrao,
        } => {
            if *n < 2 {
                e.push("são precisos ao menos 2 pontos".into());
            }
            if !finito(*media) {
                e.push("média inválida".into());
            }
            if !sd_ok(*sd) || *sd <= 0.0 {
                e.push("o desvio padrão precisa ser maior que zero".into());
            }
            e.extend(erros_do_padrao(padrao, *n));
        }
        Gerador::Fatorial {
            fatores,
            replicas,
            media,
            sd,
            efeitos,
            interacao_ab,
        } => {
            if fatores.is_empty() || fatores.len() > MAX_FATORES {
                e.push(format!("de 1 a {MAX_FATORES} fatores"));
            }
            if fatores.iter().any(|f| f.trim().is_empty()) {
                e.push("fator sem nome".into());
            }
            let nomes: BTreeSet<&str> = fatores.iter().map(|f| f.trim()).collect();
            if nomes.len() != fatores.len() {
                e.push("dois fatores com o mesmo nome".into());
            }
            if efeitos.len() != fatores.len() {
                e.push("cada fator precisa do seu efeito".into());
            }
            if *replicas < 1 {
                e.push("ao menos 1 réplica".into());
            }
            if !finito(*media) || !finito(*interacao_ab) || efeitos.iter().any(|x| !x.is_finite()) {
                e.push("média, efeito ou interação inválido".into());
            }
            if !sd_ok(*sd) {
                e.push("o desvio padrão não pode ser negativo".into());
            }
        }
        Gerador::Pareado {
            n,
            media,
            sd,
            diferenca,
            sd_diferenca,
        } => {
            if *n < 2 {
                e.push("são precisos ao menos 2 pares".into());
            }
            if !finito(*media) || !finito(*diferenca) {
                e.push("média ou diferença inválida".into());
            }
            if !sd_ok(*sd) || !sd_ok(*sd_diferenca) {
                e.push("os desvios padrão não podem ser negativos".into());
            } else if *sd_diferenca <= 0.0 {
                e.push("o desvio da diferença precisa ser maior que zero".into());
            }
        }
        Gerador::Subgrupos {
            k,
            tamanho,
            media,
            sd,
            padrao,
        } => {
            if *k < 2 {
                e.push("são precisos ao menos 2 subgrupos".into());
            }
            if !(2..=25).contains(tamanho) {
                e.push("o subgrupo tem de 2 a 25 medidas".into());
            }
            if !finito(*media) {
                e.push("média inválida".into());
            }
            if !sd_ok(*sd) || *sd <= 0.0 {
                e.push("o desvio padrão precisa ser maior que zero".into());
            }
            if let Some(p) = padrao {
                e.extend(erros_do_padrao(p, *k));
            }
        }
        // The stations are checked against the model ([`Edicao::erros_do_conjunto`]).
        Gerador::Simulacao {
            processo, duracao, ..
        } => {
            if !(duracao.is_finite() && *duracao > 0.0) {
                e.push("a duração da coleta deve ser positiva".into());
            }
            if let Err(x) = processo.validate() {
                e.push(format!("tempo de processo plantado: {x}"));
            }
        }
    }
    let n = linhas(g);
    if n > MAX_LINHAS {
        e.push(format!("{n} linhas: reduza o conjunto (até {MAX_LINHAS})"));
    }
    e
}

fn erros_do_padrao(p: &Padrao, n: usize) -> Vec<String> {
    let (inicio, x) = match p {
        Padrao::Deslocamento { inicio, tamanho } => (*inicio, *tamanho),
        Padrao::Tendencia { inicio, inclinacao } => (*inicio, *inclinacao),
    };
    let mut e = Vec::new();
    if inicio >= n {
        e.push("o padrão começa depois do fim dos dados".into());
    }
    if !x.is_finite() {
        e.push("tamanho do padrão inválido".into());
    }
    e
}

/// The problems of the parameters of a question over the set `cj`.
fn erros_da_questao(q: &QuestaoDados, cj: &ConjuntoMissao) -> Vec<String> {
    let mut e = Vec::new();
    let limites = |lie: &Option<f64>, lse: &Option<f64>, e: &mut Vec<String>| {
        if lie.is_none() && lse.is_none() {
            e.push("dê ao menos um limite de especificação".into());
        }
        if lie.is_some_and(|x| !x.is_finite()) || lse.is_some_and(|x| !x.is_finite()) {
            e.push("limite de especificação inválido".into());
        }
        if let (Some(a), Some(b)) = (lie, lse)
            && a >= b
        {
            e.push("o limite inferior precisa ser menor que o superior".into());
        }
    };
    match q {
        QuestaoDados::Ppk { lie, lse } | QuestaoDados::Cpk { lie, lse } => {
            limites(lie, lse, &mut e)
        }
        QuestaoDados::PrimeiroSinal { base } | QuestaoDados::PrimeiroSubgrupoFora { base } => {
            let n = pontos(&cj.gerador);
            if *base < 2 || *base >= n {
                e.push(format!(
                    "a base dos limites precisa de 2 a {} pontos",
                    n.saturating_sub(1)
                ));
            }
        }
        QuestaoDados::Efeito { fator } => {
            if let crate::dados::Gerador::Fatorial { fatores, .. } = &cj.gerador
                && *fator >= fatores.len()
            {
                e.push("escolha o fator".into());
            }
        }
        QuestaoDados::MeiaLargura { confianca } if !(*confianca > 0.0 && *confianca < 1.0) => {
            e.push("a confiança fica entre 0 e 1 (0,95 para 95 %)".into());
        }
        QuestaoDados::Replicacoes { meia_largura }
            if !(meia_largura.is_finite() && *meia_largura > 0.0) =>
        {
            e.push("a meia largura precisa ser maior que zero".into());
        }
        _ => {}
    }
    e
}

/// The answer of a data question for the sample of the editor, as the student would see
/// it, and the share of test students whose data have an answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Amostra {
    /// The right answer for the sample data (the fixed seed), as text, or why there is none.
    pub resposta: Result<String, String>,
    /// How many of the [`ALUNOS_DE_TESTE`] students have data with no answer.
    pub sem_resposta: usize,
    /// For a question that asks for a group: how many test students whose data point to
    /// another group than the one with the planted effect (the effect lost in the noise).
    pub fora_do_plantado: usize,
}

/// Whether the set draws its affected group by default (shifted means and simulation
/// records, so neighbours do not share the answer).
fn sorteia(tipo: TipoGerador) -> bool {
    matches!(tipo, TipoGerador::MediaDeslocada | TipoGerador::Simulacao)
}

/// The group with the planted effect in the recipe of a set.
fn grupo_plantado(g: &Gerador) -> Option<usize> {
    match g {
        Gerador::MediaDeslocada { afetado, .. } | Gerador::Simulacao { afetado, .. } => {
            Some(*afetado)
        }
        _ => None,
    }
}

impl Edicao {
    /// The first values of a set of kind `tipo` for this model: a simulation set records
    /// every machine of the model, and the planted cause makes the first one half as slow
    /// again, with little spread.
    pub fn gerador_padrao(&self, tipo: TipoGerador) -> Gerador {
        let mut g = tipo.padrao();
        if let Gerador::Simulacao {
            estacoes, processo, ..
        } = &mut g
        {
            *estacoes = estacoes_do_modelo(&self.modelo);
            let media = self
                .modelo
                .objects
                .iter()
                .find_map(|o| match &o.kind {
                    utsim_model::ObjectKind::Processor { process, .. }
                        if estacoes.first() == Some(&o.name) =>
                    {
                        Some(process.mean())
                    }
                    _ => None,
                })
                .filter(|m| m.is_finite() && *m > 0.0);
            if let Some(m) = media {
                *processo = Dist::Normal {
                    mean: crate::dados::arredondar(m * 1.5),
                    sd: crate::dados::arredondar(m * 0.15),
                };
            }
        }
        g
    }

    /// The problems of the generator of `cj`, with the stations of a simulation set
    /// checked against the model.
    pub fn erros_do_conjunto(&self, cj: &ConjuntoMissao) -> Vec<String> {
        let mut e = erros_do_gerador(&cj.gerador);
        e.extend(erros_da_simulacao(&cj.gerador, &self.modelo));
        e
    }

    /// The data of `cj` for `aluno` (empty: the fixed seed of the samples).
    fn dados_de(&self, cj: &ConjuntoMissao, aluno: &str) -> Conjunto {
        cj.gerar_com(&self.base.id, aluno, &self.modelo)
    }

    /// A free id for a new data set: `d1`, `d2`, ...
    pub fn id_conjunto_livre(&self) -> String {
        (1..)
            .map(|n| format!("d{n}"))
            .find(|id| !self.base.dados.iter().any(|c| &c.id == id))
            .expect("some id is free")
    }

    /// Adds a data set of kind `tipo` with its first values, and returns its id.
    pub fn novo_conjunto(&mut self, tipo: TipoGerador) -> String {
        let id = self.id_conjunto_livre();
        self.base.dados.push(ConjuntoMissao {
            id: id.clone(),
            nome: tipo.nome().into(),
            gerador: self.gerador_padrao(tipo),
            sortear_afetado: sorteia(tipo),
            ferramenta: tipo.ferramenta().map(str::to_string),
            lie: None,
            lse: None,
        });
        id
    }

    /// Changes the kind of the set `i`: new first values, the tool of the kind, and the
    /// questions about it that no longer fit go to the first question of the new kind.
    pub fn mudar_tipo(&mut self, i: usize, tipo: TipoGerador) {
        let Some(cj) = self.base.dados.get(i) else {
            return;
        };
        if TipoGerador::de(&cj.gerador) == tipo {
            return;
        }
        let g = self.gerador_padrao(tipo);
        let Some(cj) = self.base.dados.get_mut(i) else {
            return;
        };
        cj.gerador = g;
        cj.sortear_afetado = sorteia(tipo);
        cj.ferramenta = tipo.ferramenta().map(str::to_string);
        let id = cj.id.clone();
        let primeira = tipo.questoes().remove(0);
        for p in &mut self.base.previsao {
            if let Previsao::Dados {
                conjunto,
                questao,
                unidade,
                tolerancia,
                ..
            } = p
                && *conjunto == id
                && !tipo.aceita(questao)
            {
                *questao = primeira.clone();
                *unidade = unidade_padrao(questao).into();
                *tolerancia = tolerancia_padrao(questao);
            }
        }
    }

    /// Renames the set `i` to `novo`, and its questions follow.
    pub fn renomear_conjunto(&mut self, i: usize, novo: &str) {
        let Some(cj) = self.base.dados.get_mut(i) else {
            return;
        };
        let velho = std::mem::replace(&mut cj.id, novo.to_string());
        for p in &mut self.base.previsao {
            if let Previsao::Dados { conjunto, .. } = p
                && *conjunto == velho
            {
                *conjunto = novo.to_string();
            }
        }
    }

    /// Removes the set `i` with the questions about it; returns how many questions went.
    pub fn remover_conjunto(&mut self, i: usize) -> usize {
        if i >= self.base.dados.len() {
            return 0;
        }
        let id = self.base.dados.remove(i).id;
        let antes = self.base.previsao.len();
        self.base
            .previsao
            .retain(|p| !matches!(p, Previsao::Dados { conjunto, .. } if *conjunto == id));
        if self.base.dados.is_empty() {
            self.base.analisar_em.clear();
        }
        antes - self.base.previsao.len()
    }

    /// Removes the factor `j` of the factorial set `i`. The questions about the effect of
    /// that factor go (returns how many), those about later factors follow their factor,
    /// and the interaction of the first two factors is zeroed when one of them goes.
    pub fn remover_fator(&mut self, i: usize, j: usize) -> usize {
        let Some(cj) = self.base.dados.get_mut(i) else {
            return 0;
        };
        let Gerador::Fatorial {
            fatores,
            efeitos,
            interacao_ab,
            ..
        } = &mut cj.gerador
        else {
            return 0;
        };
        if j >= fatores.len() || fatores.len() < 2 {
            return 0;
        }
        fatores.remove(j);
        if j < efeitos.len() {
            efeitos.remove(j);
        }
        if j < 2 {
            *interacao_ab = 0.0;
        }
        let id = cj.id.clone();
        let sobre = |p: &Previsao| -> Option<usize> {
            match p {
                Previsao::Dados {
                    conjunto,
                    questao: QuestaoDados::Efeito { fator },
                    ..
                } if *conjunto == id => Some(*fator),
                _ => None,
            }
        };
        let antes = self.base.previsao.len();
        self.base.previsao.retain(|p| sobre(p) != Some(j));
        for p in &mut self.base.previsao {
            if let Some(f) = sobre(p)
                && f > j
                && let Previsao::Dados {
                    questao: QuestaoDados::Efeito { fator },
                    ..
                } = p
            {
                *fator = f - 1;
            }
        }
        antes - self.base.previsao.len()
    }

    /// Adds a question about the set `conjunto` (the first that fits its kind).
    pub fn nova_pergunta_de_dados(&mut self, conjunto: &str) {
        let Some(cj) = self.base.conjunto(conjunto) else {
            return;
        };
        let questao = TipoGerador::de(&cj.gerador).questoes().remove(0);
        let id = self.id_previsao_livre();
        self.base.previsao.push(Previsao::Dados {
            id,
            pergunta: String::new(),
            conjunto: conjunto.to_string(),
            unidade: unidade_padrao(&questao).into(),
            tolerancia: tolerancia_padrao(&questao),
            questao,
        });
    }

    /// The data the editor shows for the set `id` (the fixed seed, as with no student).
    pub fn amostra_de_dados(&self, id: &str) -> Option<Conjunto> {
        let cj = self.base.conjunto(id)?;
        if !self.erros_do_conjunto(cj).is_empty() {
            return None;
        }
        Some(self.dados_de(cj, ""))
    }

    /// The answer of the prediction `i` (a data question) for the sample data, and how
    /// many test students would get data with no answer. `None` when `i` is not a data
    /// question or its set does not exist or is not valid.
    pub fn amostra_da_pergunta(&self, i: usize) -> Option<Amostra> {
        let Some(Previsao::Dados {
            conjunto, questao, ..
        }) = self.base.previsao.get(i)
        else {
            return None;
        };
        let cj = self.base.conjunto(conjunto)?;
        if !self.erros_do_conjunto(cj).is_empty() || !erros_da_questao(questao, cj).is_empty() {
            return None;
        }
        let c = self.dados_de(cj, "");
        let resposta = questao.resposta(&c).map(|v| questao.texto(cj, v));
        let pede_grupo = matches!(
            questao,
            QuestaoDados::GrupoDeslocado
                | QuestaoDados::GrupoMaiorMedia
                | QuestaoDados::GrupoMaisVariavel
        );
        let (mut sem_resposta, mut fora_do_plantado) = (0, 0);
        for k in 1..=ALUNOS_DE_TESTE {
            let aluno = format!("aluno-de-teste-{k}");
            let receita = cj.receita(&self.base.id, &aluno);
            match questao.resposta(&receita.gerar_com(Some(&self.modelo))) {
                Err(_) => sem_resposta += 1,
                Ok(v) => {
                    if pede_grupo
                        && let Some(g) = grupo_plantado(&receita.gerador)
                        && v as usize != g
                    {
                        fora_do_plantado += 1;
                    }
                }
            }
        }
        Some(Amostra {
            resposta,
            sem_resposta,
            fora_do_plantado,
        })
    }

    /// The problems of the data sets, of the questions about them and of the apps where
    /// they are analysed (stage C of the editor).
    pub fn erros_dos_dados(&self) -> Vec<String> {
        let mut e = Vec::new();
        let b = &self.base;
        let mut ids = BTreeSet::new();
        for (i, cj) in b.dados.iter().enumerate() {
            let nome = format!("dados {}", i + 1);
            if !id_valido(&cj.id) || cj.id.contains('/') {
                e.push(format!(
                    "{nome}: id '{}' com letras minúsculas, números, _ e -",
                    cj.id
                ));
            } else if !ids.insert(cj.id.as_str()) {
                e.push(format!("{nome}: id '{}' repetido", cj.id));
            }
            if cj.nome.trim().is_empty() {
                e.push(format!("{nome}: dê um nome ao conjunto"));
            }
            e.extend(
                self.erros_do_conjunto(cj)
                    .into_iter()
                    .map(|x| format!("{nome}: {x}")),
            );
            if let Some(f) = &cj.ferramenta
                && !FERRAMENTAS.contains(&f.as_str())
            {
                e.push(format!("{nome}: ferramenta '{f}' desconhecida"));
            }
            if cj.lie.is_some_and(|x| !x.is_finite()) || cj.lse.is_some_and(|x| !x.is_finite()) {
                e.push(format!("{nome}: limite de especificação inválido"));
            }
            if let (Some(a), Some(z)) = (cj.lie, cj.lse)
                && a >= z
            {
                e.push(format!(
                    "{nome}: o limite inferior precisa ser menor que o superior"
                ));
            }
        }
        let mut apps = BTreeSet::new();
        for a in &b.analisar_em {
            if !APPS_DE_ANALISE.contains(&a.as_str()) {
                e.push(format!("app de análise '{a}' desconhecido"));
            } else if !apps.insert(a) {
                e.push(format!("app de análise '{a}' repetido"));
            }
        }
        if !b.analisar_em.is_empty() && b.dados.is_empty() {
            e.push("analisar em outro app pede ao menos um conjunto de dados".into());
        }
        for (i, p) in b.previsao.iter().enumerate() {
            let Previsao::Dados {
                conjunto,
                questao,
                tolerancia,
                ..
            } = p
            else {
                continue;
            };
            let n = i + 1;
            let t = match tolerancia {
                Tolerancia::Relativa(x) | Tolerancia::Absoluta(x) => *x,
            };
            if t.is_nan() || t < 0.0 {
                e.push(format!("previsão {n}: tolerância negativa"));
            }
            let Some(cj) = b.conjunto(conjunto) else {
                e.push(format!("previsão {n}: escolha o conjunto de dados"));
                continue;
            };
            let tipo = TipoGerador::de(&cj.gerador);
            if !tipo.aceita(questao) {
                e.push(format!(
                    "previsão {n}: a pergunta não serve para este tipo de dados"
                ));
                continue;
            }
            let es = erros_da_questao(questao, cj);
            if !es.is_empty() {
                e.extend(es.into_iter().map(|x| format!("previsão {n}: {x}")));
                continue;
            }
            if !self.erros_do_conjunto(cj).is_empty() {
                continue;
            }
            if questao.e_escolha() && questao.opcoes(cj).len() < 2 {
                e.push(format!(
                    "previsão {n}: a pergunta precisa de grupos no conjunto"
                ));
            }
            if let Err(x) = questao.resposta(&self.dados_de(cj, "")) {
                e.push(format!(
                    "previsão {n}: os dados de exemplo não têm resposta: {x}"
                ));
            }
        }
        e
    }

    /// Warnings that do not stop writing: a set no question asks about, and questions
    /// whose answer is missing for some students (the planted effect too small for the
    /// noise, a pattern no rule catches).
    pub fn avisos_dos_dados(&self) -> Vec<String> {
        let mut v = Vec::new();
        for cj in &self.base.dados {
            let usado = self
                .base
                .previsao
                .iter()
                .any(|p| matches!(p, Previsao::Dados { conjunto, .. } if *conjunto == cj.id));
            if !usado {
                v.push(format!(
                    "o conjunto '{}' não tem pergunta: o aluno só o analisa",
                    cj.nome
                ));
            }
        }
        for i in 0..self.base.previsao.len() {
            let Some(a) = self.amostra_da_pergunta(i) else {
                continue;
            };
            if a.sem_resposta > 0 {
                v.push(format!(
                    "previsão {}: {} de {ALUNOS_DE_TESTE} alunos de teste recebem dados sem resposta (aumente o efeito ou o tamanho)",
                    i + 1,
                    a.sem_resposta
                ));
            }
            if a.fora_do_plantado > 0 {
                v.push(format!(
                    "previsão {}: para {} de {ALUNOS_DE_TESTE} alunos de teste os dados apontam outro grupo que não o do efeito plantado (aumente o efeito ou a coleta)",
                    i + 1,
                    a.fora_do_plantado
                ));
            }
        }
        v
    }
}
