//! Os relatórios do estudo pelo gerador comum: um por ferramenta e o consolidado, com
//! capa (app, título, estudo, autor, data, origem dos dados), sumário no consolidado,
//! gráficos, tabelas e a identidade do app (cor e ícone).

use utsim_ferramentas::relatorios::{Block, Cover, Document, Template, Tone, html, pdf};

use crate::calculo::{Resultado, calcular};
use crate::{Aplicacao, Estudo};

/// A identidade do app no relatório.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Identidade {
    /// `UTSim Qualidade`, `UTSim Segurança do Trabalho`.
    pub app: String,
    /// Cor da faixa da capa, `#RRGGBB`.
    pub cor: String,
    /// O ícone do app em PNG (a capa do HTML o mostra).
    pub icone_png: Vec<u8>,
}

/// Quem imprime e quando, e o nome do posto do estudo (quando há).
#[derive(Clone, Copy, Debug)]
pub struct Contexto<'a> {
    pub identidade: &'a Identidade,
    pub autor: &'a str,
    /// Data como é mostrada (`09/11/2026`).
    pub data: &'a str,
    pub nome_do_posto: Option<&'a str>,
}

fn capa(e: &Estudo, titulo: &str, subtitulo: &str, origem: String, ctx: &Contexto) -> Cover {
    Cover {
        app: ctx.identidade.app.clone(),
        title: titulo.into(),
        subtitle: subtitulo.into(),
        author: if ctx.autor.is_empty() {
            e.autor.clone()
        } else {
            ctx.autor.into()
        },
        date: ctx.data.into(),
        source: origem,
        colour: ctx.identidade.cor.clone(),
        icon_png: ctx.identidade.icone_png.clone(),
    }
}

/// Escreve o resultado de uma aplicação no documento, depois de um título com o nome dela.
pub fn secao(doc: &mut Document, e: &Estudo, a: &Aplicacao) {
    let r = calcular(a, &e.assunto.problema);
    doc.heading(&a.titulo);
    let mut kv = vec![("Ferramenta".to_string(), a.ferramenta.nome().to_string())];
    if a.ferramenta.usa_tabela() {
        kv.push(("Origem dos dados".into(), a.origem.descricao()));
    }
    doc.kv(kv);
    escrever(doc, &r);
    if !a.conclusao.trim().is_empty() {
        doc.callout(Tone::Info, "Leitura da equipe", a.conclusao.trim());
    }
}

/// O resultado em blocos: indicadores, achados, gráficos, Ishikawa, textos, tabelas e avisos.
pub fn escrever(doc: &mut Document, r: &Resultado) {
    if !r.indicadores.is_empty() {
        doc.kpis(r.indicadores.clone());
    }
    for a in &r.achados {
        doc.callout(a.tom, &a.titulo, &a.texto);
    }
    for g in &r.graficos {
        doc.chart(g.clone());
    }
    if let Some((efeito, categorias)) = &r.ishikawa {
        doc.blocks.push(Block::Ishikawa {
            effect: efeito.clone(),
            categories: categorias.clone(),
        });
    }
    for (titulo, linhas) in &r.textos {
        if linhas.len() == 1 && !titulo.starts_with("Cadeia") {
            doc.kv(vec![(titulo.clone(), linhas[0].clone())]);
        } else {
            doc.para(titulo);
            doc.bullets(linhas.clone());
        }
    }
    for q in &r.quadros {
        let cab: Vec<&str> = q.cabecalho.iter().map(String::as_str).collect();
        doc.table(&q.nome, &cab, q.linhas.clone(), &q.larguras);
    }
    if !r.avisos.is_empty() {
        doc.callout(Tone::Warn, "Avisos", &r.avisos.join(" "));
    }
}

/// O relatório de uma ferramenta, com capa.
pub fn documento_ferramenta(e: &Estudo, a: &Aplicacao, ctx: &Contexto) -> Document {
    let assunto = e.assunto.resumo(ctx.nome_do_posto);
    let mut doc = Document::new(&a.titulo, &e.titulo);
    doc.app = ctx.identidade.app.clone();
    let origem = if a.ferramenta.usa_tabela() {
        a.origem.descricao()
    } else {
        "Análise da equipe".into()
    };
    doc.cover = Some(capa(
        e,
        &a.titulo,
        &format!("{} · estudo {} ({assunto})", a.ferramenta.nome(), e.titulo),
        origem,
        ctx,
    ));
    secao(&mut doc, e, a);
    doc
}

/// O relatório consolidado do estudo com as aplicações escolhidas (todas quando `ids` é
/// vazio), na ordem do estudo, com sumário.
pub fn documento_estudo(e: &Estudo, ids: &[String], ctx: &Contexto) -> Document {
    let escolhidas: Vec<&Aplicacao> = e
        .aplicacoes
        .iter()
        .filter(|a| ids.is_empty() || ids.contains(&a.id))
        .collect();
    let assunto = e.assunto.resumo(ctx.nome_do_posto);
    let mut doc = Document::new(&e.titulo, &assunto);
    doc.app = ctx.identidade.app.clone();
    let origens = e.origens();
    doc.cover = Some(capa(
        e,
        &e.titulo,
        &format!("Estudo de {assunto}"),
        if origens.is_empty() {
            "Análise da equipe".into()
        } else {
            origens.join("; ")
        },
        ctx,
    ));
    doc.toc = true;
    doc.heading("Resumo do estudo");
    let mut kv = vec![("Assunto".to_string(), assunto.clone())];
    if !e.assunto.problema.trim().is_empty() {
        kv.push(("Problema".into(), e.assunto.problema.trim().into()));
    }
    if !e.autor.is_empty() {
        kv.push(("Responsável".into(), e.autor.clone()));
    }
    if !e.criado.is_empty() {
        kv.push(("Aberto em".into(), e.criado.clone()));
    }
    doc.kv(kv);
    let rows: Vec<Vec<String>> = escolhidas
        .iter()
        .map(|a| {
            vec![
                a.titulo.clone(),
                a.ferramenta.nome().into(),
                if a.ferramenta.usa_tabela() {
                    a.origem.rotulo().into()
                } else {
                    "—".into()
                },
            ]
        })
        .collect();
    doc.table(
        "Ferramentas aplicadas",
        &["Aplicação", "Ferramenta", "Origem"],
        rows,
        &[2.5, 2.5, 1.0],
    );
    for a in &escolhidas {
        doc.blocks.push(Block::PageBreak);
        secao(&mut doc, e, a);
    }
    if !e.conclusao.trim().is_empty() {
        doc.heading("Conclusão");
        for p in e.conclusao.split("\n\n") {
            doc.para(p.trim());
        }
    }
    doc
}

/// O documento em PDF. `emitido` vai no rodapé (quem e quando imprimiu).
pub fn pdf(doc: &Document, t: &Template, emitido: &str) -> Vec<u8> {
    pdf::render(doc, t, emitido)
}

/// O documento em uma página HTML.
pub fn html(doc: &Document, t: &Template, emitido: &str) -> String {
    html::render(doc, t, emitido)
}

/// O modelo de página com a cor do app (o cabeçalho leva o nome do app quando a empresa
/// está vazia).
pub fn modelo_do_app(id: &Identidade) -> Template {
    Template {
        colour: id.cor.clone(),
        ..Template::default()
    }
}
