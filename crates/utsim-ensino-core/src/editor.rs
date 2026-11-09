//! The mission editor of the teacher (`docs/plano_ensino_e_jogo.md`, 6.2): the state of a
//! mission being edited over a UTSim model, the calibration that finds the reference and
//! suggests the goals, and the teacher's own package, where missions are written, opened
//! again, moved and removed. Everything here is data and rules; the screen is in
//! `utsim-ensino-ui`.
//!
//! A mission keeps every field it had when opened again: the editor changes only the
//! title, the levers, the measures, the goals and the statement, and the rest of the
//! [`Missao`] (questions, hints, rounds, data) goes back as it was.

use std::collections::BTreeSet;

use utsim_model::Model;

use crate::conferir::{Conferencia, conferir};
use crate::construtor::{rascunho, sugerir_alavancas, sugerir_metricas, texto_inicial};
use crate::missao::{Alavanca, Decisoes, Metrica, Missao, Objetivo, Tolerancia};
use crate::nota::Intervalo;
use crate::pacote::{Arquivos, Ato, Pacote, PacoteCarregado, carregar};

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
        })
    }

    /// The mission as it will be written: the base with the levers and measures in use.
    /// A reference that names a lever no longer used is dropped (calibrate again).
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
        let antigas: BTreeSet<&str> = m.referencia.keys().map(String::as_str).collect();
        if antigas != ids {
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
        e
    }

    /// The number of combinations of the levers in use (what the calibration simulates).
    pub fn combinacoes(&self) -> u64 {
        self.alavancas
            .iter()
            .filter(|a| a.usar)
            .map(|a| a.alavanca.valores().len().max(1) as u64)
            .fold(1u64, |acc, n| acc.saturating_mul(n))
    }

    /// The files of the mission under `dir` of a package.
    pub fn arquivos(&self, dir: &str) -> Arquivos {
        let mut m = self.missao();
        m.modelo = "modelo.ron".into();
        m.texto = "texto.md".into();
        let mut a = Arquivos::new();
        a.insert(format!("{dir}/missao.ron"), ron_missao(&m).into_bytes());
        a.insert(
            format!("{dir}/modelo.ron"),
            self.modelo_ron.as_bytes().to_vec(),
        );
        a.insert(format!("{dir}/texto.md"), self.texto.as_bytes().to_vec());
        a
    }

    /// The mission alone in a package, loaded and checked like any package.
    pub fn carregar(&self) -> Result<PacoteCarregado, Vec<String>> {
        let erros = self.erros();
        if !erros.is_empty() {
            return Err(erros);
        }
        let mut p = PacoteProfessor::novo("rascunho", "Rascunho", "");
        p.gravar(self, 0)?;
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
            let meta = match mt.objetivo {
                Objetivo::Menor => r * 1.05,
                Objetivo::Maior => r * 0.95,
            };
            let f = 10f64.powi(mt.casas as i32);
            metas.push((mt.nome.clone(), (meta * f).round() / f));
        }
    }
    Ok(Calibragem {
        conferencia: c,
        base,
        referencia,
        metas,
    })
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
        let dir = caminho.rsplit_once('/').map_or("", |(d, _)| d);
        let junta = |p: &str| {
            if dir.is_empty() {
                p.to_string()
            } else {
                format!("{dir}/{p}")
            }
        };
        if missao.modelagem.is_some() || missao.tem_pecas() || !missao.dados.is_empty() {
            return Err(
                "o editor ainda não abre missões de modelagem, de peças ou de dados".into(),
            );
        }
        let modelo = self.texto(&junta(&missao.modelo))?;
        let texto = self.texto(&junta(&missao.texto))?;
        Edicao::abrir(missao, &modelo, &texto)
    }

    /// The folder of a mission: where it already is (same id) or a new one from its id.
    fn pasta_de(&self, id: &str) -> String {
        if let Some(it) = self.missoes().into_iter().find(|it| it.id == id) {
            return it
                .caminho
                .rsplit_once('/')
                .map_or(String::new(), |(d, _)| d.to_string());
        }
        let base = slug(&id.replace('/', "_"));
        let usadas: BTreeSet<String> = self
            .arquivos
            .keys()
            .filter_map(|k| k.split_once('/').map(|(d, _)| d.to_string()))
            .collect();
        let mut d = base.clone();
        let mut n = 2;
        while usadas.contains(&d) {
            d = format!("{base}_{n}");
            n += 1;
        }
        d
    }

    /// Writes the mission: in its place when the id is already in the package, or at the
    /// end of the act `ato` (a new act when `ato` is past the last one). Checks the fields
    /// first; the whole package is checked by [`Self::conferir`].
    pub fn gravar(&mut self, ed: &Edicao, ato: usize) -> Result<String, Vec<String>> {
        let erros = ed.erros();
        if !erros.is_empty() {
            return Err(erros);
        }
        let dir = self.pasta_de(&ed.base.id);
        let caminho = format!("{dir}/missao.ron");
        // The old statement and model of the mission go: the folder is rewritten.
        let ja = self.missoes().iter().any(|it| it.caminho == caminho);
        if ja {
            let antigos: Vec<String> = self
                .arquivos
                .keys()
                .filter(|k| k.starts_with(&format!("{dir}/")))
                .cloned()
                .collect();
            for k in antigos {
                self.arquivos.remove(&k);
            }
        }
        self.arquivos.extend(ed.arquivos(&dir));
        if !ja {
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
        }
        self.sincronizar();
        Ok(caminho)
    }

    /// Removes the mission at `caminho` and its files.
    pub fn remover(&mut self, caminho: &str) {
        for a in &mut self.pacote.atos {
            a.missoes.retain(|c| c != caminho);
        }
        if let Some((dir, _)) = caminho.rsplit_once('/') {
            let pre = format!("{dir}/");
            self.arquivos.retain(|k, _| !k.starts_with(&pre));
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
        let cfg = ron::ser::PrettyConfig::new().struct_names(true);
        let txt = ron::ser::to_string_pretty(&self.pacote, cfg).expect("package serializes");
        self.arquivos.insert("pacote.ron".into(), txt.into_bytes());
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
