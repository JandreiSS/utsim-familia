//! Reports of the time study: terminal text, HTML page, SVG of the representative cycle
//! and CSV tables.

use std::fmt::Write as _;

use utsim_model::csv::{num as csvnum, row};

use super::{Comparacao, Estudo, Grupo, LinhaComparacao, Tipo};
use crate::desenho::{curto, pagina, svg_doc, texto as svg_texto};
use crate::text::{esc, num, pct};

fn opt(x: Option<f64>) -> String {
    x.map_or("—".to_string(), num)
}

fn opt_csv(x: Option<f64>) -> String {
    x.map_or(String::new(), csvnum)
}

fn sinal(x: Option<f64>) -> String {
    match x {
        Some(v) if v > 0.0 => format!("+{}", num(v)),
        Some(v) => num(v),
        None => "—".into(),
    }
}

fn sinal_pct(x: Option<f64>) -> String {
    match x {
        Some(v) if v > 0.0 => format!("+{}", pct(v)),
        Some(v) => pct(v),
        None => "—".into(),
    }
}

/// Utilisation-like indicators are fractions.
fn e_fracao(l: &LinhaComparacao) -> bool {
    l.elemento == "utilização"
}

/// Summary for the terminal.
pub fn texto(e: &Estudo, comparacao: Option<&Comparacao>) -> String {
    let u = &e.unidade;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "Estudo de tempos e métodos: {} (replicação {}, t = {} a {})",
        e.modelo,
        e.replicacao,
        num(e.janela.0),
        num(e.janela.1)
    );
    for g in &e.grupos {
        let _ = writeln!(s, "\n== {} ==", g.nome);
        let _ = writeln!(
            s,
            "ciclos: {} encontrados, {} regulares (início de \"{}\")",
            g.ciclos.len(),
            g.regulares(),
            g.ancora
        );
        if let Some(r) = g.tempo_ciclo {
            let _ = writeln!(
                s,
                "tempo de ciclo: média {} {u}, dp {}, mín {}, máx {}",
                num(r.media),
                num(r.dp),
                num(r.min),
                num(r.max)
            );
        }
        if let Some(i) = g.representativo {
            let c = &g.ciclos[i];
            let _ = writeln!(
                s,
                "ciclo representativo: n.º {} (t = {} a {}, {} {u})",
                i + 1,
                num(c.inicio),
                num(c.fim),
                num(c.duracao)
            );
        }
        let _ = writeln!(
            s,
            "\n{:<12} {:<38} {:>9} {:>8} {:>9} {:>9} {:>4}",
            "ator", "elemento", "média", "dp", "mín", "máx", "n"
        );
        for l in &g.elementos {
            let _ = writeln!(
                s,
                "{:<12} {:<38} {:>9} {:>8} {:>9} {:>9} {:>4}",
                curto(&l.ator, 12),
                curto(&l.elemento, 38),
                num(l.resumo.media),
                num(l.resumo.dp),
                num(l.resumo.min),
                num(l.resumo.max),
                l.resumo.n
            );
        }
        let _ = writeln!(
            s,
            "\n{:<12} {:<9} {:>11} {:>16} {:>16}",
            "ator", "papel", "utilização", "ocioso/ciclo", "interf./ciclo"
        );
        for a in &g.atores {
            let _ = writeln!(
                s,
                "{:<12} {:<9} {:>11} {:>16} {:>16}",
                curto(&a.ator, 12),
                a.papel,
                pct(a.utilizacao),
                num(a.ocioso),
                num(a.interferencia)
            );
        }
        if let Some(h) = &g.homem_maquina {
            let _ = writeln!(
                s,
                "\nhomem-máquina: a = {} {u}, b = {} {u}, t = {} {u}; n' = (a + t)/(a + b) = {}",
                num(h.abt.a),
                num(h.abt.b),
                num(h.abt.t),
                num(h.n_linha)
            );
            let _ = writeln!(
                s,
                "ciclo teórico com {} máquina(s): {} {u} (observado: {} {u}; cada máquina esperou o operador {} {u} por ciclo)",
                h.maquinas_atuais,
                num(h.ciclo_teorico),
                g.tempo_ciclo.map_or("—".into(), |r| num(r.media)),
                num(h.espera_maquina)
            );
            for a in &h.arranjos {
                let _ = writeln!(
                    s,
                    "  n = {}: ciclo {} {u}, operador {}, máquina {}{}",
                    a.maquinas,
                    num(a.ciclo),
                    pct(a.utilizacao_operador),
                    pct(a.utilizacao_maquina),
                    a.custo_por_unidade
                        .map_or(String::new(), |c| format!(", custo/unidade {}", num(c)))
                );
            }
            if let Some(o) = &h.otimo {
                let _ = writeln!(s, "  ótimo pelo custo: n = {}", o.maquinas);
            }
        }
        if let Some(p) = &g.padrao {
            let _ = writeln!(
                s,
                "\ntempo padrão: ritmo {}, tolerância {} ({}); ciclo observado {} {u}, normal {}, padrão {}",
                pct(p.ritmo),
                pct(p.tolerancia),
                p.origem_tolerancia,
                num(p.ciclo_observado),
                num(p.ciclo_normal),
                num(p.ciclo_padrao)
            );
            let _ = writeln!(
                s,
                "por unidade ({} unidade(s) por ciclo): observado {}, padrão {} {u}",
                num(p.unidades_por_ciclo),
                opt(p.por_unidade_observado),
                opt(p.por_unidade_padrao)
            );
        }
    }
    if let Some(c) = comparacao {
        let _ = writeln!(
            s,
            "\n== Comparação: {} (antes) × {} (depois) ==",
            c.antes, c.depois
        );
        let _ = writeln!(
            s,
            "{:<12} {:<38} {:>9} {:>9} {:>9} {:>9}",
            "ator", "elemento", "antes", "depois", "dif.", "var."
        );
        for l in c.elementos.iter().chain(&c.indicadores) {
            let f = e_fracao(l);
            let v = |x: Option<f64>| {
                if f {
                    x.map_or("—".into(), pct)
                } else {
                    opt(x)
                }
            };
            let _ = writeln!(
                s,
                "{:<12} {:<38} {:>9} {:>9} {:>9} {:>9}",
                curto(&l.ator, 12),
                curto(&l.elemento, 38),
                v(l.antes),
                v(l.depois),
                if f {
                    sinal(l.diferenca().map(|d| d * 100.0)) + " p.p."
                } else {
                    sinal(l.diferenca())
                },
                sinal_pct(l.variacao())
            );
        }
    }
    s
}

/// Multiple-activity chart of the representative cycle: one column per actor, time going
/// down on a common axis, every element with its name and duration.
pub fn svg(g: &Grupo, unidade: &str, standalone: bool) -> Option<String> {
    let i = g.representativo?;
    let ciclo = g.ciclos[i].duracao;
    if ciclo <= 0.0 {
        return None;
    }
    let (colw, gap, left, top, hgt) = (150.0, 16.0, 64.0, 56.0, 620.0);
    let width = left + g.grafico.len() as f64 * (colw + gap) + 16.0;
    let height = top + hgt + 40.0;
    let y_of = |t: f64| top + hgt * t / ciclo;
    let mut b = String::new();
    let passos = 10;
    for k in 0..=passos {
        let t = ciclo * k as f64 / passos as f64;
        let y = y_of(t);
        let _ = write!(
            b,
            "<line class=\"eixo\" x1=\"{:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" style=\"stroke-opacity:.3\"/>{}",
            left - 6.0,
            width - 10.0,
            svg_texto(6.0, y + 4.0, "etq mut", &num(t))
        );
    }
    let _ = write!(
        b,
        "{}",
        svg_texto(6.0, top - 34.0, "etq mut", &format!("tempo ({unidade})"))
    );
    for (k, col) in g.grafico.iter().enumerate() {
        let x = left + k as f64 * (colw + gap);
        for barra in &col.barras {
            let (y0, y1) = (y_of(barra.t0), y_of(barra.t1));
            let h = (y1 - y0).max(0.3);
            let _ = write!(
                b,
                "<rect x=\"{x:.1}\" y=\"{y0:.1}\" width=\"{colw}\" height=\"{h:.2}\" style=\"fill:{}\"><title>{}: {} {unidade}</title></rect>",
                barra.tipo.cor(),
                esc(&barra.elemento),
                num(barra.t1 - barra.t0)
            );
            if h >= 13.0 {
                let _ = write!(
                    b,
                    "<text class=\"etq\" x=\"{:.1}\" y=\"{:.1}\" style=\"fill:{}\">{}</text>",
                    x + 4.0,
                    y0 + h / 2.0 + 3.5,
                    // Dark text on the light grey of idle time, white on the rest.
                    if barra.tipo == Tipo::Ocioso {
                        "#111827"
                    } else {
                        "#fff"
                    },
                    esc(&format!(
                        "{} ({})",
                        curto(&barra.elemento, 20),
                        num(barra.t1 - barra.t0)
                    ))
                );
            }
        }
        let _ = write!(
            b,
            "<rect x=\"{x:.1}\" y=\"{top}\" width=\"{colw}\" height=\"{hgt}\" class=\"fino\"/>{}",
            svg_texto(x, top - 10.0, "etq b", &curto(&col.ator, 22))
        );
    }
    let _ = write!(
        b,
        "{}",
        svg_texto(
            left,
            top + hgt + 24.0,
            "etq",
            &format!("ciclo n.º {}: {} {unidade}", i + 1, num(ciclo))
        )
    );
    Some(svg_doc(
        width,
        height,
        &format!("Ciclo representativo: {}", g.nome),
        &b,
        standalone,
    ))
}

fn legenda() -> String {
    let mut leg = String::from("<div class=\"legenda\">");
    for t in Tipo::ALL {
        let _ = write!(
            leg,
            "<span><i style=\"display:inline-block;width:.9rem;height:.9rem;background:{}\"></i>{}</span>",
            t.cor(),
            esc(t.nome())
        );
    }
    leg.push_str("</div>");
    leg
}

fn tabela_comparacao(c: &Comparacao, u: &str) -> String {
    let mut h = format!(
        "<h2>Comparação de métodos</h2><p class=\"muted\">Antes: {} ({}). Depois: {} ({}). \
         Elementos pareados pelo nome do ator e do elemento; tempos médios por ciclo, em {u}.</p>\
         <table><thead><tr><th>Ator</th><th>Elemento ou indicador</th><th class=\"n\">Antes</th>\
         <th class=\"n\">Depois</th><th class=\"n\">Diferença</th><th class=\"n\">Variação</th></tr></thead><tbody>",
        esc(&c.antes),
        esc(&c.grupo_antes),
        esc(&c.depois),
        esc(&c.grupo_depois)
    );
    for (k, l) in c.elementos.iter().chain(&c.indicadores).enumerate() {
        let f = e_fracao(l);
        let v = |x: Option<f64>| {
            if f {
                x.map_or("—".into(), pct)
            } else {
                opt(x)
            }
        };
        let dif = if f {
            l.diferenca()
                .map_or("—".into(), |d| format!("{} p.p.", sinal(Some(d * 100.0))))
        } else {
            sinal(l.diferenca())
        };
        let classe = if k == c.elementos.len() {
            " class=\"total\""
        } else {
            ""
        };
        let _ = write!(
            h,
            "<tr{classe}><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
             <td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            esc(&l.ator),
            esc(&l.elemento),
            v(l.antes),
            v(l.depois),
            dif,
            sinal_pct(l.variacao())
        );
    }
    h.push_str("</tbody></table>");
    h
}

/// The whole report as an HTML page.
pub fn html(e: &Estudo, comparacao: Option<&Comparacao>, origem: &str) -> String {
    let u = &e.unidade;
    let mut b = String::new();
    let _ = write!(
        b,
        "<p>Intervalo analisado: t = {} a t = {} (replicação {}). Cada ciclo começa no início do \
         elemento-âncora; ciclos irregulares (outra ordem de atendimento, quebra, pausa) ficam \
         fora das estatísticas.</p>{}",
        num(e.janela.0),
        num(e.janela.1),
        e.replicacao,
        legenda()
    );
    for g in &e.grupos {
        let _ = write!(b, "<h2>{}</h2>", esc(&g.nome));
        let _ = write!(
            b,
            "<p>{} ciclo(s) encontrado(s), <b>{} regular(es)</b>; âncora: início de \"{}\".",
            g.ciclos.len(),
            g.regulares(),
            esc(&g.ancora)
        );
        if let Some(r) = g.tempo_ciclo {
            let _ = write!(
                b,
                " Tempo de ciclo: média <b>{} {u}</b>, desvio-padrão {}, mínimo {}, máximo {}.",
                num(r.media),
                num(r.dp),
                num(r.min),
                num(r.max)
            );
        }
        b.push_str("</p>");
        if let Some(s) = svg(g, u, false) {
            let _ = write!(b, "<div class=\"rolagem\">{s}</div>");
        }
        b.push_str(
            "<h3>Elementos</h3><p class=\"muted\">Tempo por ciclo de cada elemento (somado quando \
             acontece mais de uma vez no ciclo), nos ciclos regulares em que aparece (n). Normal = \
             observado × ritmo; padrão = normal × (1 + tolerância); tempos de máquina não levam \
             ritmo nem tolerância.</p><table><thead><tr><th>Ator</th><th>Elemento</th><th>Tipo</th>\
             <th class=\"n\">Média</th><th class=\"n\">Desvio</th><th class=\"n\">Mín.</th>\
             <th class=\"n\">Máx.</th><th class=\"n\">n</th><th class=\"n\">Normal</th>\
             <th class=\"n\">Padrão</th></tr></thead><tbody>",
        );
        for l in &g.elementos {
            let _ = write!(
                b,
                "<tr><td>{}</td><td><i style=\"display:inline-block;width:.7rem;height:.7rem;background:{}\"></i> {}</td>\
                 <td class=\"muted\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
                 <td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                esc(&l.ator),
                l.tipo.cor(),
                esc(&l.elemento),
                esc(l.tipo.nome()),
                num(l.resumo.media),
                num(l.resumo.dp),
                num(l.resumo.min),
                num(l.resumo.max),
                l.resumo.n,
                opt(l.normal),
                opt(l.padrao)
            );
        }
        b.push_str("</tbody></table>");
        b.push_str(
            "<h3>Atores</h3><table><thead><tr><th>Ator</th><th>Papel</th><th class=\"n\">Ocupado por ciclo</th>\
             <th class=\"n\">Utilização</th><th class=\"n\">Ocioso por ciclo</th>\
             <th class=\"n\">Interferência por ciclo</th></tr></thead><tbody>",
        );
        for a in &g.atores {
            let _ = write!(
                b,
                "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\"><b>{}</b></td>\
                 <td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                esc(&a.ator),
                esc(&a.papel),
                num(a.ocupado),
                pct(a.utilizacao),
                num(a.ocioso),
                num(a.interferencia)
            );
        }
        b.push_str(
            "</tbody></table><p class=\"muted\">Interferência: na máquina, o tempo parada \
             esperando o operador; no operador, o tempo ocioso enquanto uma das suas máquinas \
             trabalha sozinha (esperando a máquina).</p>",
        );
        if let Some(h) = &g.homem_maquina {
            let _ = write!(
                b,
                "<h3>Quantas máquinas por operador</h3><p>Medido nos ciclos regulares, por máquina: \
                 <b>a</b> = {} {u} (operador e máquina juntos: carga, descarga), <b>b</b> = {} {u} \
                 (trabalho independente do operador, caminhada incluída), <b>t</b> = {} {u} \
                 (máquina sozinha). <b>n' = (a + t)/(a + b) = {}</b>. Com as {} máquina(s) atuais o \
                 ciclo teórico é {} {u}; o observado é {} {u}. A diferença vem da interferência \
                 (cada máquina esperou o operador {} {u} por ciclo: no modelo o operador só sai \
                 quando a máquina chama) e da variação dos tempos; essa espera entra nos ciclos \
                 normal e padrão como observada.</p>",
                num(h.abt.a),
                num(h.abt.b),
                num(h.abt.t),
                num(h.n_linha),
                h.maquinas_atuais,
                num(h.ciclo_teorico),
                g.tempo_ciclo.map_or("—".into(), |r| num(r.media)),
                num(h.espera_maquina)
            );
            b.push_str(
                "<table><thead><tr><th class=\"n\">Máquinas (n)</th><th class=\"n\">Ciclo</th>\
                 <th class=\"n\">Utilização do operador</th><th class=\"n\">Utilização da máquina</th>\
                 <th class=\"n\">Operador ocioso por ciclo</th><th class=\"n\">Máquina ociosa por ciclo</th>\
                 <th class=\"n\">Custo por unidade</th></tr></thead><tbody>",
            );
            for a in &h.arranjos {
                let classe = if h.otimo.as_ref().is_some_and(|o| o.maquinas == a.maquinas) {
                    " class=\"total\""
                } else {
                    ""
                };
                let _ = write!(
                    b,
                    "<tr{classe}><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
                     <td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                    a.maquinas,
                    num(a.ciclo),
                    pct(a.utilizacao_operador),
                    pct(a.utilizacao_maquina),
                    num(a.operador_ocioso),
                    num(a.maquina_ociosa),
                    opt(a.custo_por_unidade)
                );
            }
            b.push_str("</tbody></table>");
            match &h.otimo {
                Some(o) => {
                    let _ = write!(
                        b,
                        "<p>Ótimo pelo custo por unidade, (K1 + n·K2)·ciclo/n: <b>n = {}</b> (linha \
                         destacada).</p>",
                        o.maquinas
                    );
                }
                None => b.push_str(
                    "<p class=\"muted\">Informe --custo-operador e --custo-maquina (por hora) para \
                     escolher n pelo custo por unidade.</p>",
                ),
            }
        }
        if let Some(p) = &g.padrao {
            let _ = write!(
                b,
                "<h3>Tempo padrão</h3><p>Ritmo {}; tolerância (PF&amp;D) {}: {}.",
                pct(p.ritmo),
                pct(p.tolerancia),
                esc(&p.origem_tolerancia)
            );
            if p.tolerancia_no_modelo > 0.0 {
                let _ = write!(
                    b,
                    " Os tempos de operação simulados já incluem a tolerância de {} do perfil da \
                     pessoa; ela é tirada antes do ritmo.",
                    pct(p.tolerancia_no_modelo)
                );
            }
            let _ = write!(
                b,
                "</p><table><tbody><tr><td>Ciclo observado</td><td class=\"n\">{} {u}</td></tr>\
                 <tr><td>Ciclo normal</td><td class=\"n\">{} {u}</td></tr>\
                 <tr class=\"total\"><td>Ciclo padrão</td><td class=\"n\">{} {u}</td></tr>\
                 <tr><td>Unidades por ciclo</td><td class=\"n\">{}</td></tr>\
                 <tr><td>Tempo observado por unidade</td><td class=\"n\">{} {u}</td></tr>\
                 <tr class=\"total\"><td>Tempo padrão por unidade</td><td class=\"n\">{} {u}</td></tr>\
                 </tbody></table>",
                num(p.ciclo_observado),
                num(p.ciclo_normal),
                num(p.ciclo_padrao),
                num(p.unidades_por_ciclo),
                opt(p.por_unidade_observado),
                opt(p.por_unidade_padrao)
            );
        }
        b.push_str(
            "<details><summary>Ciclos encontrados</summary><table><thead><tr><th class=\"n\">N.º</th>\
             <th class=\"n\">Início</th><th class=\"n\">Fim</th><th class=\"n\">Duração</th>\
             <th>Regular</th><th>Sequência de trabalho</th></tr></thead><tbody>",
        );
        for (k, c) in g.ciclos.iter().enumerate() {
            let _ = write!(
                b,
                "<tr><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
                 <td class=\"n\">{}</td><td>{}</td><td class=\"muted\">{}</td></tr>",
                k + 1,
                num(c.inicio),
                num(c.fim),
                num(c.duracao),
                if c.regular { "sim" } else { "não" },
                esc(&c.assinatura)
            );
        }
        b.push_str("</tbody></table></details>");
    }
    if let Some(c) = comparacao {
        b.push_str(&tabela_comparacao(c, u));
    }
    pagina(
        &format!("Estudo de tempos e métodos: {}", e.modelo),
        "Gráfico homem-máquina do ciclo representativo, tempos por elemento, número de máquinas \
         por operador e tempo padrão, a partir do rastro da simulação.",
        &b,
        origem,
    )
}

/// The element table as CSV (one line per group, actor and element).
pub fn csv_elementos(e: &Estudo) -> String {
    let mut s = String::from("grupo,ator,elemento,tipo,media,dp,min,max,n,normal,padrao\n");
    for g in &e.grupos {
        for l in &g.elementos {
            row(
                &mut s,
                &[
                    g.nome.clone(),
                    l.ator.clone(),
                    l.elemento.clone(),
                    l.tipo.id().to_string(),
                    csvnum(l.resumo.media),
                    csvnum(l.resumo.dp),
                    csvnum(l.resumo.min),
                    csvnum(l.resumo.max),
                    l.resumo.n.to_string(),
                    opt_csv(l.normal),
                    opt_csv(l.padrao),
                ],
            );
        }
    }
    s
}

/// The comparison as CSV.
pub fn csv_comparacao(c: &Comparacao) -> String {
    let mut s = String::from("ator,elemento,antes,depois,diferenca,variacao\n");
    for l in c.elementos.iter().chain(&c.indicadores) {
        row(
            &mut s,
            &[
                l.ator.clone(),
                l.elemento.clone(),
                opt_csv(l.antes),
                opt_csv(l.depois),
                opt_csv(l.diferenca()),
                opt_csv(l.variacao()),
            ],
        );
    }
    s
}
