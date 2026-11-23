//! The package (`pacote.ron`, `formato: 1`): missions, models, statements and glossary,
//! read from a set of files (a folder, a `.utse` zip or the campaign built into the app).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utsim_model::Model;

use crate::missao::Missao;
use crate::validar::validar_missao;

/// Package format read by this version.
pub const FORMATO_PACOTE: u32 = 1;

/// The files of a package: path with `/` to bytes.
pub type Arquivos = BTreeMap<String, Vec<u8>>;

fn pt_br() -> String {
    "pt-BR".into()
}

fn um() -> u32 {
    1
}

/// The manifest.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pacote {
    pub id: String,
    #[serde(default = "um")]
    pub formato: u32,
    pub titulo: String,
    #[serde(default)]
    pub versao: String,
    #[serde(default)]
    pub autor: String,
    #[serde(default)]
    pub descricao: String,
    /// Language of the texts of the package (`pt-BR`, `en`, `es`); the interface says so
    /// when it differs from the chosen one.
    #[serde(default = "pt_br")]
    pub idioma: String,
    /// What the package needs (`nucleo >= 0.1`, `linha:lean_office`), see [`crate::requisitos`].
    #[serde(default)]
    pub requer: Vec<String>,
    pub atos: Vec<Ato>,
    #[serde(default)]
    pub pesos: Pesos,
    #[serde(default)]
    pub registro: Registro,
    /// Glossary path inside the package.
    #[serde(default)]
    pub glossario: Option<String>,
    /// Concept of the glossary to tools of the UTSim LSS curriculum (`msa`,
    /// `capacidade`...): a solved mission records `praticou` in those tools in the
    /// company program ([`crate::competencias`]).
    #[serde(default)]
    pub competencias: std::collections::BTreeMap<String, Vec<String>>,
    /// Exam rules: the package is an exam ([`crate::prova`]), without answer key.
    #[serde(default)]
    pub prova: Option<crate::prova::Prova>,
}

/// An act: a sequence of missions, each one unlocking the next.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Ato {
    pub numero: u32,
    pub titulo: String,
    /// Mission file paths inside the package.
    pub missoes: Vec<String>,
}

/// Weights of the grade parts and the discount of each hint level.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pesos {
    pub desempenho: f64,
    pub previsao: f64,
    pub justificativa: f64,
    /// Points lost when the highest hint level used is 1, 2 or 3.
    pub dicas: [f64; 3],
}

impl Default for Pesos {
    fn default() -> Self {
        Pesos {
            desempenho: 70.0,
            previsao: 20.0,
            justificativa: 10.0,
            dicas: [0.0, 3.0, 8.0],
        }
    }
}

/// Which grade of a mission counts: the best attempt or the last one.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Registro {
    #[default]
    Melhor,
    Ultima,
}

/// The glossary (`glossario.ron`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Glossario {
    pub termos: Vec<TermoGlossario>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TermoGlossario {
    pub id: String,
    pub nome: String,
    pub texto: String,
    /// Manual chapter (`conceitos.md`), when there is one.
    #[serde(default)]
    pub manual: Option<String>,
}

/// A mission ready to run: its model already parsed.
#[derive(Clone, Debug)]
pub struct MissaoCarregada {
    pub missao: Missao,
    pub modelo: Model,
    pub texto: String,
    /// The drawn statement, when the mission has one.
    pub cena: Option<CenaCarregada>,
    /// Act number.
    pub ato: u32,
    /// The reference model of a modelling mission.
    pub gabarito: Option<Model>,
}

/// The scene of a mission with its picture (PNG) and anchors, read from the package.
#[derive(Clone)]
pub struct CenaCarregada {
    pub cena: crate::cena::Cena,
    pub png: std::sync::Arc<[u8]>,
    pub ancoras: crate::cena::Ancoras,
    /// In a layout: each machine on a transparent picture of the same size, drawn at the
    /// reference cell (`<picture>.<machine>.png`).
    pub maquinas: std::collections::BTreeMap<String, std::sync::Arc<[u8]>>,
}

impl std::fmt::Debug for CenaCarregada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CenaCarregada")
            .field("imagem", &self.cena.imagem)
            .field("bytes", &self.png.len())
            .field("ancoras", &self.ancoras.nos.len())
            .finish()
    }
}

/// The width and height of a PNG, from its header.
fn tamanho_png(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || b[..8] != [0x89, b'P', b'N', b'G', 13, 10, 26, 10] || &b[12..16] != b"IHDR" {
        return None;
    }
    let n = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    Some((n(16), n(20)))
}

/// Reads the picture and the anchors of a scene and checks that every card has its anchor.
fn carregar_cena(
    arquivos: &Arquivos,
    missao: &Missao,
    cena: &crate::cena::Cena,
    junta: &dyn Fn(&str) -> String,
) -> Result<CenaCarregada, Vec<String>> {
    let img = junta(&cena.imagem);
    let png = arquivos
        .get(&img)
        .ok_or_else(|| vec![format!("cena: a imagem '{img}' não está no pacote")])?;
    let (w, h) = tamanho_png(png).ok_or_else(|| vec![format!("cena: '{img}' não é PNG")])?;
    let anc = junta(&cena.caminho_ancoras());
    let ancoras = crate::cena::Ancoras::parse(
        arquivos
            .get(&anc)
            .ok_or_else(|| vec![format!("cena: as âncoras '{anc}' não estão no pacote")])?,
    )
    .map_err(|e| vec![format!("{anc}: {e}")])?;
    let mut erros = Vec::new();
    if (ancoras.largura, ancoras.altura) != (w as f64, h as f64) {
        erros.push(format!(
            "cena: as âncoras são de {}×{}, a imagem tem {w}×{h}: gere as duas de novo",
            ancoras.largura, ancoras.altura
        ));
    }
    let inicial = missao.decisoes_iniciais();
    for c in &cena.cartoes {
        let no = crate::cena::Cena::ancora(missao, c, &inicial);
        if !ancoras.nos.contains_key(&no) {
            erros.push(format!(
                "cena: o cartão de '{}' não tem âncora na imagem",
                c.no
            ));
        }
    }
    let mut maquinas = std::collections::BTreeMap::new();
    if let Some(a) = &missao.arranjo {
        for c in 0..a.celulas() {
            if !ancoras.nos.contains_key(&crate::cena::ancora_celula(c)) {
                erros.push(format!("cena: a célula {c} não tem âncora na imagem"));
            }
        }
        for m in &a.maquinas {
            let p = junta(&cena.imagem_maquina(&m.id));
            match arquivos.get(&p) {
                Some(b) if tamanho_png(b) == Some((w, h)) => {
                    maquinas.insert(m.id.clone(), b.as_slice().into());
                }
                Some(_) => erros.push(format!("cena: '{p}' não tem o tamanho da imagem")),
                None => erros.push(format!("cena: a imagem '{p}' não está no pacote")),
            }
        }
    }
    if !erros.is_empty() {
        return Err(erros);
    }
    Ok(CenaCarregada {
        cena: cena.clone(),
        png: png.as_slice().into(),
        ancoras,
        maquinas,
    })
}

/// `p` relative to the folder `dir` of a package file (absolute when it starts with `/`),
/// with `..` resolved.
pub fn junta(dir: &str, p: &str) -> String {
    if dir.is_empty() || p.starts_with('/') {
        p.trim_start_matches('/').to_string()
    } else {
        normalizar(&format!("{dir}/{p}"))
    }
}

/// A package read and validated.
#[derive(Clone, Debug)]
pub struct PacoteCarregado {
    pub pacote: Pacote,
    pub missoes: Vec<MissaoCarregada>,
    pub glossario: Glossario,
    /// SHA-256 of the files, in hex: goes in every delivery.
    pub hash: String,
    /// Needs this version does not meet (a warning, not an error).
    pub faltando: Vec<crate::requisitos::Falta>,
}

impl PacoteCarregado {
    pub fn missao(&self, id: &str) -> Option<&MissaoCarregada> {
        self.missoes.iter().find(|m| m.missao.id == id)
    }
    pub fn indice(&self, id: &str) -> Option<usize> {
        self.missoes.iter().position(|m| m.missao.id == id)
    }
}

fn texto(arquivos: &Arquivos, path: &str) -> Result<String, String> {
    let bytes = arquivos
        .get(path)
        .ok_or_else(|| format!("arquivo '{path}' não está no pacote"))?;
    String::from_utf8(bytes.clone()).map_err(|_| format!("'{path}' não é texto UTF-8"))
}

pub(crate) fn ron_options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

/// SHA-256 of a set of files, path and content in order.
pub fn hash_arquivos(arquivos: &Arquivos) -> String {
    let mut h = Sha256::new();
    for (k, v) in arquivos {
        h.update((k.len() as u64).to_le_bytes());
        h.update(k.as_bytes());
        h.update((v.len() as u64).to_le_bytes());
        h.update(v);
    }
    hex(&h.finalize())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads and validates a package. Every problem is reported, each with the file it is in.
/// Only the manifest (`pacote.ron`): title and description for a list of packages, with
/// no mission run.
pub fn manifesto(arquivos: &Arquivos) -> Result<Pacote, String> {
    let manifesto = texto(arquivos, "pacote.ron")?;
    ron_options()
        .from_str(&manifesto)
        .map_err(|e| format!("pacote.ron: erro de sintaxe: {e}"))
}

pub fn carregar(arquivos: &Arquivos) -> Result<PacoteCarregado, Vec<String>> {
    let pacote = manifesto(arquivos).map_err(|e| vec![e])?;
    if pacote.formato > FORMATO_PACOTE {
        return Err(vec![format!(
            "pacote.ron: formato {}; esta versão lê até o formato {FORMATO_PACOTE}",
            pacote.formato
        )]);
    }
    let mut erros = Vec::new();
    let mut missoes = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for ato in &pacote.atos {
        for path in &ato.missoes {
            match carregar_missao(arquivos, path, ato.numero) {
                Ok(m) => {
                    if !ids.insert(m.missao.id.clone()) {
                        erros.push(format!("{path}: id '{}' repetido", m.missao.id));
                    }
                    missoes.push(m);
                }
                Err(es) => erros.extend(es.into_iter().map(|e| format!("{path}: {e}"))),
            }
        }
    }
    let glossario = match &pacote.glossario {
        Some(path) => match texto(arquivos, path).and_then(|t| {
            ron_options()
                .from_str::<Glossario>(&t)
                .map_err(|e| format!("{path}: erro de sintaxe: {e}"))
        }) {
            Ok(g) => g,
            Err(e) => {
                erros.push(e);
                Glossario::default()
            }
        },
        None => Glossario::default(),
    };
    for m in &missoes {
        for c in &m.missao.conceitos {
            if pacote.glossario.is_some() && !glossario.termos.iter().any(|t| &t.id == c) {
                erros.push(format!(
                    "{}: conceito '{c}' não está no glossário",
                    m.missao.id
                ));
            }
        }
    }
    for (c, ferramentas) in &pacote.competencias {
        if !glossario.termos.iter().any(|t| &t.id == c) {
            erros.push(format!(
                "competencias: conceito '{c}' não está no glossário"
            ));
        }
        if ferramentas.iter().any(|f| f.trim().is_empty()) {
            erros.push(format!("competencias: ferramenta vazia em '{c}'"));
        }
    }
    if !erros.is_empty() {
        return Err(erros);
    }
    let faltando = crate::requisitos::faltando(&pacote.requer);
    Ok(PacoteCarregado {
        hash: hash_arquivos(arquivos),
        pacote,
        missoes,
        glossario,
        faltando,
    })
}

fn carregar_missao(
    arquivos: &Arquivos,
    path: &str,
    ato: u32,
) -> Result<MissaoCarregada, Vec<String>> {
    let missao =
        Missao::parse(&texto(arquivos, path).map_err(|e| vec![e])?).map_err(|e| vec![e])?;
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    let junta = |p: &str| junta(dir, p);
    let modelo_txt = texto(arquivos, &junta(&missao.modelo)).map_err(|e| vec![e])?;
    let modelo =
        Model::parse(&modelo_txt).map_err(|e| vec![format!("modelo '{}': {e}", missao.modelo)])?;
    let texto_md = texto(arquivos, &junta(&missao.texto)).map_err(|e| vec![e])?;
    let gabarito = match &missao.modelagem {
        Some(md) => {
            let txt = texto(arquivos, &junta(&md.gabarito)).map_err(|e| vec![e])?;
            Some(Model::parse(&txt).map_err(|e| vec![format!("gabarito '{}': {e}", md.gabarito)])?)
        }
        None => None,
    };
    // A modelling mission asks about the reference model (the starting one is incomplete).
    validar_missao(&missao, gabarito.as_ref().unwrap_or(&modelo))?;
    let cena = match &missao.cena {
        Some(c) => Some(carregar_cena(arquivos, &missao, c, &junta)?),
        None => None,
    };
    Ok(MissaoCarregada {
        missao,
        modelo,
        texto: texto_md,
        cena,
        ato,
        gabarito,
    })
}

/// Resolves `a/b/../c` to `a/c`.
fn normalizar(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            x => out.push(x),
        }
    }
    out.join("/")
}
