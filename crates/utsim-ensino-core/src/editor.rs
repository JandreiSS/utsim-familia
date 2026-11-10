//! The mission editor of the teacher (`docs/plano_ensino_e_jogo.md`, 6.2): the state of a
//! mission being edited over a UTSim model, the calibration that finds the reference and
//! suggests the goals, and the teacher's own package, where missions are written, opened
//! again, moved and removed. Everything here is data and rules; the screen is in
//! `utsim-ensino-ui`.
//!
//! A mission keeps every field it had when opened again: the editor changes the title,
//! the levers, the measures, the goals, the statement, the questions, the hints, the
//! concepts and the data sets ([`crate::editor_dados`]), and the rest of the [`Missao`]
//! (rounds, pieces) goes back as it was.

use std::collections::BTreeSet;

use utsim_model::Model;

use crate::conferir::{Conferencia, conferir};
use crate::construtor::{rascunho, sugerir_alavancas, sugerir_metricas, texto_inicial};
use crate::missao::{
    Alavanca, Decisoes, Metrica, Missao, Objetivo, Previsao, Resposta, Termo, Tolerancia,
};
use crate::nota::Intervalo;
use crate::pacote::{Arquivos, Ato, Glossario, Pacote, PacoteCarregado, carregar};

/// A lever of the model, used in the mission or not.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemAlavanca {
    pub usar: bool,
    pub alavanca: Alavanca,
}

/// A measure of the model, used in the mission or not.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemMetrica {
    pub usar: bool,
    pub metrica: Metrica,
}

/// A mission being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct Edicao {
    /// The mission as opened (or drafted): the fields the editor does not show.
    pub base: Missao,
    pub modelo_ron: String,
    pub modelo: Model,
    pub texto: String,
    pub alavancas: Vec<ItemAlavanca>,
    pub metricas: Vec<ItemMetrica>,
    /// Path of `missao.ron` in the teacher's package when the mission was opened from it
    /// (or written to it): writing again replaces that mission, whatever its id now.
    pub origem: Option<String>,
}

/// A mission id: words of letters, digits, `_` and `-`, separated by `/`.
pub fn id_valido(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id.split('/').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        })
}

/// A folder name from a title: lower case ASCII, `_` for the rest.
pub fn slug(s: &str) -> String {
    let s: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c if c.is_ascii_alphanumeric() => c,
            _ => '_',
        })
        .collect();
    let mut out = String::new();
    for c in s.chars() {
        if c == '_' && out.ends_with('_') {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() { "missao".into() } else { out }
}

impl Edicao {
    /// A new mission over the model text `modelo_ron`: every suggested lever and measure,
    /// the lead times in use and the throughputs shown but out of the grade.
    pub fn nova(id: &str, titulo: &str, modelo_ron: &str) -> Result<Edicao, String> {
        let modelo = Model::parse(modelo_ron).map_err(|e| format!("modelo: {e}"))?;
        let alavancas = sugerir_alavancas(&modelo);
        let metricas = sugerir_metricas(&modelo);
        let base = rascunho(id, titulo, &modelo, Vec::new(), Vec::new());
        let texto = texto_inicial(titulo, &modelo, &alavancas);
        Ok(Edicao {
            base,
            modelo_ron: modelo_ron.to_string(),
            modelo,
            texto,
            alavancas: alavancas
                .into_iter()
                .map(|alavanca| ItemAlavanca {
                    usar: true,
                    alavanca,
                })
                .collect(),
            metricas: metricas
                .into_iter()
                .map(|metrica| ItemMetrica {
                    usar: metrica.peso > 0.0,
                    metrica,
                })
                .collect(),
            origem: None,
        })
    }

    /// A mission written before, opened again: its levers and measures in use, followed by
    /// the suggestions of the model it does not use yet.
    pub fn abrir(missao: Missao, modelo_ron: &str, texto: &str) -> Result<Edicao, String> {
        let modelo = Model::parse(modelo_ron).map_err(|e| format!("modelo: {e}"))?;
        let mut alavancas: Vec<ItemAlavanca> = missao
            .alavancas
            .iter()
            .cloned()
            .map(|alavanca| ItemAlavanca {
                usar: true,
                alavanca,
            })
            .collect();
        for a in sugerir_alavancas(&modelo) {
            if !alavancas.iter().any(|x| x.alavanca.id() == a.id()) {
                alavancas.push(ItemAlavanca {
                    usar: false,
                    alavanca: a,
                });
            }
        }
        let mut metricas: Vec<ItemMetrica> = missao
            .metricas
            .iter()
            .cloned()
            .map(|metrica| ItemMetrica {
                usar: true,
                metrica,
            })
            .collect();
        for m in sugerir_metricas(&modelo) {
            if !metricas.iter().any(|x| x.metrica.nome == m.nome) {
                metricas.push(ItemMetrica {
                    usar: false,
                    metrica: m,
                });
            }
        }
        Ok(Edicao {
            base: missao,
            modelo_ron: modelo_ron.to_string(),
            modelo,
            texto: texto.to_string(),
            alavancas,
            metricas,
            origem: None,
        })
    }

    /// The mission as it will be written: the base with the levers and measures in use.
    /// A reference that names a lever no longer in use is dropped (calibrate again).
    pub fn missao(&self) -> Missao {
        let mut m = self.base.clone();
        m.alavancas = self
            .alavancas
            .iter()
            .filter(|a| a.usar)
            .map(|a| a.alavanca.clone())
            .collect();
        m.metricas = self
            .metricas
            .iter()
            .filter(|x| x.usar)
            .map(|x| x.metrica.clone())
            .collect();
        let ids: BTreeSet<&str> = m.alavancas.iter().map(|a| a.id()).collect();
        if m.referencia.keys().any(|k| !ids.contains(k.as_str())) {
            m.referencia = Decisoes::new();
        }
        m
    }

    /// The problems of the fields, before the full check of the package (empty when the
    /// mission can be written).
    pub fn erros(&self) -> Vec<String> {
        let mut e = Vec::new();
        if !id_valido(&self.base.id) {
            e.push(format!(
                "id '{}': use letras minúsculas, números, _ e -, com / entre as partes",
                self.base.id
            ));
        }
        if self.base.titulo.trim().is_empty() {
            e.push("a missão precisa de um título".into());
        }
        if self.texto.trim().is_empty() {
            e.push("o enunciado está vazio".into());
        }
        let usadas: Vec<&ItemAlavanca> = self.alavancas.iter().filter(|a| a.usar).collect();
        let so_perguntas = usadas.is_empty() && !self.base.previsao.is_empty();
        if usadas.is_empty() && !so_perguntas {
            e.push("marque ao menos uma alavanca".into());
        }
        for a in &usadas {
            let a = &a.alavanca;
            if a.rotulo().trim().is_empty() {
                e.push(format!("alavanca '{}' sem rótulo", a.id()));
            }
            match a {
                Alavanca::Quantidade {
                    de,
                    ate,
                    custo_unitario,
                    ..
                } => {
                    if de > ate {
                        e.push(format!("{}: 'de' maior que 'até'", a.rotulo()));
                    }
                    if *custo_unitario < 0.0 {
                        e.push(format!("{}: custo negativo", a.rotulo()));
                    }
                }
                Alavanca::Valor {
                    de,
                    ate,
                    passo,
                    custo_unitario,
                    ..
                } => {
                    if de > ate || *passo <= 0.0 {
                        e.push(format!("{}: faixa ou passo impossível", a.rotulo()));
                    } else if (ate - de) / passo > 1000.0 {
                        e.push(format!("{}: mais de 1000 valores na faixa", a.rotulo()));
                    }
                    if *custo_unitario < 0.0 {
                        e.push(format!("{}: custo negativo", a.rotulo()));
                    }
                }
                Alavanca::Escolha { opcoes, .. } => {
                    if opcoes.len() < 2 {
                        e.push(format!("{}: são precisas duas opções", a.rotulo()));
                    }
                }
            }
        }
        let metricas: Vec<&Metrica> = self
            .metricas
            .iter()
            .filter(|m| m.usar)
            .map(|m| &m.metrica)
            .collect();
        if metricas.is_empty() && !so_perguntas {
            e.push("marque ao menos uma métrica".into());
        } else if !so_perguntas && metricas.iter().all(|m| m.peso <= 0.0) {
            e.push("ao menos uma métrica precisa de peso maior que zero".into());
        }
        for m in &metricas {
            if m.rotulo.trim().is_empty() {
                e.push(format!("métrica '{}' sem rótulo", m.nome));
            }
            if m.peso < 0.0 {
                e.push(format!("{}: peso negativo", m.rotulo));
            }
            let t = match m.tolerancia {
                Tolerancia::Relativa(x) | Tolerancia::Absoluta(x) => x,
            };
            if t.is_nan() || t < 0.0 {
                e.push(format!("{}: tolerância negativa", m.rotulo));
            }
            if m.meta.is_some_and(|x| !x.is_finite()) {
                e.push(format!("{}: meta inválida", m.rotulo));
            }
        }
        if self.combinacoes() > LIMITE_COMBINACOES {
            e.push(format!(
                "{} combinações de alavancas: reduza as faixas (até {LIMITE_COMBINACOES})",
                self.combinacoes()
            ));
        }
        e.extend(self.erros_das_perguntas());
        e.extend(self.erros_dos_dados());
        e
    }

    /// The problems of the questions, the measures shown before predicting, the hints and
    /// the concepts (stage B of the editor).
    pub fn erros_das_perguntas(&self) -> Vec<String> {
        let mut e = Vec::new();
        let b = &self.base;
        let tolerancia_ruim = |t: &Tolerancia| {
            let x = match t {
                Tolerancia::Relativa(x) | Tolerancia::Absoluta(x) => *x,
            };
            x.is_nan() || x < 0.0
        };
        let termos_ruins = |v: &[Termo]| {
            v.iter()
                .any(|t| t.chave.trim().is_empty() || !t.fator.is_finite())
        };
        for (i, m) in b.medicoes.iter().enumerate() {
            let nome = if m.rotulo.trim().is_empty() {
                format!("medição {}", i + 1)
            } else {
                m.rotulo.clone()
            };
            if m.rotulo.trim().is_empty() {
                e.push(format!("{nome}: sem rótulo"));
            }
            if m.valor.is_empty() || termos_ruins(&m.valor) {
                e.push(format!("{nome}: escolha a métrica do modelo"));
            }
        }
        let mut ids = BTreeSet::new();
        for (i, p) in b.previsao.iter().enumerate() {
            let n = i + 1;
            if !id_valido(p.id()) || p.id().contains('/') {
                e.push(format!(
                    "previsão {n}: id '{}' com letras minúsculas, números, _ e -",
                    p.id()
                ));
            } else if !ids.insert(p.id().to_string()) {
                e.push(format!("previsão {n}: id '{}' repetido", p.id()));
            }
            if p.pergunta().trim().is_empty() {
                e.push(format!("previsão {n}: escreva a pergunta"));
            }
            match p {
                Previsao::Escolha {
                    opcoes, resposta, ..
                } => {
                    if opcoes.len() < 2 {
                        e.push(format!("previsão {n}: são precisas duas opções"));
                    }
                    if opcoes.iter().any(|o| o.rotulo.trim().is_empty()) {
                        e.push(format!("previsão {n}: opção sem texto"));
                    }
                    match resposta {
                        Resposta::Fixa(k) if *k >= opcoes.len() => {
                            e.push(format!("previsão {n}: marque a opção certa"));
                        }
                        Resposta::Maior | Resposta::Menor
                            if opcoes
                                .iter()
                                .any(|o| o.valor.is_empty() || termos_ruins(&o.valor)) =>
                        {
                            e.push(format!(
                                "previsão {n}: cada opção precisa da métrica que decide a resposta"
                            ));
                        }
                        _ => {}
                    }
                }
                Previsao::Valor {
                    valor, tolerancia, ..
                } => {
                    if valor.is_empty() || termos_ruins(valor) {
                        e.push(format!("previsão {n}: escolha a métrica do modelo"));
                    }
                    if tolerancia_ruim(tolerancia) {
                        e.push(format!("previsão {n}: tolerância negativa"));
                    }
                }
                Previsao::Calculo {
                    calculo,
                    tolerancia,
                    ..
                } => {
                    if !calculo.valor().is_finite() {
                        e.push(format!("previsão {n}: o cálculo não dá um número"));
                    }
                    if tolerancia_ruim(tolerancia) {
                        e.push(format!("previsão {n}: tolerância negativa"));
                    }
                }
                Previsao::Dados { .. } => {}
            }
        }
        for (i, q) in b.justificativa.iter().enumerate() {
            let n = i + 1;
            if q.pergunta.trim().is_empty() {
                e.push(format!("justificativa {n}: escreva a pergunta"));
            }
            if q.opcoes.len() < 2 {
                e.push(format!("justificativa {n}: são precisas duas opções"));
            }
            if q.opcoes.iter().any(|o| o.trim().is_empty()) {
                e.push(format!("justificativa {n}: opção sem texto"));
            }
            if q.certa >= q.opcoes.len() {
                e.push(format!("justificativa {n}: marque a opção certa"));
            }
        }
        if b.dicas.len() > 3 {
            e.push("no máximo três dicas".into());
        }
        if b.dicas.iter().any(|d| d.trim().is_empty()) {
            e.push("dica vazia: escreva ou remova".into());
        }
        let mut cs = BTreeSet::new();
        for c in &b.conceitos {
            if !cs.insert(c) {
                e.push(format!("conceito '{c}' repetido"));
            }
        }
        e
    }

    /// A free id for a new prediction: `p1`, `p2`, ...
    pub fn id_previsao_livre(&self) -> String {
        (1..)
            .map(|n| format!("p{n}"))
            .find(|id| !self.base.previsao.iter().any(|p| p.id() == id))
            .expect("some id is free")
    }

    /// The keys a term can name, besides the metrics of the model: the cost of the
    /// decisions and the number of each lever in use.
    pub fn chaves_de_decisao(&self) -> Vec<String> {
        let mut v = vec!["custo".to_string()];
        v.extend(
            self.alavancas
                .iter()
                .filter(|a| a.usar)
                .map(|a| format!("decisao.{}", a.alavanca.id())),
        );
        v
    }

    /// The number of combinations of the levers in use (what the calibration simulates).
    pub fn combinacoes(&self) -> u64 {
        self.alavancas
            .iter()
            .filter(|a| a.usar)
            .map(|a| a.alavanca.valores().len().max(1) as u64)
            .fold(1u64, |acc, n| acc.saturating_mul(n))
    }

    /// The mission alone in a package, loaded and checked like any package.
    pub fn carregar(&self) -> Result<PacoteCarregado, Vec<String>> {
        let erros = self.erros();
        if !erros.is_empty() {
            return Err(erros);
        }
        let mut p = PacoteProfessor::novo("rascunho", "Rascunho", "");
        let mut e = self.clone();
        e.origem = None;
        e.base.modelo = "modelo.ron".into();
        e.base.texto = "texto.md".into();
        // The concepts are checked against the teacher's package when writing, not here.
        e.base.conceitos.clear();
        p.gravar(&mut e, 0)?;
        p.conferir()
    }

    /// Applies a calibration: the reference and, when `metas` is true, the suggested goals.
    pub fn aplicar(&mut self, c: &Calibragem, metas: bool) {
        self.base.referencia = c.conferencia.melhor.clone();
        if metas {
            for (nome, v) in &c.metas {
                if let Some(x) = self
                    .metricas
                    .iter_mut()
                    .find(|x| x.usar && &x.metrica.nome == nome)
                {
                    x.metrica.meta = Some(*v);
                }
            }
        }
    }
}

/// The most combinations a mission may have: those of the exhaustive search of the
/// checker, which the calibration runs.
pub const LIMITE_COMBINACOES: u64 = crate::conferir::MAX_COMBINACOES as u64;

fn ron_missao(m: &Missao) -> String {
    let cfg = ron::ser::PrettyConfig::new().struct_names(true);
    ron::ser::to_string_pretty(m, cfg).expect("mission serializes")
}

/// The result of a calibration.
#[derive(Clone, Debug)]
pub struct Calibragem {
    pub conferencia: Conferencia,
    /// Mean of each measure in use with the initial decisions and with the reference.
    pub base: Vec<(String, f64)>,
    pub referencia: Vec<(String, f64)>,
    /// Suggested goal of each measure with weight: 5 % of slack around the reference,
    /// rounded to its decimals.
    pub metas: Vec<(String, f64)>,
}

impl Calibragem {
    /// The share of the feasible combinations that reach every goal (0 to 1).
    pub fn fracao_meta(&self) -> f64 {
        if self.conferencia.viaveis == 0 {
            0.0
        } else {
            self.conferencia.batem_meta as f64 / self.conferencia.viaveis as f64
        }
    }
}

/// Simulates every combination of the levers of `ed` (the exhaustive check of the
/// package), finds the reference and suggests the goals. It takes as long as the
/// combinations times the replications.
pub fn calibrar(ed: &Edicao) -> Result<Calibragem, Vec<String>> {
    let p = ed.carregar()?;
    let m = &p.missoes[0];
    let c = conferir(m).map_err(|e| vec![e.to_string()])?;
    if c.viaveis == 0 {
        let mut e = vec![
            "nenhuma combinação cabe no orçamento e nas restrições: aumente o orçamento ou mude as faixas"
                .to_string(),
        ];
        e.extend(c.erros);
        return Err(e);
    }
    let inicial: Decisoes = m
        .missao
        .alavancas
        .iter()
        .map(|a| (a.id().to_string(), a.inicial()))
        .collect();
    let sim = crate::motor::rodar(
        m,
        "",
        &[m.missao.completar(&inicial), m.missao.completar(&c.melhor)],
        &|_| true,
    )
    .map_err(|e| vec![e.to_string()])?;
    let media = |mt: &Metrica, i: usize| -> Result<f64, Vec<String>> {
        Ok(Intervalo::de(
            &crate::motor::valor_metrica(&m.missao, mt, &sim[i])
                .map_err(|e| vec![e.to_string()])?,
        )
        .media)
    };
    let mut base = Vec::new();
    let mut referencia = Vec::new();
    let mut metas = Vec::new();
    for mt in &m.missao.metricas {
        let b = media(mt, 0)?;
        let r = media(mt, 1)?;
        base.push((mt.nome.clone(), b));
        referencia.push((mt.nome.clone(), r));
        if mt.peso > 0.0 {
            metas.push((mt.nome.clone(), meta_sugerida(r, mt.objetivo, mt.casas)));
        }
    }
    Ok(Calibragem {
        conferencia: c,
        base,
        referencia,
        metas,
    })
}

/// The goal 5 % of slack around the reference value `r`, rounded to `casas` decimals
/// toward the slack, so the reference always reaches it.
pub fn meta_sugerida(r: f64, objetivo: Objetivo, casas: u32) -> f64 {
    let f = 10f64.powi(casas as i32);
    match objetivo {
        Objetivo::Menor => ((r + 0.05 * r.abs()) * f).ceil() / f,
        Objetivo::Maior => ((r - 0.05 * r.abs()) * f).floor() / f,
    }
}

/// `p` relative to the folder `dir` of a package, with `..` resolved.
fn junta(dir: &str, p: &str) -> String {
    let todo = if dir.is_empty() || p.starts_with('/') {
        p.trim_start_matches('/').to_string()
    } else {
        format!("{dir}/{p}")
    };
    let mut out: Vec<&str> = Vec::new();
    for parte in todo.split('/') {
        match parte {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            x => out.push(x),
        }
    }
    out.join("/")
}

fn pasta(caminho: &str) -> &str {
    caminho.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The metric keys the model produces, sorted (one replication is run): what a measure
/// or a prediction can name, besides [`Edicao::chaves_de_decisao`].
pub fn chaves_do_modelo(modelo: &Model) -> Result<Vec<String>, String> {
    let mut m = modelo.clone();
    m.run.replications = 1;
    let reports = m
        .run_replications()
        .map_err(|e| format!("o modelo não roda: {e}"))?;
    let v: BTreeSet<String> = reports
        .iter()
        .flat_map(|r| r.metrics().into_keys())
        .collect();
    Ok(v.into_iter().collect())
}

/// The glossary of the built-in campaign: the concepts any mission can name.
pub fn glossario_da_campanha() -> Glossario {
    crate::campanha::arquivo("glossario.ron")
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| crate::pacote::ron_options().from_str::<Glossario>(t).ok())
        .unwrap_or_default()
}

/// A mission of the teacher's package, as listed.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemPacote {
    /// Index of the act.
    pub ato: usize,
    /// Path of `missao.ron` inside the package.
    pub caminho: String,
    pub id: String,
    pub titulo: String,
}

/// The teacher's own package: the manifest and the files.
#[derive(Clone, Debug, PartialEq)]
pub struct PacoteProfessor {
    pub pacote: Pacote,
    pub arquivos: Arquivos,
}

impl PacoteProfessor {
    /// An empty package with one act.
    pub fn novo(id: &str, titulo: &str, autor: &str) -> PacoteProfessor {
        PacoteProfessor {
            pacote: Pacote {
                id: id.to_string(),
                formato: crate::pacote::FORMATO_PACOTE,
                titulo: titulo.to_string(),
                versao: "0.1.0".into(),
                autor: autor.to_string(),
                descricao: String::new(),
                idioma: "pt-BR".into(),
                requer: Vec::new(),
                atos: vec![Ato {
                    numero: 1,
                    titulo: titulo.to_string(),
                    missoes: Vec::new(),
                }],
                pesos: Default::default(),
                registro: Default::default(),
                glossario: None,
                competencias: Default::default(),
                prova: None,
            },
            arquivos: Arquivos::new(),
        }
    }

    /// A package read from its files (a folder or a `.utse`).
    pub fn de_arquivos(arquivos: Arquivos) -> Result<PacoteProfessor, String> {
        let pacote = crate::pacote::manifesto(&arquivos)?;
        if pacote.prova.is_some() {
            return Err("este pacote é uma prova: o editor não abre provas".into());
        }
        Ok(PacoteProfessor { pacote, arquivos })
    }

    /// Every mission, in the order of the acts.
    pub fn missoes(&self) -> Vec<ItemPacote> {
        let mut out = Vec::new();
        for (i, ato) in self.pacote.atos.iter().enumerate() {
            for c in &ato.missoes {
                let m = self
                    .arquivos
                    .get(c)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .and_then(|t| Missao::parse(t).ok());
                out.push(ItemPacote {
                    ato: i,
                    caminho: c.clone(),
                    id: m.as_ref().map(|m| m.id.clone()).unwrap_or_default(),
                    titulo: m.map(|m| m.titulo).unwrap_or_default(),
                });
            }
        }
        out
    }

    fn texto(&self, path: &str) -> Result<String, String> {
        let b = self
            .arquivos
            .get(path)
            .ok_or_else(|| format!("'{path}' não está no pacote"))?;
        String::from_utf8(b.clone()).map_err(|_| format!("'{path}' não é texto UTF-8"))
    }

    /// Opens the mission at `caminho` for editing.
    pub fn abrir(&self, caminho: &str) -> Result<Edicao, String> {
        let missao = Missao::parse(&self.texto(caminho)?)?;
        let dir = pasta(caminho);
        let junta = |p: &str| junta(dir, p);
        if missao.modelagem.is_some() || missao.tem_pecas() {
            return Err("o editor ainda não abre missões de modelagem ou de peças".into());
        }
        let modelo = self.texto(&junta(&missao.modelo))?;
        let texto = self.texto(&junta(&missao.texto))?;
        let mut ed = Edicao::abrir(missao, &modelo, &texto)?;
        ed.origem = Some(caminho.to_string());
        Ok(ed)
    }

    /// A new folder for a mission with id `id`, not used by any file of the package.
    fn pasta_nova(&self, id: &str) -> String {
        let base = slug(&id.replace('/', "_"));
        let usadas: BTreeSet<&str> = self
            .arquivos
            .keys()
            .filter_map(|k| k.split_once('/').map(|(d, _)| d))
            .collect();
        let mut d = base.clone();
        let mut n = 2;
        while usadas.contains(d.as_str()) {
            d = format!("{base}_{n}");
            n += 1;
        }
        d
    }

    /// The files a mission refers to (its model, statement and reference model), resolved.
    fn referencias(&self, caminho: &str) -> Vec<String> {
        let Ok(m) = self.texto(caminho).and_then(|t| Missao::parse(&t)) else {
            return Vec::new();
        };
        let dir = pasta(caminho);
        let mut v = vec![junta(dir, &m.modelo), junta(dir, &m.texto)];
        if let Some(md) = &m.modelagem {
            v.push(junta(dir, &md.gabarito));
        }
        v
    }

    /// Writes the mission. A mission opened from this package (`ed.origem`) goes back to
    /// its place, with its files where they were, even when its id changed. A new one goes
    /// to a folder of its own at the end of the act `ato` (a new act when `ato` is past the
    /// last one). An id already used by another mission is refused. Checks the fields
    /// first; the whole package is checked by [`Self::conferir`]. Sets `ed.origem`.
    pub fn gravar(&mut self, ed: &mut Edicao, ato: usize) -> Result<String, Vec<String>> {
        let mut erros = ed.erros();
        let g = self.glossario();
        for c in &ed.base.conceitos {
            if !g.termos.iter().any(|t| &t.id == c) {
                erros.push(format!(
                    "conceito '{c}' não está no glossário do pacote nem no da campanha"
                ));
            }
        }
        if !erros.is_empty() {
            return Err(erros);
        }
        let lista = self.missoes();
        let origem = ed
            .origem
            .clone()
            .filter(|c| lista.iter().any(|it| &it.caminho == c));
        if lista
            .iter()
            .any(|it| it.id == ed.base.id && Some(&it.caminho) != origem.as_ref())
        {
            return Err(vec![format!(
                "já existe outra missão com o id '{}' no pacote: troque o id",
                ed.base.id
            )]);
        }
        let mut m = ed.missao();
        let caminho = match &origem {
            Some(c) => c.clone(),
            None => {
                m.modelo = "modelo.ron".into();
                m.texto = "texto.md".into();
                format!("{}/missao.ron", self.pasta_nova(&m.id))
            }
        };
        let dir = pasta(&caminho).to_string();
        self.arquivos
            .insert(caminho.clone(), ron_missao(&m).into_bytes());
        self.arquivos
            .insert(junta(&dir, &m.modelo), ed.modelo_ron.as_bytes().to_vec());
        self.arquivos
            .insert(junta(&dir, &m.texto), ed.texto.as_bytes().to_vec());
        if origem.is_none() {
            if ato >= self.pacote.atos.len() {
                let n = self.pacote.atos.len() as u32 + 1;
                self.pacote.atos.push(Ato {
                    numero: n,
                    titulo: format!("Ato {n}"),
                    missoes: Vec::new(),
                });
            }
            let i = ato.min(self.pacote.atos.len() - 1);
            self.pacote.atos[i].missoes.push(caminho.clone());
            ed.base.modelo = m.modelo;
            ed.base.texto = m.texto;
        }
        ed.origem = Some(caminho.clone());
        self.sincronizar();
        Ok(caminho)
    }

    /// Removes the mission at `caminho`: its file, and the files it refers to that no
    /// other mission uses. Anything else of the package stays.
    pub fn remover(&mut self, caminho: &str) {
        let seus = self.referencias(caminho);
        for a in &mut self.pacote.atos {
            a.missoes.retain(|c| c != caminho);
        }
        let outros: BTreeSet<String> = self
            .missoes()
            .iter()
            .flat_map(|it| self.referencias(&it.caminho))
            .collect();
        self.arquivos.remove(caminho);
        for f in seus.iter().filter(|f| !outros.contains(*f)) {
            self.arquivos.remove(f);
        }
        self.sincronizar();
    }

    /// Moves the mission one place up (`-1`) or down (`1`) in the order of the package,
    /// across acts at their ends.
    pub fn mover(&mut self, caminho: &str, passo: i32) {
        let lista = self.missoes();
        let Some(i) = lista.iter().position(|it| it.caminho == caminho) else {
            return;
        };
        let it = &lista[i];
        let n_atos = self.pacote.atos.len();
        let ato = &mut self.pacote.atos[it.ato];
        let j = ato.missoes.iter().position(|c| c == caminho).unwrap();
        if passo < 0 {
            if j > 0 {
                ato.missoes.swap(j, j - 1);
            } else if it.ato > 0 {
                let c = ato.missoes.remove(0);
                self.pacote.atos[it.ato - 1].missoes.push(c);
            }
        } else if j + 1 < ato.missoes.len() {
            ato.missoes.swap(j, j + 1);
        } else if it.ato + 1 < n_atos {
            let c = ato.missoes.remove(j);
            self.pacote.atos[it.ato + 1].missoes.insert(0, c);
        }
        self.sincronizar();
    }

    /// Writes `pacote.ron` from the manifest (empty acts other than the first go).
    fn sincronizar(&mut self) {
        let primeiro = self.pacote.atos.first().cloned();
        self.pacote.atos.retain(|a| !a.missoes.is_empty());
        if self.pacote.atos.is_empty()
            && let Some(a) = primeiro
        {
            self.pacote.atos.push(a);
        }
        for (i, a) in self.pacote.atos.iter_mut().enumerate() {
            a.numero = i as u32 + 1;
        }
        self.sincronizar_glossario();
        let cfg = ron::ser::PrettyConfig::new().struct_names(true);
        let txt = ron::ser::to_string_pretty(&self.pacote, cfg).expect("package serializes");
        self.arquivos.insert("pacote.ron".into(), txt.into_bytes());
    }

    /// The glossary the package carries (empty when it has none). An error when the file
    /// is missing or does not parse: then it is never rewritten.
    fn glossario_proprio(&self) -> Result<Glossario, String> {
        match &self.pacote.glossario {
            None => Ok(Glossario::default()),
            Some(g) => self.texto(g).and_then(|t| {
                crate::pacote::ron_options()
                    .from_str::<Glossario>(&t)
                    .map_err(|e| format!("{g}: erro de sintaxe: {e}"))
            }),
        }
    }

    /// The concepts a mission of this package can name: the terms of its own glossary,
    /// then those of the campaign it does not have.
    pub fn glossario(&self) -> Glossario {
        let mut g = self.glossario_proprio().unwrap_or_default();
        for t in glossario_da_campanha().termos {
            if !g.termos.iter().any(|x| x.id == t.id) {
                g.termos.push(t);
            }
        }
        g
    }

    /// Writes the glossary of the package when a mission names concepts: its own terms,
    /// plus the terms of the campaign that the missions name and it does not have. A
    /// glossary that does not parse is left as it is (the check of the package reports
    /// it). A package without a glossary gets one only when every concept named is known,
    /// so a package that loaded before still loads.
    fn sincronizar_glossario(&mut self) {
        let usados: BTreeSet<String> = self
            .missoes()
            .iter()
            .filter_map(|it| self.texto(&it.caminho).ok())
            .filter_map(|t| Missao::parse(&t).ok())
            .flat_map(|m| m.conceitos)
            .collect();
        if usados.is_empty() && self.pacote.glossario.is_none() {
            return;
        }
        let Ok(mut g) = self.glossario_proprio() else {
            return;
        };
        let campanha = glossario_da_campanha();
        if self.pacote.glossario.is_none()
            && usados
                .iter()
                .any(|c| !campanha.termos.iter().any(|t| &t.id == c))
        {
            return;
        }
        let mut mudou = self.pacote.glossario.is_none();
        for c in &usados {
            if !g.termos.iter().any(|t| &t.id == c)
                && let Some(t) = campanha.termos.iter().find(|t| &t.id == c)
            {
                g.termos.push(t.clone());
                mudou = true;
            }
        }
        if !mudou {
            return;
        }
        let path = self.pacote.glossario.clone().unwrap_or_else(|| {
            (1..)
                .map(|n| {
                    if n == 1 {
                        "glossario.ron".to_string()
                    } else {
                        format!("glossario_{n}.ron")
                    }
                })
                .find(|p| !self.arquivos.contains_key(p))
                .expect("some name is free")
        });
        let cfg = ron::ser::PrettyConfig::new().struct_names(true);
        let txt = ron::ser::to_string_pretty(&g, cfg).expect("glossary serializes");
        self.arquivos.insert(path.clone(), txt.into_bytes());
        self.pacote.glossario = Some(path);
    }

    /// The files with `pacote.ron` up to date.
    pub fn todos(&self) -> Arquivos {
        let mut p = self.clone();
        p.sincronizar();
        p.arquivos
    }

    /// Loads and checks the whole package (the same check as the CI and the student's app).
    pub fn conferir(&self) -> Result<PacoteCarregado, Vec<String>> {
        if self.missoes().is_empty() {
            return Err(vec!["o pacote ainda não tem missões".into()]);
        }
        carregar(&self.todos())
    }
}
