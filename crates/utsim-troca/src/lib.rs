//! Pedido e resposta entre os apps da família (seção 2 de `docs/plano_integracao_familia.md`).
//! Sem servidor, sem rede e sem interface: só arquivos JSON e linha de comando, para que
//! os apps de janela, o 3D e o Ensino usem o mesmo formato.
//!
//! - Quem chama monta um [`Pedido`], grava com [`Pedido::gravar`] numa pasta de troca e abre
//!   o outro app com [`Pedido::argumentos`] (`--utsim-pedido=<caminho>`). Enquanto o outro app
//!   não lê pedidos, [`Pedido::argumentos_diretos`] dá os argumentos diretos que ele já
//!   entende (`--utsim-abrir=`, `--utsim-ferramenta=`...), os mesmos de
//!   `utsim_posto_ui::abrir::Abrir`.
//! - Quem atende acha o pedido com [`caminho_do_pedido`], lê com [`Pedido::ler`], confere os
//!   arquivos de entrada ([`Pedido::conferir_entrada`]) e, ao concluir, grava a
//!   [`Resposta`] no caminho pedido ([`Resposta::gravar`]).
//! - Quem chamou vigia a resposta com [`Resposta::ler`], que confere o `id` e os hashes.
//!
//! As chaves das ferramentas são as de `utsim_estudo::importar::chave` (`carta`,
//! `capacidade`, `msa`, `pareto`, `histograma`, `dispersao`, `estratificacao`, `folha`,
//! `fmea` e as outras do estudo). O resultado de uma ferramenta vai no formato de troca dela
//! ([`Resultado`]: `{ "ferramenta", "formato", "dados" }`).
//!
//! As datas (`quando`) vêm de quem chama, nunca do relógio do crate, como no `utsim-diario`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Versão do formato do pedido e da resposta. Um arquivo de formato maior é recusado.
pub const FORMATO: u32 = 1;

/// A opção de linha de comando que leva o caminho do pedido.
pub const OPCAO_PEDIDO: &str = "--utsim-pedido";

/// Fim do nome dos arquivos de pedido e de resposta.
pub const SUFIXO_PEDIDO: &str = ".utsim-pedido.json";
pub const SUFIXO_RESPOSTA: &str = ".utsim-resposta.json";

/// As ações conhecidas (o campo `acao` é texto, para um app antigo recusar com clareza uma
/// ação nova em vez de falhar ao ler o arquivo).
pub mod acao {
    pub const ABRIR_FERRAMENTA: &str = "abrir_ferramenta";
    pub const EDITAR_NO: &str = "editar_no";
    pub const ABRIR_MODELO: &str = "abrir_modelo";
    pub const RODAR_MISSAO: &str = "rodar_missao";
    pub const ANALISAR: &str = "analisar";
    pub const PLANEJAR_EXPERIMENTO: &str = "planejar_experimento";
    pub const AJUSTAR_DISTRIBUICAO: &str = "ajustar_distribuicao";
}

#[derive(Debug, thiserror::Error)]
pub enum Erro {
    #[error(transparent)]
    Pasta(#[from] utsim_pasta::Error),
    #[error("{}: {}", .0.display(), .1)]
    Io(PathBuf, std::io::Error),
    #[error("{}: JSON inválido: {}", .0.display(), .1)]
    Json(PathBuf, serde_json::Error),
    #[error("{}: formato {} é mais novo que este app (lê até {FORMATO}); atualize pelo launcher", .0.display(), .1)]
    FormatoNovo(PathBuf, u32),
    #[error("ferramenta desconhecida: {0}")]
    Ferramenta(String),
    #[error("{}: o arquivo mudou depois do pedido (hash diferente)", .0.display())]
    Hash(PathBuf),
    #[error("{}: a resposta tem de terminar em {SUFIXO_RESPOSTA}", .0.display())]
    NomeResposta(PathBuf),
    #[error("a resposta é de outro pedido ({achado}, esperado {esperado})")]
    OutroPedido { achado: String, esperado: String },
}

/// Um arquivo com o SHA-256 (hex) dos bytes, para quem recebe conferir que é o mesmo.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Arquivo {
    pub caminho: PathBuf,
    pub sha256: String,
}

impl Arquivo {
    /// Lê o arquivo e guarda o hash.
    /// O caminho fica absoluto: o outro app roda em outra pasta.
    pub fn de(caminho: impl Into<PathBuf>) -> Result<Arquivo, Erro> {
        let caminho = absoluto(&caminho.into());
        let bytes = std::fs::read(&caminho).map_err(|e| Erro::Io(caminho.clone(), e))?;
        Ok(Arquivo {
            sha256: sha256_hex(&bytes),
            caminho,
        })
    }

    /// O arquivo ainda tem o hash guardado (um hash vazio também é recusado).
    pub fn conferir(&self) -> Result<(), Erro> {
        let bytes = std::fs::read(&self.caminho).map_err(|e| Erro::Io(self.caminho.clone(), e))?;
        if sha256_hex(&bytes).eq_ignore_ascii_case(&self.sha256) {
            Ok(())
        } else {
            Err(Erro::Hash(self.caminho.clone()))
        }
    }
}

/// Os dados de partida.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entrada {
    /// Arquivos de dados (CSV, xlsx) com hash.
    pub arquivos: Vec<Arquivo>,
    /// Conteúdo pequeno no próprio pedido (um CSV curto), no lugar de um arquivo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conteudo: Option<String>,
    /// A aba do xlsx.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aba: Option<String>,
    /// As colunas escolhidas por quem chama, pelo nome do cabeçalho.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub colunas: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lie: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lse: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subgrupo: Option<usize>,
    /// Os fatores de `planejar_experimento`: o campo do modelo e os níveis.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fatores: Vec<Fator>,
    /// Os indicadores que respondem ao experimento (`planejar_experimento`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub respostas: Vec<String>,
    /// O orçamento de corridas do plano (sem as replicações).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corridas: Option<usize>,
    /// As replicações de cada corrida no modelo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicacoes: Option<u32>,
}

/// Um fator de `planejar_experimento`: um campo do modelo variando entre dois níveis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fator {
    /// O nome na planilha (vazio: a chave do campo).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub nome: String,
    /// O campo do modelo, `objeto.campo` (como em `utsim_model::experiment::patch`).
    pub campo: String,
    /// O valor em RON com `{x}` no lugar do nível (vazio: `{x}`), para campos como
    /// `Exp(mean: {x})`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub valor: String,
    pub baixo: f64,
    pub alto: f64,
    /// O nível vai arredondado (campos inteiros, como `capacity`).
    pub inteiro: bool,
}

/// Onde o resultado vai morar.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contexto {
    /// Projeto `.lss` do Seis Sigma.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projeto: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entregavel: Option<String>,
    /// Modelo `.ron` e nó.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modelo: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no: Option<String>,
    /// Pasta de unidade e posto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unidade: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posto: Option<String>,
}

/// O pedido (`<id>.utsim-pedido.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pedido {
    pub formato: u32,
    pub id: String,
    /// Produto e versão de quem chama (`estatistica 0.2.3`).
    pub de: String,
    /// Uma das [`acao`].
    pub acao: String,
    /// A chave da ferramenta, quando a ação é sobre uma ferramenta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ferramenta: Option<String>,
    pub entrada: Entrada,
    pub contexto: Contexto,
    /// Onde gravar a resposta. Sem ela, não há resposta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resposta: Option<PathBuf>,
    pub somente_leitura: bool,
}

impl Pedido {
    /// Um pedido novo com `id` único.
    pub fn novo(de: &str, acao: &str) -> Pedido {
        Pedido {
            formato: FORMATO,
            id: novo_id(),
            de: de.to_string(),
            acao: acao.to_string(),
            ..Pedido::default()
        }
    }

    /// `abrir_ferramenta` com arquivos de dados (lidos agora para o hash). A chave aceita
    /// maiúsculas, acentos e espaços (`Estratificação`) e é guardada na forma canônica.
    pub fn abrir_ferramenta(
        de: &str,
        ferramenta: &str,
        arquivos: &[PathBuf],
    ) -> Result<Pedido, Erro> {
        let f = utsim_estudo::importar::de_chave(ferramenta)
            .ok_or_else(|| Erro::Ferramenta(ferramenta.to_string()))?;
        let mut p = Pedido::novo(de, acao::ABRIR_FERRAMENTA);
        p.ferramenta = Some(utsim_estudo::importar::chave(f).to_string());
        p.entrada.arquivos = arquivos
            .iter()
            .map(|c| Arquivo::de(c.clone()))
            .collect::<Result<_, _>>()?;
        Ok(p)
    }

    /// `planejar_experimento` sobre um modelo `.ron`: os fatores e as respostas.
    pub fn planejar_experimento(
        de: &str,
        modelo: impl Into<PathBuf>,
        fatores: Vec<Fator>,
        respostas: Vec<String>,
    ) -> Pedido {
        let mut p = Pedido::novo(de, acao::PLANEJAR_EXPERIMENTO);
        p.contexto.modelo = Some(modelo.into());
        p.entrada.fatores = fatores;
        p.entrada.respostas = respostas;
        p
    }

    /// A ferramenta do estudo, quando a chave é conhecida.
    pub fn ferramenta_estudo(&self) -> Option<utsim_estudo::Ferramenta> {
        self.ferramenta
            .as_deref()
            .and_then(utsim_estudo::importar::de_chave)
    }

    /// O nome do arquivo do pedido.
    pub fn nome_arquivo(&self) -> String {
        format!("{}{SUFIXO_PEDIDO}", seguro(&self.id))
    }

    /// Grava o pedido em `pasta` (criada se faltar), de uma vez. Sem caminho de resposta,
    /// ele fica ao lado do pedido. Devolve o caminho do pedido.
    pub fn gravar(&mut self, pasta: &Path) -> Result<PathBuf, Erro> {
        let pasta = &absoluto(pasta);
        std::fs::create_dir_all(pasta).map_err(|e| Erro::Io(pasta.to_path_buf(), e))?;
        if self.resposta.is_none() {
            self.resposta = Some(pasta.join(format!("{}{SUFIXO_RESPOSTA}", seguro(&self.id))));
        }
        let caminho = pasta.join(self.nome_arquivo());
        let texto =
            serde_json::to_string_pretty(self).map_err(|e| Erro::Json(caminho.clone(), e))?;
        utsim_pasta::write_replace(&caminho, texto.as_bytes())?;
        Ok(caminho)
    }

    /// Lê um pedido. Campos que faltam ficam no padrão; um formato mais novo é recusado.
    pub fn ler(caminho: &Path) -> Result<Pedido, Erro> {
        let p: Pedido = ler_json(caminho)?;
        if p.formato > FORMATO {
            return Err(Erro::FormatoNovo(caminho.to_path_buf(), p.formato));
        }
        Ok(p)
    }

    /// Confere o hash de cada arquivo de entrada.
    pub fn conferir_entrada(&self) -> Result<(), Erro> {
        self.entrada.arquivos.iter().try_for_each(Arquivo::conferir)
    }

    /// Os argumentos para abrir o outro app com este pedido (um argumento só, sem shell).
    pub fn argumentos(caminho_pedido: &Path) -> Vec<String> {
        vec![format!("{OPCAO_PEDIDO}={}", caminho_pedido.display())]
    }

    /// Os argumentos diretos equivalentes, para os apps que não leem pedidos (o Qualidade e
    /// o Segurança já leem `--utsim-pedido`; os diretos servem às versões instaladas antes,
    /// em `utsim_posto_ui::abrir::Abrir`). Não há resposta por
    /// esse caminho, e `entrada.conteudo` e `entrada.colunas` não passam: grave o conteúdo
    /// num arquivo antes. Os caminhos saem absolutos; LIE e LSE que não são números e
    /// subgrupo 0 ficam de fora.
    pub fn argumentos_diretos(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(m) = &self.contexto.modelo {
            v.push(format!("--utsim-abrir={}", absoluto(m).display()));
            if let Some(n) = &self.contexto.no {
                v.push(format!("--utsim-no={n}"));
            }
        }
        if self.acao == acao::ABRIR_FERRAMENTA {
            for a in &self.entrada.arquivos {
                v.push(format!("--utsim-abrir={}", absoluto(&a.caminho).display()));
            }
            if let Some(f) = &self.ferramenta {
                v.push(format!("--utsim-ferramenta={f}"));
            }
            let e = &self.entrada;
            if let Some(a) = &e.aba {
                v.push(format!("--utsim-aba={a}"));
            }
            if let Some(x) = e.lie.filter(|x| x.is_finite()) {
                v.push(format!("--utsim-lie={x}"));
            }
            if let Some(x) = e.lse.filter(|x| x.is_finite()) {
                v.push(format!("--utsim-lse={x}"));
            }
            if let Some(n) = e.subgrupo.filter(|&n| n >= 1) {
                v.push(format!("--utsim-subgrupo={n}"));
            }
        }
        v
    }
}

/// Como terminou o pedido.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Estado {
    #[default]
    Concluido,
    Cancelado,
    Erro,
    /// Um estado de um formato mais novo.
    #[serde(other)]
    Desconhecido,
}

/// O resultado de uma ferramenta no formato de troca dela (seção 2.4).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resultado {
    pub ferramenta: String,
    pub formato: u32,
    pub dados: serde_json::Value,
}

/// Quem atendeu.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Origem {
    pub app: String,
    pub versao: String,
    pub quem: String,
    /// Data e hora dadas por quem grava (ISO 8601).
    pub quando: String,
}

/// A resposta (`<id>.utsim-resposta.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resposta {
    pub formato: u32,
    /// O `id` do pedido.
    pub pedido: String,
    pub estado: Estado,
    /// Texto para a pessoa quando o estado é `erro` ou `cancelado`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mensagem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resultado: Option<Resultado>,
    /// Arquivos gerados (relatório HTML, PDF, CSV), com hash.
    pub arquivos: Vec<Arquivo>,
    pub origem: Origem,
}

impl Resposta {
    pub fn nova(pedido: &Pedido, estado: Estado, origem: Origem) -> Resposta {
        Resposta {
            formato: FORMATO,
            pedido: pedido.id.clone(),
            estado,
            origem,
            ..Resposta::default()
        }
    }

    /// Grava a resposta de uma vez no caminho que o pedido deu. O nome tem de terminar em
    /// `.utsim-resposta.json`, para um pedido não mandar gravar por cima de outro arquivo.
    pub fn gravar(&self, caminho: &Path) -> Result<(), Erro> {
        if !caminho
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(SUFIXO_RESPOSTA))
        {
            return Err(Erro::NomeResposta(caminho.to_path_buf()));
        }
        if let Some(d) = caminho.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(d).map_err(|e| Erro::Io(d.to_path_buf(), e))?;
        }
        let texto =
            serde_json::to_string_pretty(self).map_err(|e| Erro::Json(caminho.to_path_buf(), e))?;
        utsim_pasta::write_replace(caminho, texto.as_bytes())?;
        Ok(())
    }

    /// Lê a resposta do pedido: `None` enquanto ela não existe. Confere o `id` e o hash de
    /// cada arquivo gerado.
    pub fn ler(pedido: &Pedido) -> Result<Option<Resposta>, Erro> {
        let Some(caminho) = &pedido.resposta else {
            return Ok(None);
        };
        if !caminho.is_file() {
            return Ok(None);
        }
        let r: Resposta = ler_json(caminho)?;
        if r.formato > FORMATO {
            return Err(Erro::FormatoNovo(caminho.clone(), r.formato));
        }
        if r.pedido != pedido.id {
            return Err(Erro::OutroPedido {
                achado: r.pedido,
                esperado: pedido.id.clone(),
            });
        }
        r.arquivos.iter().try_for_each(Arquivo::conferir)?;
        Ok(Some(r))
    }
}

/// O caminho do pedido nos argumentos (sem o nome do programa): `--utsim-pedido=<c>` ou
/// `--utsim-pedido <c>`. Fica o último.
pub fn caminho_do_pedido<I: IntoIterator<Item = String>>(args: I) -> Option<PathBuf> {
    let mut achado = None;
    let mut it = args.into_iter().peekable();
    while let Some(a) = it.next() {
        if let Some(v) = a.strip_prefix(OPCAO_PEDIDO) {
            if let Some(v) = v.strip_prefix('=').filter(|v| !v.is_empty()) {
                achado = Some(PathBuf::from(v));
            } else if v.is_empty()
                && let Some(v) = it.next_if(|v| !v.starts_with("--"))
            {
                achado = Some(PathBuf::from(v));
            }
        }
    }
    achado
}

/// O caminho absoluto (sem tocar no disco); fica como está se não der.
fn absoluto(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// O programa `nome` (sem `.exe`) ao lado deste programa ou no `PATH`. A versão instalada
/// pelo launcher (`utsim_launcher_core::install::launch_command`) vem antes e fica com quem
/// chama, para este crate não depender do launcher.
pub fn localizar_programa(nome: &str) -> Option<PathBuf> {
    let arquivo = if cfg!(windows) {
        format!("{nome}.exe")
    } else {
        nome.to_string()
    };
    let ao_lado = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&arquivo)))
        .filter(|c| c.is_file());
    ao_lado.or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|d| d.join(&arquivo))
            .find(|c| c.is_file())
    })
}

/// SHA-256 em hex minúsculo.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Um identificador único neste computador: hora em nanossegundos, processo e contador.
pub fn novo_id() -> String {
    static CONTADOR: AtomicU64 = AtomicU64::new(0);
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = CONTADOR.fetch_add(1, Ordering::Relaxed);
    format!("{ns:x}-{:x}-{n:x}", std::process::id())
}

/// O `id` só com letras, números, `-` e `_`, para virar nome de arquivo.
fn seguro(id: &str) -> String {
    let s: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "pedido".into() } else { s }
}

fn ler_json<T: serde::de::DeserializeOwned>(caminho: &Path) -> Result<T, Erro> {
    let texto = utsim_pasta::read_text(caminho)?;
    serde_json::from_str(&texto).map_err(|e| Erro::Json(caminho.to_path_buf(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta_com_hash_e_resposta() {
        let d = tempfile::tempdir().unwrap();
        let dados = d.path().join("medidas.csv");
        std::fs::write(&dados, "x\n1\n2\n").unwrap();
        let mut p =
            Pedido::abrir_ferramenta("estatistica 0.2.3", "Carta", std::slice::from_ref(&dados))
                .unwrap();
        p.entrada.lie = Some(0.5);
        p.entrada.lse = Some(2.5);
        p.entrada.subgrupo = Some(5);
        assert_eq!(p.ferramenta.as_deref(), Some("carta"));
        assert_eq!(
            p.ferramenta_estudo(),
            Some(utsim_estudo::Ferramenta::CartaControle)
        );
        let troca = d.path().join("troca");
        let c = p.gravar(&troca).unwrap();
        assert!(
            c.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(SUFIXO_PEDIDO)
        );
        let lido = Pedido::ler(&c).unwrap();
        assert_eq!(lido, p);
        lido.conferir_entrada().unwrap();

        // O atendente grava a resposta; quem chamou lê.
        assert!(Resposta::ler(&p).unwrap().is_none());
        let rel = d.path().join("carta.html");
        std::fs::write(&rel, "<p>ok</p>").unwrap();
        let mut r = Resposta::nova(
            &lido,
            Estado::Concluido,
            Origem {
                app: "qualidade".into(),
                versao: "0.9.0".into(),
                quem: "Ana".into(),
                quando: "2026-11-10T10:00:00".into(),
            },
        );
        r.resultado = Some(Resultado {
            ferramenta: "carta".into(),
            formato: 1,
            dados: serde_json::json!({"media": 1.5}),
        });
        r.arquivos.push(Arquivo::de(&rel).unwrap());
        r.gravar(lido.resposta.as_ref().unwrap()).unwrap();
        let volta = Resposta::ler(&p).unwrap().unwrap();
        assert_eq!(volta, r);

        // Arquivo trocado depois do pedido: recusado.
        std::fs::write(&dados, "x\n9\n").unwrap();
        assert!(matches!(lido.conferir_entrada(), Err(Erro::Hash(_))));
        std::fs::write(&rel, "outro").unwrap();
        assert!(matches!(Resposta::ler(&p), Err(Erro::Hash(_))));
    }

    #[test]
    fn resposta_de_outro_pedido_e_formato_novo() {
        let d = tempfile::tempdir().unwrap();
        let mut a = Pedido::novo("x", acao::ANALISAR);
        let b = Pedido::novo("x", acao::ANALISAR);
        assert_ne!(a.id, b.id);
        a.gravar(d.path()).unwrap();
        Resposta::nova(&b, Estado::Cancelado, Origem::default())
            .gravar(a.resposta.as_ref().unwrap())
            .unwrap();
        assert!(matches!(Resposta::ler(&a), Err(Erro::OutroPedido { .. })));
        // Estado novo, nome errado, hash vazio.
        let r: Resposta = serde_json::from_str(r#"{"pedido": "x", "estado": "parcial"}"#).unwrap();
        assert_eq!(r.estado, Estado::Desconhecido);
        assert!(matches!(
            r.gravar(&d.path().join("algo.json")),
            Err(Erro::NomeResposta(_))
        ));
        let sem_hash = Arquivo {
            caminho: d.path().join("x"),
            sha256: String::new(),
        };
        std::fs::write(&sem_hash.caminho, "1").unwrap();
        assert!(sem_hash.conferir().is_err());
        let c = d.path().join("novo.utsim-pedido.json");
        std::fs::write(&c, r#"{"formato": 99, "acao": "x", "campo_novo": 1}"#).unwrap();
        assert!(matches!(Pedido::ler(&c), Err(Erro::FormatoNovo(_, 99))));
        std::fs::write(&c, r#"{"formato": 1, "acao": "editar_no"}"#).unwrap();
        assert_eq!(Pedido::ler(&c).unwrap().acao, acao::EDITAR_NO);
    }

    #[test]
    fn ferramenta_desconhecida() {
        assert!(matches!(
            Pedido::abrir_ferramenta("x", "planilha", &[]),
            Err(Erro::Ferramenta(_))
        ));
        assert_eq!(
            Pedido::abrir_ferramenta("x", "Estratificação", &[])
                .unwrap()
                .ferramenta
                .as_deref(),
            Some("estratificacao")
        );
    }

    #[test]
    fn argumentos() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            caminho_do_pedido(a(&["--utsim-pedido=C:/a b/p.json"])),
            Some(PathBuf::from("C:/a b/p.json"))
        );
        assert_eq!(
            caminho_do_pedido(a(&["x.ron", "--utsim-pedido", "p.json"])),
            Some(PathBuf::from("p.json"))
        );
        assert_eq!(caminho_do_pedido(a(&["--utsim-pedido", "--outra"])), None);
        assert_eq!(caminho_do_pedido(a(&["--utsim-pedidos=x"])), None);
        assert_eq!(
            caminho_do_pedido(a(&["--utsim-pedido=p.json", "--utsim-pedido="])),
            Some(PathBuf::from("p.json"))
        );
        assert_eq!(
            Pedido::argumentos(Path::new("p.json")),
            vec!["--utsim-pedido=p.json".to_string()]
        );

        let mut p = Pedido::novo("x", acao::ABRIR_FERRAMENTA);
        p.ferramenta = Some("capacidade".into());
        let d = std::env::current_dir().unwrap().join("d.csv");
        p.entrada.arquivos.push(Arquivo {
            caminho: d.clone(),
            sha256: String::new(),
        });
        p.entrada.aba = Some("Dados".into());
        p.entrada.lie = Some(9.5);
        p.entrada.lse = Some(10.5);
        p.entrada.subgrupo = Some(4);
        assert_eq!(
            p.argumentos_diretos(),
            a(&[
                format!("--utsim-abrir={}", d.display()).as_str(),
                "--utsim-ferramenta=capacidade",
                "--utsim-aba=Dados",
                "--utsim-lie=9.5",
                "--utsim-lse=10.5",
                "--utsim-subgrupo=4",
            ])
        );
        p.entrada.subgrupo = Some(0);
        p.entrada.lie = Some(f64::NAN);
        assert!(
            !p.argumentos_diretos()
                .iter()
                .any(|s| s.contains("subgrupo") || s.contains("lie"))
        );
        let mut m = Pedido::novo("x", acao::EDITAR_NO);
        let modelo = std::env::current_dir().unwrap().join("m.ron");
        m.contexto.modelo = Some("m.ron".into());
        m.contexto.no = Some("Torno 1".into());
        assert_eq!(
            m.argumentos_diretos(),
            a(&[
                format!("--utsim-abrir={}", modelo.display()).as_str(),
                "--utsim-no=Torno 1"
            ])
        );
    }

    #[test]
    fn planejar_experimento_ida_e_volta() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Pedido::planejar_experimento(
            "utsim 0.5.0",
            dir.path().join("linha.ron"),
            vec![
                Fator {
                    campo: "maquina.process".into(),
                    valor: "Exp(mean: {x})".into(),
                    baixo: 2.0,
                    alto: 4.0,
                    ..Fator::default()
                },
                Fator {
                    nome: "Buffer".into(),
                    campo: "fila.capacity".into(),
                    baixo: 2.0,
                    alto: 8.0,
                    inteiro: true,
                    ..Fator::default()
                },
            ],
            vec!["throughput".into()],
        );
        p.entrada.corridas = Some(14);
        let c = p.gravar(dir.path()).unwrap();
        let lido = Pedido::ler(&c).unwrap();
        assert_eq!(lido, p);
        assert_eq!(lido.acao, acao::PLANEJAR_EXPERIMENTO);
        // Um pedido antigo, sem os campos novos, continua sendo lido.
        let velho = r#"{"formato":1,"id":"a","de":"x","acao":"analisar"}"#;
        let v: Pedido = serde_json::from_str(velho).unwrap();
        assert!(v.entrada.fatores.is_empty() && v.entrada.corridas.is_none());
    }

    #[test]
    fn id_vira_nome_de_arquivo() {
        assert_eq!(seguro("a/b:c"), "a_b_c");
        assert_eq!(seguro(""), "pedido");
    }
}
