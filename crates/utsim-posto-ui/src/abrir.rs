//! Abrir pela linha de comando (seção 2.2 de `docs/plano_integracao_familia.md`):
//!
//! - `--utsim-abrir=<modelo.ron> --utsim-no=<nome>`: o 3D abre o app na tela Modelo com o
//!   nó escolhido. A trava é tomada como num abrir normal; com a trava de outro, o modelo
//!   fica só para leitura e a tela avisa.
//! - `--utsim-abrir=<dados.csv|.xlsx>` (um ou mais) com `--utsim-ferramenta=<chave>`,
//!   `--utsim-aba=`, `--utsim-lie=`, `--utsim-lse=` e `--utsim-subgrupo=` opcionais: um
//!   estudo novo com uma ferramenta por arquivo, de origem Importado (as missões do
//!   Ensino). Sem ferramenta, a tela Ferramentas pede que a pessoa escolha.
//!
//! - `--utsim-pedido=<id.utsim-pedido.json>`: o pedido de outro app (`utsim-troca`, seção 2
//!   do plano). O modelo e o nó do contexto, os arquivos de entrada (conferidos pelo hash),
//!   o conteúdo curto, a ferramenta, a aba, LIE, LSE e subgrupo viram as opções acima. As
//!   colunas escolhidas (`entrada.colunas`) ficam, nessa ordem, e as outras saem. Com
//!   `somente_leitura`, o modelo abre sem a trava. Ao fechar, o app grava a resposta no
//!   caminho pedido ([`responder`]).
//!
//! Cada opção aceita `--opcao=valor` e `--opcao valor`. Um caminho solto também abre.

use std::path::{Path, PathBuf};

use utsim_estudo::{Ferramenta, Origem, Parametros, Tabela, importar};

use utsim_posto::pasta::MANIFESTO;

use crate::Sessao;
use crate::ferramentas::TelaFerramentas;
use crate::modelo::TelaModelo;

/// Um arquivo de dados lido: (nome, tabela, origem).
pub type Arquivo = (String, Tabela, Origem);

/// O que a linha de comando pediu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Abrir {
    pub modelo: Option<PathBuf>,
    pub no: Option<String>,
    pub dados: Vec<PathBuf>,
    pub ferramenta: Option<Ferramenta>,
    pub aba: Option<String>,
    pub lie: Option<f64>,
    pub lse: Option<f64>,
    pub subgrupo: Option<usize>,
    /// As colunas a usar de cada arquivo, pelo nome do cabeçalho e nessa ordem (do pedido).
    pub colunas: Vec<String>,
    /// Abrir o modelo só para leitura, sem tomar a trava (do pedido).
    pub somente_leitura: bool,
    /// O pedido de outro app (`--utsim-pedido`), para responder ao fechar.
    pub pedido: Option<utsim_troca::Pedido>,
    /// O que impediu atender o pedido: vai na resposta, com estado erro.
    pub falha_pedido: Option<String>,
    /// Opções que não entendemos (vão para a barra de estado).
    pub erros: Vec<String>,
}

/// A tela que o app mostra depois de atender o pedido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destino {
    Modelo,
    Ferramentas,
}

const OPCOES: [&str; 8] = [
    "--utsim-abrir",
    "--utsim-pedido",
    "--utsim-no",
    "--utsim-ferramenta",
    "--utsim-aba",
    "--utsim-lie",
    "--utsim-lse",
    "--utsim-subgrupo",
];

fn eh_modelo(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ron"))
}

impl Abrir {
    /// Lê os argumentos (sem o nome do programa).
    pub fn de_args<I: IntoIterator<Item = String>>(args: I) -> Abrir {
        let mut a = Abrir::default();
        let mut it = args.into_iter().peekable();
        while let Some(arg) = it.next() {
            let (opcao, valor) = match arg.split_once('=') {
                Some((o, v)) if o.starts_with("--") => (o.to_string(), Some(v.to_string())),
                _ => (arg.clone(), None),
            };
            if !opcao.starts_with("--") {
                a.caminho(PathBuf::from(&arg));
                continue;
            }
            if !OPCOES.contains(&opcao.as_str()) {
                if opcao.starts_with("--utsim-") {
                    a.erros.push(format!("opção desconhecida: {opcao}"));
                }
                continue;
            }
            let valor = match valor {
                Some(v) => v,
                None => match it.next_if(|v| !v.starts_with("--")) {
                    Some(v) => v,
                    None => {
                        a.erros.push(format!("{opcao} sem valor"));
                        continue;
                    }
                },
            };
            let numero = |v: &str| utsim_estudo::tabela::numero(v);
            match opcao.as_str() {
                "--utsim-abrir" => a.caminho(PathBuf::from(valor)),
                "--utsim-pedido" => a.de_pedido(Path::new(&valor)),
                "--utsim-no" => a.no = Some(valor),
                "--utsim-ferramenta" => match importar::de_chave(&valor) {
                    Some(f) => a.ferramenta = Some(f),
                    None => a.erros.push(format!("ferramenta desconhecida: {valor}")),
                },
                "--utsim-aba" => a.aba = Some(valor),
                "--utsim-lie" => match numero(&valor) {
                    Some(v) => a.lie = Some(v),
                    None => a.erros.push(format!("LIE não é número: {valor}")),
                },
                "--utsim-lse" => match numero(&valor) {
                    Some(v) => a.lse = Some(v),
                    None => a.erros.push(format!("LSE não é número: {valor}")),
                },
                "--utsim-subgrupo" => match valor.trim().parse::<usize>() {
                    Ok(n) if n >= 1 => a.subgrupo = Some(n),
                    _ => a.erros.push(format!("subgrupo inválido: {valor}")),
                },
                _ => {}
            }
        }
        a
    }

    fn caminho(&mut self, p: PathBuf) {
        if eh_modelo(&p) {
            if self.modelo.is_some() {
                self.erros
                    .push("mais de um modelo pedido: fica o último".to_string());
            }
            self.modelo = Some(p);
        } else {
            self.dados.push(p);
        }
    }

    /// Lê o pedido de outro app e põe o que ele pede nas opções diretas.
    fn de_pedido(&mut self, caminho: &Path) {
        use utsim_troca::acao;
        let p = match utsim_troca::Pedido::ler(caminho) {
            Ok(p) => p,
            Err(e) => {
                self.erros.push(format!("pedido: {e}"));
                return;
            }
        };
        if let Some(m) = &p.contexto.modelo {
            self.caminho(m.clone());
            self.no = p.contexto.no.clone();
        }
        self.somente_leitura = p.somente_leitura;
        match p.acao.as_str() {
            acao::ABRIR_FERRAMENTA | acao::ANALISAR => {
                match p.conferir_entrada() {
                    Ok(()) => self
                        .dados
                        .extend(p.entrada.arquivos.iter().map(|a| a.caminho.clone())),
                    Err(e) => self.falhar_pedido(format!("pedido: {e}")),
                }
                if let Some(c) = p
                    .entrada
                    .conteudo
                    .as_deref()
                    .filter(|c| !c.trim().is_empty())
                {
                    match gravar_conteudo(caminho, &p.id, c) {
                        Ok(c) => self.dados.push(c),
                        Err(e) => self.falhar_pedido(format!("pedido: {e}")),
                    }
                }
                if let Some(k) = &p.ferramenta {
                    match importar::de_chave(k) {
                        Some(f) => self.ferramenta = Some(f),
                        None => self.falhar_pedido(format!("ferramenta desconhecida: {k}")),
                    }
                }
                let e = &p.entrada;
                self.colunas = e.colunas.clone();
                self.aba = e.aba.clone().or(self.aba.take());
                self.lie = e.lie.filter(|x| x.is_finite()).or(self.lie);
                self.lse = e.lse.filter(|x| x.is_finite()).or(self.lse);
                self.subgrupo = e.subgrupo.filter(|&n| n >= 1).or(self.subgrupo);
            }
            acao::ABRIR_MODELO | acao::EDITAR_NO => {
                if p.contexto.modelo.is_none() {
                    self.falhar_pedido("pedido sem modelo para abrir".into());
                }
            }
            outra => self.falhar_pedido(format!("este app não atende a ação {outra}")),
        }
        self.pedido = Some(p);
    }

    fn falhar_pedido(&mut self, m: String) {
        self.erros.push(m.clone());
        self.falha_pedido.get_or_insert(m);
    }

    /// Nada a abrir.
    pub fn vazio(&self) -> bool {
        self.modelo.is_none()
            && self.dados.is_empty()
            && self.erros.is_empty()
            && self.pedido.is_none()
    }

    /// Os parâmetros que valem para todas as ferramentas pedidas.
    pub fn parametros(&self) -> Parametros {
        let mut p = Parametros {
            lie: self.lie,
            lse: self.lse,
            ..Parametros::default()
        };
        if let Some(n) = self.subgrupo {
            p.subgrupo = n;
        }
        if let (Some(i), Some(s)) = (self.lie, self.lse) {
            p.tolerancia = Some(s - i);
        }
        p
    }
}

/// O pedido atendido, que o app guarda até fechar para responder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Atendido {
    pub abrir: Abrir,
    /// O estudo que o pedido abriu na tela Ferramentas.
    pub estudo: Option<String>,
}

impl Atendido {
    /// Depois de [`atender`]: `None` quando não houve pedido de outro app.
    pub fn de(a: &Abrir, ferramentas: &TelaFerramentas) -> Option<Atendido> {
        a.pedido.as_ref()?;
        let abriu_estudo = a.ferramenta.is_some() && !a.dados.is_empty();
        Some(Atendido {
            abrir: a.clone(),
            estudo: ferramentas
                .estudo
                .as_ref()
                .filter(|_| abriu_estudo && a.falha_pedido.is_none())
                .map(|e| e.id.clone()),
        })
    }

    /// Grava a resposta ([`responder`]) em nome de `app` na `versao`.
    pub fn responder(
        &self,
        ferramentas: &TelaFerramentas,
        s: &Sessao,
        app: &str,
        versao: &str,
    ) -> Option<Result<PathBuf, String>> {
        let origem = utsim_troca::Origem {
            app: app.into(),
            versao: versao.into(),
            quem: pessoa(s),
            quando: crate::util::agora_iso(),
        };
        responder(&self.abrir, self.estudo.as_deref(), ferramentas, origem)
    }
}

/// Grava o conteúdo curto do pedido como CSV ao lado dele (`<id>.entrada.csv`).
fn gravar_conteudo(pedido: &Path, id: &str, conteudo: &str) -> Result<PathBuf, String> {
    let id: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let pasta = pedido.parent().unwrap_or(Path::new("."));
    let c = pasta.join(format!("{id}.entrada.csv"));
    utsim_pasta::write_replace(&c, conteudo.as_bytes()).map_err(|e| e.to_string())?;
    Ok(c)
}

/// Grava a resposta do pedido, se houve pedido com caminho de resposta. `estudo` é o id do
/// estudo aberto pelo pedido: com ele aberto na tela, a resposta leva os indicadores, os
/// achados e os avisos de cada aplicação ([`resultado_do_estudo`]). Devolve o caminho
/// gravado ou o erro.
pub fn responder(
    a: &Abrir,
    estudo: Option<&str>,
    ferramentas: &TelaFerramentas,
    origem: utsim_troca::Origem,
) -> Option<Result<PathBuf, String>> {
    use utsim_troca::{Estado, Resposta};
    let p = a.pedido.as_ref()?;
    let caminho = p.resposta.clone()?;
    let r = match &a.falha_pedido {
        Some(m) => Resposta {
            mensagem: Some(m.clone()),
            ..Resposta::nova(p, Estado::Erro, origem)
        },
        None => {
            let aberto = ferramentas
                .estudo
                .as_ref()
                .filter(|e| estudo.is_some_and(|id| id == e.id));
            let mut r = Resposta::nova(p, Estado::Concluido, origem);
            match aberto {
                Some(e) => r.resultado = Some(resultado_do_estudo(p, e)),
                None if estudo.is_some() => {
                    r.mensagem = Some(
                        "O estudo do pedido não estava aberto ao fechar: veja na unidade.".into(),
                    );
                }
                None => {}
            }
            r
        }
    };
    Some(
        r.gravar(&caminho)
            .map(|()| caminho)
            .map_err(|e| e.to_string()),
    )
}

/// O estudo no formato de troca (versão 1): título, id e, por aplicação, ferramenta,
/// indicadores, achados, avisos e a leitura da equipe, como a tela mostra.
pub fn resultado_do_estudo(
    p: &utsim_troca::Pedido,
    e: &utsim_estudo::Estudo,
) -> utsim_troca::Resultado {
    use serde_json::json;
    let aplicacoes: Vec<_> = e
        .aplicacoes
        .iter()
        .map(|a| {
            let r = utsim_estudo::calculo::calcular(a, &e.assunto.problema);
            let indicadores: Vec<_> = r
                .indicadores
                .iter()
                .map(|k| json!({ "nome": k.label, "valor": k.value, "nota": k.note }))
                .collect();
            let achados: Vec<_> = r
                .achados
                .iter()
                .map(|x| json!({ "titulo": x.titulo, "texto": x.texto }))
                .collect();
            json!({
                "titulo": a.titulo,
                "ferramenta": importar::chave(a.ferramenta),
                "indicadores": indicadores,
                "achados": achados,
                "avisos": r.avisos,
                "conclusao": a.conclusao,
            })
        })
        .collect();
    utsim_troca::Resultado {
        ferramenta: p.ferramenta.clone().unwrap_or_else(|| "estudo".into()),
        formato: 1,
        dados: json!({ "estudo": e.titulo, "id": e.id, "aplicacoes": aplicacoes }),
    }
}

/// Lê os arquivos de dados: (nome do arquivo, tabela, origem). Os que falham vão para a
/// lista de erros.
pub fn ler_dados(a: &Abrir) -> (Vec<Arquivo>, Vec<String>) {
    let mut ok = Vec::new();
    let mut erros = Vec::new();
    for p in &a.dados {
        let nome = p.file_name().map_or_else(
            || p.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let r = std::fs::read(p)
            .map_err(|e| e.to_string())
            .and_then(|b| Tabela::importar(&nome, &b, a.aba.as_deref()));
        let r = r.and_then(|(t, o)| Ok((escolher_colunas(t, &a.colunas)?, o)));
        match r {
            Ok((t, o)) if !t.vazia() => ok.push((nome, t, o)),
            Ok(_) => erros.push(format!("{nome}: o arquivo não tem dados")),
            Err(e) => erros.push(format!("{nome}: {e}")),
        }
    }
    (ok, erros)
}

/// Fica com as `colunas` (pelo nome, sem acento nem caixa), na ordem pedida. Sem colunas,
/// a tabela fica como está. Uma coluna que não existe é erro.
fn escolher_colunas(t: Tabela, colunas: &[String]) -> Result<Tabela, String> {
    if colunas.is_empty() {
        return Ok(t);
    }
    let nomes: Vec<String> = t.colunas.iter().map(|c| importar::normalizar(c)).collect();
    let mut idx = Vec::new();
    let mut faltam = Vec::new();
    for c in colunas {
        match nomes.iter().position(|n| *n == importar::normalizar(c)) {
            Some(i) => idx.push(i),
            None => faltam.push(c.as_str()),
        }
    }
    if !faltam.is_empty() {
        return Err(format!("coluna que não existe: {}", faltam.join(", ")));
    }
    let pegar = |l: &[String]| {
        idx.iter()
            .map(|&i| l.get(i).cloned().unwrap_or_default())
            .collect()
    };
    Ok(Tabela {
        colunas: pegar(&t.colunas),
        linhas: t.linhas.iter().map(|l| pegar(l)).collect(),
    })
}

/// A pasta da unidade "Análises avulsas", ao lado da configuração do app.
pub fn pasta_avulsa(s: &Sessao) -> Option<PathBuf> {
    s.arquivo_config()
        .and_then(|a| a.parent())
        .map(|d| d.join("analises-avulsas"))
}

/// Quem abre: o autor lembrado ou o usuário do sistema.
pub fn pessoa(s: &Sessao) -> String {
    if let Some(u) = s.unidade.as_ref() {
        return u.autor().to_string();
    }
    let a = s.config.autor.trim();
    if !a.is_empty() {
        return a.to_string();
    }
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "pessoa".into())
}

/// Reabre a última unidade, se ela ainda existe.
fn reabrir_ultima(s: &mut Sessao) {
    if s.unidade.is_some() {
        return;
    }
    let ultima = PathBuf::from(s.config.ultima_unidade.trim());
    if !ultima.as_os_str().is_empty() && ultima.join(MANIFESTO).exists() {
        let autor = pessoa(s);
        s.abrir(&ultima, &autor);
    }
}

/// Abre a unidade de análises avulsas (cria na primeira vez).
fn abrir_avulsa(s: &mut Sessao) -> bool {
    let Some(p) = pasta_avulsa(s) else {
        s.falhar("Sem pasta de configuração para a unidade de análises avulsas.");
        return false;
    };
    let autor = pessoa(s);
    if p.join(MANIFESTO).exists() {
        s.abrir(&p, &autor)
    } else {
        s.criar(&p, "Análises avulsas", &autor)
    }
}

/// Atende o pedido: abre o modelo no nó ou cria o estudo com os dados. Devolve a tela a
/// mostrar.
pub fn atender(
    a: &Abrir,
    s: &mut Sessao,
    modelo: &mut TelaModelo,
    ferramentas: &mut TelaFerramentas,
) -> Option<Destino> {
    let mut erros = a.erros.clone();
    reabrir_ultima(s);
    let mut destino = None;
    if let Some(m) = &a.modelo {
        let quem = pessoa(s);
        match modelo.abrir_em_no(m, a.no.as_deref(), &quem) {
            Ok(()) => {
                if a.somente_leitura {
                    modelo.soltar_edicao();
                }
                destino = Some(Destino::Modelo);
            }
            Err(e) => erros.push(e),
        }
    }
    if !a.dados.is_empty() {
        let (lidos, falhas) = ler_dados(a);
        erros.extend(falhas);
        if !lidos.is_empty() && (s.unidade.is_some() || abrir_avulsa(s)) {
            let p = a.parametros();
            match a.ferramenta {
                Some(f) => {
                    if ferramentas.importar_estudo(s, lidos, f, &p).is_some() {
                        destino = Some(Destino::Ferramentas);
                    }
                }
                None => {
                    ferramentas.dados_pendentes = Some((lidos, p));
                    destino = Some(Destino::Ferramentas);
                }
            }
        }
    }
    if !erros.is_empty() {
        s.falhar(erros.join(". "));
    }
    destino
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Abrir {
        Abrir::de_args(v.iter().map(|s| s.to_string()))
    }

    #[test]
    fn model_and_node_in_both_forms() {
        let a = args(&["--utsim-abrir=C:/m/linha 1.ron", "--utsim-no=torno"]);
        assert_eq!(a.modelo, Some(PathBuf::from("C:/m/linha 1.ron")));
        assert_eq!(a.no.as_deref(), Some("torno"));
        assert!(a.erros.is_empty() && a.dados.is_empty());
        let b = args(&["--utsim-abrir", "C:/m/linha 1.ron", "--utsim-no", "torno"]);
        assert_eq!(a, b);
        assert!(args(&[]).vazio());
    }

    #[test]
    fn data_files_with_tool_and_limits() {
        let a = args(&[
            "--utsim-abrir=antes.csv",
            "--utsim-abrir=depois.xlsx",
            "--utsim-ferramenta=capacidade",
            "--utsim-lie=24,985",
            "--utsim-lse=25.015",
            "--utsim-subgrupo=5",
            "--utsim-aba=Dados",
        ]);
        assert_eq!(a.dados.len(), 2);
        assert_eq!(a.ferramenta, Some(Ferramenta::Capacidade));
        assert_eq!(a.lie, Some(24.985));
        let p = a.parametros();
        assert_eq!(p.subgrupo, 5);
        assert!((p.tolerancia.unwrap() - 0.03).abs() < 1e-9);
        assert_eq!(a.aba.as_deref(), Some("Dados"));
        assert!(a.erros.is_empty());
    }

    fn telas() -> (TelaModelo, TelaFerramentas) {
        let resumo: crate::FnResumo = std::sync::Arc::new(|_| Ok(Default::default()));
        (
            TelaModelo::nova("qualidade", utsim_edit::abas::Aba::Qualidade, resumo),
            TelaFerramentas::nova(
                utsim_estudo::App::Qualidade,
                utsim_estudo::relatorio::Identidade::default(),
                None,
            ),
        )
    }

    fn copiar_modelo(dir: &Path) -> PathBuf {
        let m = dir.join("linha com espaço.ron");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../examples/inspecao_e_retrabalho.ron"
            ),
            &m,
        )
        .unwrap();
        m
    }

    #[test]
    fn the_3d_opens_the_model_on_the_node_and_its_lock_makes_it_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let m = copiar_modelo(dir.path());
        let mut s = Sessao::nova(Some(dir.path().join("config.json")));
        let (mut tm, mut tf) = telas();
        let a = args(&[
            &format!("--utsim-abrir={}", m.display()),
            "--utsim-no=torno",
        ]);
        assert_eq!(atender(&a, &mut s, &mut tm, &mut tf), Some(Destino::Modelo));
        assert!(
            s.unidade.is_none(),
            "sem unidade, o modelo abre assim mesmo"
        );
        assert_eq!(tm.selecionado, "torno");
        assert!(tm.pode_editar(), "trava livre: o app toma a trava");
        tm.fechar();

        // O 3D com a trava: só leitura, com aviso.
        utsim_pasta::trava::travar(
            &m,
            "utsim-3d",
            "Bruno",
            "outra",
            utsim_pasta::trava::agora_ms(),
        )
        .unwrap();
        let (mut tm, mut tf) = telas();
        atender(&a, &mut s, &mut tm, &mut tf);
        assert!(!tm.pode_editar());
        assert!(tm.aviso_trava().unwrap().contains("Bruno"));
        assert_eq!(tm.selecionado, "torno");

        // Nó que não existe: abre e avisa.
        let (mut tm, mut tf) = telas();
        let b = args(&[&format!("--utsim-abrir={}", m.display()), "--utsim-no=nada"]);
        atender(&b, &mut s, &mut tm, &mut tf);
        assert!(tm.aberto() && tm.selecionado.is_empty());
        assert!(tm.mensagem.contains("nada"), "{}", tm.mensagem);
    }

    #[test]
    fn data_files_become_a_study_in_the_loose_analyses_unit() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Sessao::nova(Some(dir.path().join("config.json")));
        let antes = dir.path().join("antes.csv");
        let depois = dir.path().join("depois.csv");
        let mut csv = String::from(
            "subgrupo;x1;x2;x3
",
        );
        for i in 1..=10 {
            let v = |k: f64| utsim_estudo::num(25.0 + ((i as f64) * k).sin() * 0.004, 4);
            csv.push_str(&format!(
                "{i};{};{};{}
",
                v(1.1),
                v(2.3),
                v(3.7)
            ));
        }
        std::fs::write(&antes, &csv).unwrap();
        std::fs::write(&depois, &csv).unwrap();
        let (mut tm, mut tf) = telas();
        let a = args(&[
            &format!("--utsim-abrir={}", antes.display()),
            &format!("--utsim-abrir={}", depois.display()),
            "--utsim-ferramenta=capacidade",
            "--utsim-lie=24,985",
            "--utsim-lse=25,015",
        ]);
        assert_eq!(
            atender(&a, &mut s, &mut tm, &mut tf),
            Some(Destino::Ferramentas)
        );
        let u = s.unidade.as_ref().expect("unidade de análises avulsas");
        assert!(u.raiz().ends_with("analises-avulsas"));
        let e = tf.estudo.as_ref().unwrap();
        assert_eq!(e.titulo, "Análise de antes.csv, depois.csv");
        assert_eq!(e.aplicacoes.len(), 2);
        let ap = &e.aplicacoes[0];
        assert_eq!(ap.titulo, "antes");
        assert_eq!(ap.parametros.subgrupo, 3);
        assert_eq!(ap.parametros.lie, Some(24.985));
        assert!(matches!(ap.origem, Origem::Importado { .. }));
        // Gravado na unidade.
        assert!(tf.estudos.iter().any(|x| x.id == e.id));

        // Sem ferramenta: os dados esperam a escolha.
        let (mut tm, mut tf) = telas();
        let b = args(&[&format!("--utsim-abrir={}", antes.display())]);
        assert_eq!(
            atender(&b, &mut s, &mut tm, &mut tf),
            Some(Destino::Ferramentas)
        );
        assert_eq!(tf.dados_pendentes.as_ref().unwrap().0.len(), 1);
    }

    #[test]
    fn errors_are_kept_and_other_options_ignored() {
        let a = args(&[
            "--utsim-ferramenta=nada",
            "--utsim-lie=x",
            "--utsim-cor=azul",
            "--outra",
            "--utsim-no",
        ]);
        assert_eq!(a.erros.len(), 4, "{:?}", a.erros);
        assert!(!a.vazio());
    }

    /// Dez subgrupos de 3 diâmetros perto de 25 mm.
    fn csv_diametros() -> String {
        let mut csv = String::from("subgrupo;x1;x2;x3\n");
        for i in 1..=10 {
            let v = |k: f64| utsim_estudo::num(25.0 + ((i as f64) * k).sin() * 0.004, 4);
            csv.push_str(&format!("{i};{};{};{}\n", v(1.1), v(2.3), v(3.7)));
        }
        csv
    }

    fn origem() -> utsim_troca::Origem {
        utsim_troca::Origem {
            app: "qualidade".into(),
            versao: "0.0.0".into(),
            quem: "teste".into(),
            quando: "2026-11-10T12:00:00Z".into(),
        }
    }

    #[test]
    fn request_opens_the_data_and_answers_with_the_study() {
        use utsim_troca::{Estado, Pedido, Resposta};
        let dir = tempfile::tempdir().unwrap();
        let dados = dir.path().join("diametros.csv");
        std::fs::write(&dados, csv_diametros()).unwrap();
        let mut p = Pedido::abrir_ferramenta("estatistica 0.2.0", "Capacidade", &[dados]).unwrap();
        p.entrada.lie = Some(24.985);
        p.entrada.lse = Some(25.015);
        let caminho = p.gravar(&dir.path().join("troca")).unwrap();

        let a = Abrir::de_args(Pedido::argumentos(&caminho));
        assert!(
            a.erros.is_empty() && a.falha_pedido.is_none(),
            "{:?}",
            a.erros
        );
        assert_eq!(a.dados.len(), 1);
        assert_eq!(a.ferramenta, Some(Ferramenta::Capacidade));
        assert_eq!((a.lie, a.lse), (Some(24.985), Some(25.015)));
        assert!(!a.vazio());
        // A forma com espaço também vale.
        let b = args(&["--utsim-pedido", &caminho.display().to_string()]);
        assert_eq!(a, b);

        let mut s = Sessao::nova(Some(dir.path().join("config.json")));
        let (mut tm, mut tf) = telas();
        assert_eq!(
            atender(&a, &mut s, &mut tm, &mut tf),
            Some(Destino::Ferramentas)
        );
        let at = Atendido::de(&a, &tf).unwrap();
        assert_eq!(
            at.estudo.as_deref(),
            tf.estudo.as_ref().map(|e| e.id.as_str())
        );
        let gravado = responder(&at.abrir, at.estudo.as_deref(), &tf, origem())
            .unwrap()
            .unwrap();
        assert_eq!(Some(gravado), p.resposta.clone());
        let r = Resposta::ler(&p).unwrap().expect("resposta gravada");
        assert_eq!(r.estado, Estado::Concluido);
        assert_eq!(r.origem.app, "qualidade");
        let res = r.resultado.unwrap();
        assert_eq!(res.ferramenta, "capacidade");
        let ap = &res.dados["aplicacoes"][0];
        assert_eq!(ap["ferramenta"], "capacidade");
        let nomes: Vec<&str> = ap["indicadores"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|k| k["nome"].as_str())
            .collect();
        assert!(
            nomes.contains(&"Cpk") && nomes.contains(&"Ppk"),
            "{nomes:?}"
        );

        // Sem pedido, nada a responder.
        assert!(responder(&args(&[]), None, &tf, origem()).is_none());
        assert!(Atendido::de(&args(&[]), &tf).is_none());
    }

    #[test]
    fn changed_file_or_unknown_action_answers_with_an_error() {
        use utsim_troca::{Estado, Pedido, Resposta, acao};
        let dir = tempfile::tempdir().unwrap();
        let dados = dir.path().join("d.csv");
        std::fs::write(&dados, csv_diametros()).unwrap();
        let mut p =
            Pedido::abrir_ferramenta("estatistica", "carta", std::slice::from_ref(&dados)).unwrap();
        let caminho = p.gravar(dir.path()).unwrap();
        std::fs::write(&dados, "outro conteúdo").unwrap();
        let a = Abrir::de_args(Pedido::argumentos(&caminho));
        assert!(a.dados.is_empty(), "o arquivo mudado não abre");
        assert!(a.falha_pedido.as_deref().unwrap().contains("hash"));
        let (_, tf) = telas();
        let at = Atendido::de(&a, &tf).unwrap();
        assert_eq!(at.estudo, None);
        responder(&at.abrir, None, &tf, origem()).unwrap().unwrap();
        let r = Resposta::ler(&p).unwrap().unwrap();
        assert_eq!(r.estado, Estado::Erro);
        assert!(r.mensagem.unwrap().contains("hash"));

        let mut q = Pedido::novo("estatistica", acao::RODAR_MISSAO);
        let c = q.gravar(dir.path()).unwrap();
        let a = Abrir::de_args(Pedido::argumentos(&c));
        assert!(a.falha_pedido.unwrap().contains(acao::RODAR_MISSAO));

        // Um pedido que não existe é só um erro na barra: não há onde responder.
        let a = args(&["--utsim-pedido=nao-existe.utsim-pedido.json"]);
        assert!(a.pedido.is_none() && a.erros.len() == 1, "{:?}", a.erros);
    }

    #[test]
    fn short_content_and_model_context() {
        use utsim_troca::{Pedido, acao};
        let dir = tempfile::tempdir().unwrap();
        let mut p = Pedido::novo("estatistica", acao::ABRIR_FERRAMENTA);
        p.ferramenta = Some("histograma".into());
        p.entrada.conteudo = Some("x\n1\n2\n3\n".into());
        p.entrada.subgrupo = Some(0);
        let c = p.gravar(dir.path()).unwrap();
        let a = Abrir::de_args(Pedido::argumentos(&c));
        assert!(a.erros.is_empty(), "{:?}", a.erros);
        assert_eq!(a.dados.len(), 1);
        assert!(a.dados[0].ends_with(format!("{}.entrada.csv", p.id)));
        assert_eq!(
            std::fs::read_to_string(&a.dados[0]).unwrap(),
            "x\n1\n2\n3\n"
        );
        assert_eq!(a.subgrupo, None, "subgrupo 0 fica de fora");

        let mut q = Pedido::novo("3d", acao::EDITAR_NO);
        q.contexto.modelo = Some(dir.path().join("linha.ron"));
        q.contexto.no = Some("torno".into());
        let c = q.gravar(dir.path()).unwrap();
        let a = Abrir::de_args(Pedido::argumentos(&c));
        assert_eq!(a.modelo, q.contexto.modelo);
        assert_eq!(a.no.as_deref(), Some("torno"));
        assert!(a.falha_pedido.is_none());
    }

    #[test]
    fn request_columns_pick_and_order_and_read_only_model() {
        use utsim_troca::{Pedido, acao};
        let dir = tempfile::tempdir().unwrap();
        let dados = dir.path().join("pecas.csv");
        std::fs::write(
            &dados,
            "Lote;Diâmetro;Peso\n1;25,001;10\n2;24,998;11\n3;25,003;12\n",
        )
        .unwrap();
        let mut p = Pedido::abrir_ferramenta("estatistica", "histograma", &[dados]).unwrap();
        p.entrada.colunas = vec!["peso".into(), "DIAMETRO".into()];
        let c = p.gravar(dir.path()).unwrap();
        let a = Abrir::de_args(Pedido::argumentos(&c));
        assert_eq!(a.colunas, ["peso", "DIAMETRO"]);
        let (lidos, erros) = ler_dados(&a);
        assert!(erros.is_empty(), "{erros:?}");
        let t = &lidos[0].1;
        assert_eq!(t.colunas, ["Peso", "Diâmetro"]);
        assert_eq!(t.linhas[1], ["11", "24,998"]);

        // Coluna que não existe: o arquivo não abre e o erro diz qual.
        let mut b = a.clone();
        b.colunas = vec!["altura".into()];
        let (lidos, erros) = ler_dados(&b);
        assert!(lidos.is_empty());
        assert!(erros[0].contains("altura"), "{erros:?}");

        // Só leitura: o modelo abre no nó sem ficar com a trava.
        let m = copiar_modelo(dir.path());
        let mut q = Pedido::novo("3d", acao::EDITAR_NO);
        q.contexto.modelo = Some(m.clone());
        q.contexto.no = Some("torno".into());
        q.somente_leitura = true;
        let c = q.gravar(dir.path()).unwrap();
        let a = Abrir::de_args(Pedido::argumentos(&c));
        assert!(a.somente_leitura);
        let mut s = Sessao::nova(Some(dir.path().join("config.json")));
        let (mut tm, mut tf) = telas();
        assert_eq!(atender(&a, &mut s, &mut tm, &mut tf), Some(Destino::Modelo));
        assert!(tm.aberto() && !tm.pode_editar());
        assert_eq!(tm.selecionado, "torno");
        // Sem trava nossa, outro app pode tomar a edição.
        let (mut outro, _) = telas();
        outro.abrir(&m, "Bruno").unwrap();
        assert!(outro.pode_editar());
    }
}
