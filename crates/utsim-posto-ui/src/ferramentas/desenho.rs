//! As ferramentas desenhadas da tela Ferramentas: o Ishikawa como espinha editável, os 5
//! porquês em árvore, o A3 como folha, a casa da qualidade do QFD, a matriz S × O da FMEA
//! e o Pareto vivo. Os desenhos são os de `utsim_visual` (os mesmos do LSS); o que cada
//! gesto faz nos dados está em `utsim_estudo::interacao`, e o formato do estudo não muda.

use egui::{Color32, RichText};
use utsim_estudo::calculo::{self, NPR_ALTO, NPR_MEDIO, RiscoFmea};
use utsim_estudo::interacao::{self as ia, Papel};
use utsim_estudo::{Aplicacao, Categoria, Ferramenta};
use utsim_ferramentas::ap::Ap;
use utsim_visual::{Nivel, arvore, casa, cores, espinha, matriz, pareto};

use crate::texto::{fx, tx, vx};

/// Por qual risco a FMEA é colorida.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Criterio {
    #[default]
    Ap,
    Npr,
}

/// O nível de risco de uma linha da FMEA pelo critério.
pub fn nivel(r: &RiscoFmea, c: Criterio) -> Nivel {
    match c {
        Criterio::Ap => match r.ap {
            Ap::High => Nivel::Alto,
            Ap::Medium => Nivel::Medio,
            Ap::Low => Nivel::Baixo,
        },
        Criterio::Npr => {
            if r.npr >= NPR_ALTO {
                Nivel::Alto
            } else if r.npr >= NPR_MEDIO {
                Nivel::Medio
            } else {
                Nivel::Baixo
            }
        }
    }
}

/// O selo da linha na grade: a cor do nível e "AP H · 240".
pub fn selo(r: &RiscoFmea, c: Criterio) -> (Color32, String) {
    (
        cores::do_nivel(nivel(r, c)),
        format!("{} · {}", r.ap.letter(), r.npr),
    )
}

fn hint(problema: &str, padrao: &'static str) -> String {
    if problema.is_empty() {
        padrao.to_string()
    } else {
        problema.to_string()
    }
}

/// O Ishikawa: o efeito, a espinha editável e o botão de categoria. Devolve a causa com que
/// a pessoa pediu para começar um 5 porquês.
pub fn ishikawa(
    ui: &mut egui::Ui,
    a: &mut Aplicacao,
    problema: &str,
    com_porques: &[String],
) -> Option<String> {
    ui.horizontal(|ui| {
        ui.label(tx("ferramentas-efeito"));
        ui.add(
            egui::TextEdit::singleline(&mut a.efeito)
                .hint_text(hint(problema, tx("ferramentas-o_efeito")))
                .desired_width((ui.available_width() - 140.0).max(200.0)),
        );
        if ui.button(tx("ferramentas-categoria")).clicked() {
            a.categorias.push(Categoria {
                nome: tx("ferramentas-nova_categoria").into(),
                causas: Vec::new(),
            });
        }
    });
    ui.label(RichText::new(tx("ferramentas-dica_espinha")).small().weak());
    let ossos: Vec<espinha::Osso> = a
        .categorias
        .iter()
        .map(|c| espinha::Osso {
            nome: c.nome.clone(),
            cartoes: c
                .causas
                .iter()
                .map(|t| espinha::Cartao {
                    marca: com_porques
                        .contains(&t.trim().to_lowercase())
                        .then(|| "5P".to_string()),
                    ..espinha::Cartao::novo(t.clone())
                })
                .collect(),
        })
        .collect();
    let efeito = if a.efeito.trim().is_empty() {
        problema.to_string()
    } else {
        a.efeito.clone()
    };
    let acoes = espinha::mostrar(
        ui,
        ("espinha", a.id.as_str()),
        &espinha::Espinha::editavel(&efeito, &ossos),
    );
    let mut porques = None;
    for acao in acoes {
        let c = &mut a.categorias;
        match acao {
            espinha::Acao::Mover { osso, cartao, para } => {
                ia::mover_causa(c, osso, cartao, para);
            }
            espinha::Acao::Acrescentar { osso, texto } => ia::acrescentar_causa(c, osso, &texto),
            espinha::Acao::Editar {
                osso,
                cartao,
                texto,
            } => ia::editar_causa(c, osso, cartao, &texto),
            espinha::Acao::Remover { osso, cartao } => ia::remover_causa(c, osso, cartao),
            espinha::Acao::PorQues { osso, cartao } => {
                porques = c.get(osso).and_then(|x| x.causas.get(cartao)).cloned();
            }
            espinha::Acao::RenomearOsso { osso, nome } => ia::renomear_categoria(c, osso, &nome),
            espinha::Acao::RemoverOsso { osso } => ia::remover_categoria(c, osso),
        }
    }
    porques
}

/// Os 5 porquês em árvore, montada das cadeias do estudo.
pub fn porques(ui: &mut egui::Ui, a: &mut Aplicacao, problema: &str) {
    ui.horizontal(|ui| {
        ui.label(tx("ferramentas-problema"));
        ui.add(
            egui::TextEdit::singleline(&mut a.efeito)
                .hint_text(hint(problema, tx("ferramentas-o_problema")))
                .desired_width(f32::INFINITY),
        );
    });
    ui.label(RichText::new(tx("ferramentas-dica_arvore")).small().weak());
    let raiz = if a.efeito.trim().is_empty() {
        problema.to_string()
    } else {
        a.efeito.clone()
    };
    let nos = ia::arvore_dos_porques(&raiz, &a.cadeias);
    let vista: Vec<arvore::No> = nos
        .iter()
        .map(|n| {
            let tipo = match n.papel {
                Papel::Problema => arvore::Tipo::Raiz,
                Papel::Porque { .. } => arvore::Tipo::Porque,
                Papel::Acao { .. } => arvore::Tipo::Acao,
            };
            let mut v = arvore::No::novo(n.pai, tipo, n.texto.clone());
            if tipo == arvore::Tipo::Acao {
                v.removivel = !n.texto.trim().is_empty();
            }
            if n.cadeias.len() > 1 && tipo == arvore::Tipo::Porque {
                v.dica = Some(fx(
                    "ferramentas-no_de_varias_cadeias",
                    &[("n", vx(&n.cadeias.len())), ("texto", vx(&n.texto))],
                ));
            }
            v
        })
        .collect();
    let acoes = egui::ScrollArea::horizontal()
        .id_salt(("arvore_rolagem", a.id.as_str()))
        .show(ui, |ui| {
            arvore::mostrar(
                ui,
                ("arvore", a.id.as_str()),
                &arvore::Arvore {
                    nos: &vista,
                    marcar: None,
                },
            )
        })
        .inner;
    for acao in acoes {
        match acao {
            arvore::Acao::Editar { no, texto } => {
                ia::editar_no(&mut a.efeito, &mut a.cadeias, &nos, no, &texto);
            }
            arvore::Acao::Filho { no, texto } => ia::porque_filho(&mut a.cadeias, &nos, no, &texto),
            arvore::Acao::Remover { no } => ia::remover_no(&mut a.cadeias, &nos, no),
            arvore::Acao::Marcar { .. } => {}
        }
    }
}

/// O A3 como uma folha: as seções em duas colunas, cada uma com o seu número e a marca de
/// preenchida, e na Análise as miniaturas do Pareto e do Ishikawa do mesmo estudo.
pub fn a3(ui: &mut egui::Ui, a: &mut Aplicacao, outras: &[Aplicacao]) {
    let n = a.secoes.len();
    let feitas = ia::secoes_preenchidas(a);
    ui.horizontal(|ui| {
        ui.add(
            egui::ProgressBar::new(if n == 0 {
                0.0
            } else {
                feitas as f32 / n as f32
            })
            .desired_width(220.0)
            .text(fx(
                "ferramentas-secoes_a3",
                &[("n", vx(&feitas)), ("total", vx(&n))],
            )),
        );
    });
    ui.add_space(4.0);
    let esquerda = if n == 7 { 4 } else { n.div_ceil(2) };
    let acento = cores::destaque(ui);
    let fraco = ui.visuals().weak_text_color();
    let id = a.id.clone();
    let (cols_esq, cols_dir) = a.secoes.split_at_mut(esquerda.min(n));
    ui.columns(2, |cols| {
        for (lado, secoes, base) in [(0usize, cols_esq, 0usize), (1, cols_dir, esquerda)] {
            let ui = &mut cols[lado];
            for (k, s) in secoes.iter_mut().enumerate() {
                let cheia = !s.texto.trim().is_empty();
                let cor = if cheia { acento } else { fraco };
                egui::Frame::group(ui.style())
                    .stroke(egui::Stroke::new(if cheia { 1.5 } else { 1.0 }, cor))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}.", base + k + 1))
                                    .strong()
                                    .size(15.0)
                                    .color(cor),
                            );
                            ui.label(RichText::new(&s.titulo).strong().color(cor));
                            if cheia {
                                ui.label(RichText::new("✔").color(cores::VERDE));
                            }
                        });
                        ui.add(
                            egui::TextEdit::multiline(&mut s.texto)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                        if s.titulo == "Análise" {
                            miniaturas(ui, &id, outras);
                        }
                    });
                ui.add_space(4.0);
            }
        }
    });
}

/// O Pareto e o Ishikawa do estudo, pequenos, dentro da Análise do A3.
fn miniaturas(ui: &mut egui::Ui, id: &str, outras: &[Aplicacao]) {
    let pareto_a = outras.iter().find(|x| x.ferramenta == Ferramenta::Pareto);
    let ish = outras.iter().find(|x| x.ferramenta == Ferramenta::Ishikawa);
    if pareto_a.is_none() && ish.is_none() {
        ui.label(
            RichText::new(tx("ferramentas-a3_sem_outras"))
                .small()
                .weak(),
        );
        return;
    }
    ui.label(RichText::new(tx("ferramentas-do_estudo")).small().strong());
    if let Some(p) = pareto_a {
        ui.label(RichText::new(&p.titulo).small());
        let barras = barras(p);
        pareto::mostrar(ui, ("a3_pareto", id), &barras, 170.0);
    }
    if let Some(i) = ish {
        ui.label(RichText::new(&i.titulo).small());
        let ossos: Vec<espinha::Osso> = i
            .categorias
            .iter()
            .map(|c| espinha::Osso {
                nome: c.nome.clone(),
                cartoes: c
                    .causas
                    .iter()
                    .map(|t| espinha::Cartao::novo(t.clone()))
                    .collect(),
            })
            .collect();
        espinha::mostrar(
            ui,
            ("a3_espinha", id),
            &espinha::Espinha::miniatura(&i.efeito, &ossos),
        );
    }
}

/// As barras do Pareto da aplicação (o mesmo cálculo do resultado).
pub fn barras(a: &Aplicacao) -> Vec<pareto::Barra> {
    calculo::pareto_da_tabela(a)
        .0
        .rows
        .iter()
        .map(|r| pareto::Barra {
            nome: r.name.clone(),
            valor: r.value,
            pct: r.pct,
            acum: r.cum_pct,
            vital: r.vital,
        })
        .collect()
}

/// O id do Pareto vivo da aplicação (a barra escolhida fica na memória da tela com ele).
pub fn id_pareto(a: &Aplicacao) -> (&'static str, String) {
    ("pareto_vivo", a.id.clone())
}

/// A casa da qualidade, desenhada da tabela e editada com cliques.
pub fn casa_qfd(ui: &mut egui::Ui, a: &mut Aplicacao) {
    let c = ia::casa_da_tabela(a);
    ui.label(RichText::new(tx("ferramentas-casa_da_qualidade")).strong());
    if c.necessidades.is_empty() || c.requisitos.is_empty() {
        ui.label(RichText::new(tx("ferramentas-casa_vazia")).small().weak());
        return;
    }
    ui.label(RichText::new(tx("ferramentas-dica_casa")).small().weak());
    let acoes = egui::ScrollArea::horizontal()
        .id_salt(("casa_rolagem", a.id.as_str()))
        .show(ui, |ui| {
            casa::mostrar(
                ui,
                ("casa", a.id.as_str()),
                &casa::Casa {
                    necessidades: &c.necessidades,
                    requisitos: &c.requisitos,
                    relacoes: &c.relacoes,
                    telhado: &c.telhado,
                    pesos: &c.pesos,
                    editavel: true,
                },
            )
        })
        .inner;
    for acao in acoes {
        match acao {
            casa::Acao::Relacao {
                necessidade,
                requisito,
                valor,
            } => ia::definir_relacao(a, &c, necessidade, requisito, valor),
            casa::Acao::Correlacao { a: x, b, valor } => ia::definir_correlacao(a, &c, x, b, valor),
        }
    }
}

/// A matriz S × O da FMEA com a escolha do critério. Devolve a linha clicada.
pub fn matriz_fmea(
    ui: &mut egui::Ui,
    a: &Aplicacao,
    criterio: &mut Criterio,
    foco: Option<usize>,
) -> Option<usize> {
    let (riscos, _) = calculo::riscos_fmea(&a.tabela);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tx("ferramentas-matriz_s_o")).strong());
        ui.label(tx("ferramentas-cor_por"));
        ui.selectable_value(criterio, Criterio::Ap, "AP")
            .on_hover_text(tx("ferramentas-dica_ap"));
        ui.selectable_value(criterio, Criterio::Npr, "NPR")
            .on_hover_text(tx("ferramentas-dica_npr"));
    });
    let cm = a.tabela.coluna("Modo de falha").unwrap_or(1);
    let pontos: Vec<matriz::Ponto> = riscos
        .iter()
        .map(|r| matriz::Ponto {
            linha: r.linha,
            s: r.s,
            o: r.o,
            nivel: nivel(r, *criterio),
            rotulo: format!(
                "{}: {} (D {}, NPR {}, AP {})",
                r.linha + 1,
                a.tabela.celula(r.linha, cm).trim(),
                r.d,
                r.npr,
                r.ap.letter()
            ),
        })
        .collect();
    let mut clicada = None;
    ui.horizontal(|ui| {
        clicada = matriz::mostrar(ui, ("matriz", a.id.as_str()), &pontos, foco);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(tx("ferramentas-legenda_matriz"))
                    .small()
                    .weak(),
            );
            for (nv, chave) in [
                (Nivel::Alto, "ferramentas-risco_alto"),
                (Nivel::Medio, "ferramentas-risco_medio"),
                (Nivel::Baixo, "ferramentas-risco_baixo"),
            ] {
                let n = pontos.iter().filter(|p| p.nivel == nv).count();
                ui.label(RichText::new(fx(chave, &[("n", vx(&n))])).color(cores::do_nivel(nv)));
            }
        });
    });
    clicada
}

#[cfg(test)]
mod tests {
    use super::*;

    fn risco(s: u8, o: u8, d: u8, ap: Ap) -> RiscoFmea {
        RiscoFmea {
            linha: 0,
            s,
            o,
            d,
            npr: u32::from(s) * u32::from(o) * u32::from(d),
            ap,
        }
    }

    #[test]
    fn a_cor_da_linha_segue_o_criterio() {
        let r = risco(9, 5, 5, Ap::High);
        assert_eq!(nivel(&r, Criterio::Ap), Nivel::Alto);
        assert_eq!(nivel(&r, Criterio::Npr), Nivel::Alto);
        let r = risco(5, 4, 5, Ap::Low);
        assert_eq!(nivel(&r, Criterio::Npr), Nivel::Medio, "NPR 100");
        assert_eq!(nivel(&r, Criterio::Ap), Nivel::Baixo);
        assert_eq!(selo(&r, Criterio::Ap).1, "L · 100");
    }
}
