//! O resultado de cada ferramenta: indicadores, gráficos, tabelas, achados e avisos. A
//! tela mostra o mesmo [`Resultado`] que o relatório imprime.
//!
//! As colunas lidas vêm de [`Parametros::colunas`](crate::Parametros), por índice, com
//! estes padrões quando faltam:
//!
//! | Ferramenta | Colunas |
//! |---|---|
//! | Folha de verificação | categoria; as demais são os períodos |
//! | Estratificação | estrato, valor |
//! | Pareto | categoria, quantidade, custo unitário (opcional) |
//! | Histograma, capacidade | valor |
//! | Dispersão | X, Y |
//! | Carta de controle | subgrupo (rótulo), valor; nas de atributos: amostra, contagem, tamanho |
//! | MSA | peça, operador, valor |
//! | FMEA | as de [`COLUNAS_FMEA`] |
//! | QFD, tolerâncias, poka-yoke | pelo nome, ver [`projeto`] |

use utsim_ferramentas::ap::{Ap, ApTable};
use utsim_ferramentas::relatorios::{Chart, ChartKind, Kpi, Tone};
use utsim_quality::capability::{self, Options, Spec};
use utsim_quality::charts::{self, ChartData, ChartKind as Carta};
use utsim_quality::msa::{self, GageData, GageOptions, Measurement};
use utsim_quality::rules::RuleSet;

use crate::Tabela;
use crate::tabela::numero;
use crate::{Aplicacao, Ferramenta, num};

pub mod projeto;

pub use projeto::{
    COLUNAS_POKA_YOKE, COLUNAS_QFD, COLUNAS_QFD_REQUISITOS, COLUNAS_QFD_TELHADO,
    COLUNAS_TOLERANCIAS, COLUNAS_VERIFICACOES,
};

/// As colunas da FMEA, na ordem da tabela.
pub const COLUNAS_FMEA: [&str; 10] = [
    "Etapa",
    "Modo de falha",
    "Efeito",
    "S",
    "Causa",
    "O",
    "Prevenção",
    "Detecção",
    "D",
    "Ação recomendada",
];

/// Uma espinha do Ishikawa: a categoria e as causas.
pub type Espinha = (String, Vec<String>);

/// Uma tabela do resultado.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quadro {
    pub nome: String,
    pub cabecalho: Vec<String>,
    pub linhas: Vec<Vec<String>>,
    /// Larguras relativas das colunas (vazio divide igual).
    pub larguras: Vec<f64>,
}

/// Um achado em destaque: o que o resultado mostra.
#[derive(Clone, Debug, PartialEq)]
pub struct Achado {
    pub tom: Tone,
    pub titulo: String,
    pub texto: String,
}

/// O resultado de uma aplicação.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resultado {
    pub indicadores: Vec<Kpi>,
    pub graficos: Vec<Chart>,
    pub quadros: Vec<Quadro>,
    /// O Ishikawa: efeito e `(categoria, causas)`.
    pub ishikawa: Option<(String, Vec<Espinha>)>,
    /// Seções de texto (A3) ou cadeias (5 porquês): `(título, linhas)`.
    pub textos: Vec<(String, Vec<String>)>,
    pub achados: Vec<Achado>,
    /// O que impediu o cálculo ou foi deixado de lado (células que não são número...).
    pub avisos: Vec<String>,
}

impl Resultado {
    /// Se não há nada para mostrar além de avisos.
    pub fn vazio(&self) -> bool {
        self.indicadores.is_empty()
            && self.graficos.is_empty()
            && self.quadros.is_empty()
            && self.ishikawa.is_none()
            && self.textos.is_empty()
    }

    fn kpi(&mut self, rotulo: &str, valor: String, nota: &str) {
        self.indicadores.push(Kpi {
            label: rotulo.into(),
            value: valor,
            note: nota.into(),
        });
    }

    fn achado(&mut self, tom: Tone, titulo: &str, texto: String) {
        self.achados.push(Achado {
            tom,
            titulo: titulo.into(),
            texto,
        });
    }

    fn quadro(
        &mut self,
        nome: &str,
        cabecalho: &[&str],
        linhas: Vec<Vec<String>>,
        larguras: &[f64],
    ) {
        self.quadros.push(Quadro {
            nome: nome.into(),
            cabecalho: cabecalho.iter().map(|s| s.to_string()).collect(),
            linhas,
            larguras: larguras.to_vec(),
        });
    }
}

/// O índice da coluna `i` dos parâmetros, ou o padrão.
fn col(a: &Aplicacao, i: usize, padrao: usize) -> usize {
    a.parametros.colunas.get(i).copied().unwrap_or(padrao)
}

fn nome_col(a: &Aplicacao, c: usize) -> String {
    a.tabela
        .colunas
        .get(c)
        .cloned()
        .unwrap_or_else(|| format!("Coluna {}", c + 1))
}

/// Avisa das células que não são número numa coluna lida como número.
fn avisar_invalidas(a: &Aplicacao, r: &mut Resultado, c: usize) {
    let inv = a.tabela.invalidas(c);
    if !inv.is_empty() {
        let linhas: Vec<String> = inv.iter().take(5).map(|i| (i + 1).to_string()).collect();
        r.avisos.push(format!(
            "{} {} da coluna {} não {} número e {} de fora (linha {}{}).",
            inv.len(),
            if inv.len() == 1 {
                "célula"
            } else {
                "células"
            },
            nome_col(a, c),
            if inv.len() == 1 { "é" } else { "são" },
            if inv.len() == 1 { "ficou" } else { "ficaram" },
            linhas.join(", "),
            if inv.len() > 5 { "..." } else { "" }
        ));
    }
}

fn media(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn desvio(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return f64::NAN;
    }
    let m = media(xs);
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0)).sqrt()
}

fn minmax(xs: &[f64]) -> (f64, f64) {
    xs.iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &x| {
            (lo.min(x), hi.max(x))
        })
}

/// Calcula a aplicação. `problema` é o do estudo, usado quando a aplicação não tem efeito.
pub fn calcular(a: &Aplicacao, problema: &str) -> Resultado {
    let mut r = Resultado::default();
    if a.ferramenta.usa_tabela() && a.tabela.vazia() {
        r.avisos.push(
            "A tabela está vazia: digite os dados, importe um CSV ou puxe da simulação.".into(),
        );
        return r;
    }
    match a.ferramenta {
        Ferramenta::FolhaVerificacao => folha(a, &mut r),
        Ferramenta::Estratificacao => estratificacao(a, &mut r),
        Ferramenta::Pareto => pareto(a, &mut r),
        Ferramenta::Histograma => histograma(a, &mut r),
        Ferramenta::Dispersao => dispersao(a, &mut r),
        Ferramenta::CartaControle => carta(a, &mut r),
        Ferramenta::Capacidade => capacidade(a, &mut r),
        Ferramenta::Msa => msa_rr(a, &mut r),
        Ferramenta::Ishikawa => ishikawa(a, problema, &mut r),
        Ferramenta::CincoPorques => porques(a, problema, &mut r),
        Ferramenta::Fmea => fmea(a, &mut r),
        Ferramenta::A3 => a3(a, &mut r),
        Ferramenta::Qfd => projeto::qfd(a, &mut r),
        Ferramenta::Tolerancias => projeto::tolerancias(a, &mut r),
        Ferramenta::PokaYoke => projeto::poka_yoke(a, &mut r),
    }
    r
}

fn folha(a: &Aplicacao, r: &mut Resultado) {
    let c = col(a, 0, 0);
    let periodos: Vec<usize> = (0..a.tabela.colunas.len()).filter(|&i| i != c).collect();
    if periodos.is_empty() {
        r.avisos
            .push("A folha precisa de ao menos uma coluna de contagem.".into());
        return;
    }
    for &p in &periodos {
        avisar_invalidas(a, r, p);
    }
    let mut linhas: Vec<(String, Vec<f64>)> = Vec::new();
    for l in &a.tabela.linhas {
        let nome = l.get(c).map_or("", |s| s.trim()).to_string();
        if nome.is_empty() {
            continue;
        }
        let v: Vec<f64> = periodos
            .iter()
            .map(|&p| l.get(p).and_then(|s| numero(s)).unwrap_or(0.0))
            .collect();
        match linhas
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(&nome))
        {
            Some((_, acc)) => acc.iter_mut().zip(&v).for_each(|(x, y)| *x += y),
            None => linhas.push((nome, v)),
        }
    }
    if linhas.is_empty() {
        r.avisos.push("Nenhuma categoria preenchida.".into());
        return;
    }
    let total_cat: Vec<f64> = linhas.iter().map(|(_, v)| v.iter().sum()).collect();
    let total: f64 = total_cat.iter().sum();
    let por_periodo: Vec<f64> = (0..periodos.len())
        .map(|j| linhas.iter().map(|(_, v)| v[j]).sum())
        .collect();
    let mut ordem: Vec<usize> = (0..linhas.len()).collect();
    ordem.sort_by(|&x, &y| total_cat[y].total_cmp(&total_cat[x]));
    let maior = ordem[0];
    r.kpi("Total de ocorrências", num(total, 2), "");
    r.kpi("Categorias", linhas.len().to_string(), "");
    r.kpi(
        "Mais frequente",
        linhas[maior].0.clone(),
        &format!(
            "{} ({} %)",
            num(total_cat[maior], 2),
            num(100.0 * total_cat[maior] / total.max(f64::MIN_POSITIVE), 1)
        ),
    );
    r.graficos.push(Chart {
        title: "Ocorrências por categoria e período".into(),
        bars: true,
        labels: ordem.iter().map(|&i| linhas[i].0.clone()).collect(),
        series: periodos
            .iter()
            .enumerate()
            .map(|(j, &p)| {
                (
                    nome_col(a, p),
                    ordem.iter().map(|&i| linhas[i].1[j]).collect(),
                )
            })
            .collect(),
        kind: ChartKind::Bars,
        ..Chart::default()
    });
    let mut cab: Vec<String> = vec![nome_col(a, c)];
    cab.extend(periodos.iter().map(|&p| nome_col(a, p)));
    cab.push("Total".into());
    let mut rows: Vec<Vec<String>> = ordem
        .iter()
        .map(|&i| {
            let mut row = vec![linhas[i].0.clone()];
            row.extend(linhas[i].1.iter().map(|v| num(*v, 2)));
            row.push(num(total_cat[i], 2));
            row
        })
        .collect();
    let mut tot = vec!["Total".to_string()];
    tot.extend(por_periodo.iter().map(|v| num(*v, 2)));
    tot.push(num(total, 2));
    rows.push(tot);
    let cab_ref: Vec<&str> = cab.iter().map(String::as_str).collect();
    let mut larg = vec![2.0];
    larg.extend(std::iter::repeat_n(1.0, periodos.len() + 1));
    r.quadro("Folha de verificação", &cab_ref, rows, &larg);
    if periodos.len() > 1 {
        let (jmax, vmax) =
            por_periodo
                .iter()
                .enumerate()
                .fold(
                    (0, f64::NEG_INFINITY),
                    |acc, (j, &v)| if v > acc.1 { (j, v) } else { acc },
                );
        let m = media(&por_periodo);
        if m > 0.0 && vmax > 1.5 * m {
            r.achado(
                Tone::Warn,
                "Período fora do padrão",
                format!(
                    "{} concentrou {} ocorrências, {} % acima da média dos períodos.",
                    nome_col(a, periodos[jmax]),
                    num(vmax, 2),
                    num(100.0 * (vmax / m - 1.0), 0)
                ),
            );
        }
    }
}

fn estratificacao(a: &Aplicacao, r: &mut Resultado) {
    let (ce, cv) = (col(a, 0, 0), col(a, 1, 1));
    avisar_invalidas(a, r, cv);
    let mut grupos: Vec<(String, Vec<f64>)> = Vec::new();
    for l in &a.tabela.linhas {
        let (Some(e), Some(v)) = (l.get(ce), l.get(cv).and_then(|s| numero(s))) else {
            continue;
        };
        let e = if e.trim().is_empty() {
            "(sem estrato)"
        } else {
            e.trim()
        };
        match grupos.iter_mut().find(|(n, _)| n == e) {
            Some((_, xs)) => xs.push(v),
            None => grupos.push((e.to_string(), vec![v])),
        }
    }
    if grupos.is_empty() {
        r.avisos.push("Nenhum valor numérico com estrato.".into());
        return;
    }
    let todos: Vec<f64> = grupos.iter().flat_map(|(_, x)| x.iter().copied()).collect();
    let mg = media(&todos);
    r.kpi("Estratos", grupos.len().to_string(), "");
    r.kpi("Valores", todos.len().to_string(), "");
    r.kpi("Média geral", num(mg, 3), &nome_col(a, cv));
    r.graficos.push(Chart {
        title: format!("Média de {} por {}", nome_col(a, cv), nome_col(a, ce)),
        bars: true,
        labels: grupos.iter().map(|(n, _)| n.clone()).collect(),
        series: vec![(
            "média".into(),
            grupos.iter().map(|(_, x)| media(x)).collect(),
        )],
        hlines: vec![("média geral".into(), mg)],
        kind: ChartKind::Bars,
        ..Chart::default()
    });
    let rows = grupos
        .iter()
        .map(|(n, x)| {
            let (lo, hi) = minmax(x);
            vec![
                n.clone(),
                x.len().to_string(),
                num(media(x), 3),
                num(desvio(x), 3),
                num(lo, 3),
                num(hi, 3),
                num(x.iter().sum(), 3),
            ]
        })
        .collect();
    r.quadro(
        "Resumo por estrato",
        &[
            "Estrato", "n", "Média", "Desvio", "Mínimo", "Máximo", "Soma",
        ],
        rows,
        &[2.0, 0.7, 1.0, 1.0, 1.0, 1.0, 1.0],
    );
    if grupos.len() > 1 {
        let dp = desvio(&todos);
        if let Some((n, x)) = grupos.iter().max_by(|p, q| {
            (media(&p.1) - mg)
                .abs()
                .total_cmp(&(media(&q.1) - mg).abs())
        }) && dp > 0.0
            && (media(x) - mg).abs() > dp
        {
            r.achado(
                Tone::Warn,
                "Estrato que se destaca",
                format!(
                    "{n} tem média {} contra {} no geral, mais de um desvio padrão de diferença.",
                    num(media(x), 3),
                    num(mg, 3)
                ),
            );
        }
    }
}

/// O Pareto da tabela de uma aplicação (o mesmo do resultado) e a coluna das categorias:
/// a tela desenha o gráfico vivo com ele e filtra a tabela pela barra escolhida.
pub fn pareto_da_tabela(a: &Aplicacao) -> (utsim_ferramentas::pareto::Pareto, usize) {
    let (cc, cq) = (col(a, 0, 0), col(a, 1, 1));
    let entrada: Vec<(String, f64)> = a
        .tabela
        .linhas
        .iter()
        .filter_map(|l| {
            let n = l.get(cc)?.trim();
            (!n.is_empty()).then(|| {
                (
                    n.to_string(),
                    l.get(cq).and_then(|s| numero(s)).unwrap_or(0.0),
                )
            })
        })
        .collect();
    (
        utsim_ferramentas::pareto::pareto(&entrada, a.parametros.outros_abaixo),
        cc,
    )
}

fn pareto(a: &Aplicacao, r: &mut Resultado) {
    let (cc, cq) = (col(a, 0, 0), col(a, 1, 1));
    let cu = a
        .parametros
        .colunas
        .get(2)
        .copied()
        .or_else(|| (a.tabela.colunas.len() > 2).then_some(2));
    avisar_invalidas(a, r, cq);
    let (p, _) = pareto_da_tabela(a);
    if p.total <= 0.0 {
        r.avisos.push("As quantidades somam zero.".into());
        return;
    }
    let vitais: Vec<&str> = p.vital().map(|v| v.name.as_str()).collect();
    r.kpi("Total", num(p.total, 2), &nome_col(a, cq));
    r.kpi("Categorias", p.rows.len().to_string(), "");
    r.kpi(
        "Poucas vitais",
        format!("{} de {}", vitais.len(), p.rows.len()),
        &format!(
            "{} % do total",
            num(p.vital().last().map_or(0.0, |v| v.cum_pct), 1)
        ),
    );
    r.graficos.push(Chart {
        title: format!("Pareto de {}", nome_col(a, cq).to_lowercase()),
        labels: p.rows.iter().map(|v| v.name.clone()).collect(),
        series: vec![(
            nome_col(a, cq).to_lowercase(),
            p.rows.iter().map(|v| v.value).collect(),
        )],
        kind: ChartKind::Pareto,
        ..Chart::default()
    });
    let rows = p
        .rows
        .iter()
        .map(|v| {
            vec![
                v.name.clone(),
                num(v.value, 2),
                num(v.pct, 1),
                num(v.cum_pct, 1),
                if v.vital { "sim".into() } else { String::new() },
            ]
        })
        .collect();
    r.quadro(
        "Categorias em ordem",
        &["Categoria", "Quantidade", "%", "% acumulado", "Vital"],
        rows,
        &[2.4, 1.0, 0.8, 1.0, 0.7],
    );
    r.achado(
        Tone::Info,
        "Onde atacar primeiro",
        format!(
            "{} {} {} % das ocorrências.",
            vitais.join(", "),
            if vitais.len() == 1 {
                "responde por"
            } else {
                "respondem por"
            },
            num(p.vital().last().map_or(0.0, |v| v.cum_pct), 1)
        ),
    );
    // Com custo unitário, o Pareto do custo mostra se a ordem muda.
    if let Some(cu) = cu.filter(|&c| !a.tabela.numeros(c).is_empty()) {
        let custo: Vec<(String, f64)> = a
            .tabela
            .linhas
            .iter()
            .filter_map(|l| {
                let n = l.get(cc)?.trim();
                let q = l.get(cq).and_then(|s| numero(s)).unwrap_or(0.0);
                let u = l.get(cu).and_then(|s| numero(s)).unwrap_or(0.0);
                (!n.is_empty()).then(|| (n.to_string(), q * u))
            })
            .collect();
        let pc = utsim_ferramentas::pareto::pareto(&custo, a.parametros.outros_abaixo);
        if pc.total > 0.0 {
            r.kpi(
                "Custo total",
                num(pc.total, 2),
                "quantidade × custo unitário",
            );
            r.graficos.push(Chart {
                title: "Pareto do custo".into(),
                labels: pc.rows.iter().map(|v| v.name.clone()).collect(),
                series: vec![("custo".into(), pc.rows.iter().map(|v| v.value).collect())],
                kind: ChartKind::Pareto,
                ..Chart::default()
            });
            if pc.rows.first().map(|v| &v.name) != p.rows.first().map(|v| &v.name) {
                r.achado(
                    Tone::Warn,
                    "O custo muda a prioridade",
                    format!(
                        "A categoria mais frequente é {}, mas a mais cara é {}.",
                        p.rows[0].name, pc.rows[0].name
                    ),
                );
            }
        }
    }
}

fn limites_linhas(a: &Aplicacao) -> Vec<(String, f64)> {
    let p = &a.parametros;
    [("LIE", p.lie), ("LSE", p.lse), ("Alvo", p.alvo)]
        .into_iter()
        .filter_map(|(n, v)| v.map(|v| (n.to_string(), v)))
        .collect()
}

fn histograma(a: &Aplicacao, r: &mut Resultado) {
    let c = col(a, 0, 0);
    avisar_invalidas(a, r, c);
    let xs = a.tabela.numeros(c);
    if xs.len() < 2 {
        r.avisos
            .push("O histograma precisa de ao menos 2 valores.".into());
        return;
    }
    let (lo, hi) = minmax(&xs);
    let (m, s) = (media(&xs), desvio(&xs));
    r.kpi("n", xs.len().to_string(), "");
    r.kpi("Média", num(m, 4), "");
    r.kpi("Desvio padrão", num(s, 4), "");
    r.kpi("Mínimo", num(lo, 4), "");
    r.kpi("Máximo", num(hi, 4), "");
    let p = &a.parametros;
    let fora = xs
        .iter()
        .filter(|&&x| p.lie.is_some_and(|l| x < l) || p.lse.is_some_and(|u| x > u))
        .count();
    if p.lie.is_some() || p.lse.is_some() {
        r.kpi(
            "Fora da especificação",
            fora.to_string(),
            &format!("{} %", num(100.0 * fora as f64 / xs.len() as f64, 2)),
        );
        if fora > 0 {
            r.achado(
                Tone::Danger,
                "Valores fora da especificação",
                format!("{fora} de {} valores estão fora dos limites.", xs.len()),
            );
        } else if let (Some(l), Some(u)) = (p.lie, p.lse)
            && s > 0.0
            && (u - l) < 6.0 * s
        {
            r.achado(
                Tone::Warn,
                "Pouca folga",
                "Todos os valores estão dentro, mas a tolerância é menor que seis desvios: confira a capacidade.".into(),
            );
        }
    }
    r.graficos.push(Chart {
        title: format!("Histograma de {}", nome_col(a, c).to_lowercase()),
        series: vec![(nome_col(a, c).to_lowercase(), xs)],
        hlines: limites_linhas(a),
        kind: ChartKind::Histogram,
        ..Chart::default()
    });
}

fn dispersao(a: &Aplicacao, r: &mut Resultado) {
    let (cx, cy) = (col(a, 0, 0), col(a, 1, 1));
    avisar_invalidas(a, r, cx);
    avisar_invalidas(a, r, cy);
    let pares: Vec<(f64, f64)> = a
        .tabela
        .linhas
        .iter()
        .filter_map(|l| Some((numero(l.get(cx)?)?, numero(l.get(cy)?)?)))
        .collect();
    if pares.len() < 3 {
        r.avisos
            .push("A dispersão precisa de ao menos 3 pares de valores.".into());
        return;
    }
    let xs: Vec<f64> = pares.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pares.iter().map(|p| p.1).collect();
    let (mx, my) = (media(&xs), media(&ys));
    let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    let syy: f64 = ys.iter().map(|y| (y - my).powi(2)).sum();
    let sxy: f64 = pares.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
    r.kpi("Pares", pares.len().to_string(), "");
    if sxx <= 0.0 || syy <= 0.0 {
        r.avisos
            .push("Uma das variáveis não varia: não há correlação para calcular.".into());
    } else {
        let rr = sxy / (sxx * syy).sqrt();
        let b = sxy / sxx;
        let b0 = my - b * mx;
        r.kpi("Correlação (r)", num(rr, 3), "");
        r.kpi(
            "R²",
            num(rr * rr, 3),
            "parte da variação de Y explicada por X",
        );
        r.kpi(
            "Inclinação",
            num(b, 4),
            &format!("y = {} + {}·x", num(b0, 4), num(b, 4)),
        );
        let forca = match rr.abs() {
            v if v >= 0.8 => "forte",
            v if v >= 0.5 => "moderada",
            v if v >= 0.3 => "fraca",
            _ => "desprezível",
        };
        r.achado(
            if rr.abs() >= 0.5 { Tone::Info } else { Tone::Ok },
            "Relação entre as variáveis",
            format!(
                "Correlação {forca} e {} (r = {}). Correlação não prova causa: confirme com experimento.",
                if rr >= 0.0 { "positiva" } else { "negativa" },
                num(rr, 3)
            ),
        );
    }
    r.graficos.push(Chart {
        title: format!("{} × {}", nome_col(a, cy), nome_col(a, cx)),
        series: vec![(nome_col(a, cx), xs), (nome_col(a, cy), ys)],
        kind: ChartKind::Scatter,
        ..Chart::default()
    });
}

/// O tipo de carta: o escolhido ou, sem escolha, pelo tamanho do subgrupo.
pub fn tipo_carta(a: &Aplicacao) -> Carta {
    Carta::parse(&a.parametros.carta).unwrap_or(match a.parametros.subgrupo {
        0 | 1 => Carta::IMr,
        2..=10 => Carta::XbarR,
        _ => Carta::XbarS,
    })
}

fn regras(a: &Aplicacao) -> RuleSet {
    match a.parametros.regras.as_str() {
        "western" => RuleSet::western_electric(),
        "nelson" => RuleSet::nelson(),
        _ => RuleSet::basic(),
    }
}

/// Os valores em subgrupos consecutivos do tamanho pedido (o último incompleto sai).
fn subgrupos(xs: &[f64], n: usize) -> Vec<Vec<f64>> {
    let n = n.max(1);
    xs.chunks(n)
        .filter(|c| c.len() == n)
        .map(<[f64]>::to_vec)
        .collect()
}

fn carta(a: &Aplicacao, r: &mut Resultado) {
    let tipo = tipo_carta(a);
    let (dados, rotulos) = if tipo.is_attribute() {
        let (cl, cc, cn) = (col(a, 0, 0), col(a, 1, 1), col(a, 2, 2));
        avisar_invalidas(a, r, cc);
        let mut counts = Vec::new();
        let mut sizes = Vec::new();
        let mut rot = Vec::new();
        for l in &a.tabela.linhas {
            let Some(c) = l.get(cc).and_then(|s| numero(s)) else {
                continue;
            };
            counts.push(c);
            sizes.push(l.get(cn).and_then(|s| numero(s)).unwrap_or(1.0));
            rot.push(l.get(cl).cloned().unwrap_or_default());
        }
        (ChartData::Attributes { counts, sizes }, rot)
    } else {
        let (cl, cv) = (col(a, 0, 0), col(a, 1, 1));
        avisar_invalidas(a, r, cv);
        let n = if tipo == Carta::IMr {
            1
        } else {
            a.parametros.subgrupo.max(2)
        };
        let linhas: Vec<(String, f64)> = a
            .tabela
            .linhas
            .iter()
            .filter_map(|l| Some((l.get(cl).cloned().unwrap_or_default(), numero(l.get(cv)?)?)))
            .collect();
        let xs: Vec<f64> = linhas.iter().map(|p| p.1).collect();
        let g = subgrupos(&xs, n);
        if xs.len() % n != 0 {
            r.avisos.push(format!(
                "{} valores finais não completam um subgrupo de {n} e ficaram de fora.",
                xs.len() % n
            ));
        }
        let rot = (0..g.len()).map(|i| linhas[i * n].0.clone()).collect();
        (ChartData::Variables(g), rot)
    };
    let c = match charts::phase1(tipo, &dados, &[], &regras(a)) {
        Ok(c) => c,
        Err(e) => {
            r.avisos
                .push(format!("A carta não pôde ser calculada: {e}."));
            return;
        }
    };
    let k = dados.len();
    let rotulos: Vec<String> = rotulos
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if s.trim().is_empty() {
                (i + 1).to_string()
            } else {
                s.clone()
            }
        })
        .collect();
    let mut sinais = Vec::new();
    for (pi, p) in c.panels.iter().enumerate() {
        let constante = |v: &[f64]| v.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-12);
        let mut ch = Chart {
            title: format!("{} ({})", p.name, tipo.label()),
            labels: rotulos.clone(),
            series: vec![(
                p.name.clone(),
                p.values.iter().map(|v| v.unwrap_or(f64::NAN)).collect(),
            )],
            marks: {
                let mut m: Vec<usize> = p.violations.iter().map(|v| v.index).collect();
                m.sort_unstable();
                m.dedup();
                m
            },
            kind: ChartKind::Lines,
            ..Chart::default()
        };
        if constante(&p.ucl) && constante(&p.lcl) && !p.ucl.is_empty() {
            ch.hlines = vec![
                ("LSC".into(), p.ucl[0]),
                ("LC".into(), p.center[0]),
                ("LIC".into(), p.lcl[0]),
            ];
            let s = (p.ucl[0] - p.center[0]) / 3.0;
            if pi == 0 && s > 0.0 {
                ch.bands = vec![("zona C".into(), p.center[0] - s, p.center[0] + s)];
            }
        } else {
            ch.series.push(("LSC".into(), p.ucl.clone()));
            ch.series.push(("LIC".into(), p.lcl.clone()));
            ch.hlines = vec![("LC".into(), p.center[0])];
        }
        r.graficos.push(ch);
        if pi == 0 {
            r.kpi("LC", num(p.center[0], 4), &p.name);
            r.kpi("LSC", num(p.ucl[0], 4), "");
            r.kpi("LIC", num(p.lcl[0], 4), "");
        }
        for v in &p.violations {
            sinais.push(vec![
                p.name.clone(),
                rotulos.get(v.index).cloned().unwrap_or_default(),
                num(p.values[v.index].unwrap_or(f64::NAN), 4),
                v.rule.describe(),
            ]);
        }
    }
    r.indicadores.insert(
        0,
        Kpi {
            label: "Subgrupos".into(),
            value: k.to_string(),
            note: tipo.label().into(),
        },
    );
    if sinais.is_empty() {
        r.achado(
            Tone::Ok,
            "Processo sob controle",
            "Nenhuma regra disparou: a variação é de causas comuns.".into(),
        );
    } else {
        r.achado(
            Tone::Danger,
            "Sinais de causa especial",
            format!(
                "{} {} : investigue o que mudou nesses pontos antes de calcular a capacidade.",
                sinais.len(),
                if sinais.len() == 1 { "sinal" } else { "sinais" }
            )
            .replace(" :", ":"),
        );
        r.quadro(
            "Sinais",
            &["Painel", "Ponto", "Valor", "Regra"],
            sinais,
            &[1.2, 1.0, 1.0, 3.0],
        );
    }
    if !c.notes.is_empty() {
        r.textos.push(("Notas do cálculo".into(), c.notes));
    }
}

fn capacidade(a: &Aplicacao, r: &mut Resultado) {
    let c = col(a, 0, 0);
    avisar_invalidas(a, r, c);
    let xs = a.tabela.numeros(c);
    let p = &a.parametros;
    let spec = Spec {
        lsl: p.lie,
        usl: p.lse,
        target: p.alvo,
    };
    let n = p.subgrupo.max(1);
    let grupos = if n == 1 {
        vec![xs.clone()]
    } else {
        subgrupos(&xs, n)
    };
    let cap = match capability::capability(&grupos, &spec, &Options::default()) {
        Ok(cap) => cap,
        Err(e) => {
            r.avisos
                .push(format!("A capacidade não pôde ser calculada: {e}."));
            return;
        }
    };
    let ind = |i: Option<capability::Index>| i.map_or("—".into(), |i| num(i.value, 2));
    r.kpi("Cp", ind(cap.cp), "potencial, dentro dos subgrupos");
    r.kpi(
        "Cpk",
        num(cap.cpk.value, 2),
        &format!("IC {} a {}", num(cap.cpk.lo, 2), num(cap.cpk.hi, 2)),
    );
    r.kpi("Pp", ind(cap.pp), "desempenho, variação total");
    r.kpi("Ppk", num(cap.ppk.value, 2), "");
    r.kpi(
        "PPM total",
        num(cap.ppm_overall.total, 0),
        "esperado pela variação total",
    );
    r.kpi(
        "Nível sigma",
        num(cap.sigma_level, 2),
        "com deslocamento de 1,5",
    );
    let fmt_i = |i: Option<capability::Index>| {
        i.map_or(vec!["—".into(), String::new()], |i| {
            vec![
                num(i.value, 3),
                format!("{} a {}", num(i.lo, 3), num(i.hi, 3)),
            ]
        })
    };
    let mut rows = Vec::new();
    for (nome, i) in [
        ("Cp", cap.cp),
        ("Cpk", Some(cap.cpk)),
        ("Pp", cap.pp),
        ("Ppk", Some(cap.ppk)),
    ] {
        let mut row = vec![nome.to_string()];
        row.extend(fmt_i(i));
        rows.push(row);
    }
    r.quadro(
        &format!("Índices (IC de {} %)", num(100.0 * cap.level, 0)),
        &["Índice", "Valor", "Intervalo"],
        rows,
        &[1.0, 1.0, 2.0],
    );
    r.quadro(
        "Resumo",
        &["Medida", "Valor"],
        vec![
            vec!["n".into(), cap.n.to_string()],
            vec!["Média".into(), num(cap.mean, 4)],
            vec!["Desvio dentro".into(), num(cap.sigma_within, 4)],
            vec!["Desvio total".into(), num(cap.sigma_overall, 4)],
            vec!["PPM observado".into(), num(cap.ppm_observed.total, 0)],
            vec!["PPM dentro".into(), num(cap.ppm_within.total, 0)],
            vec!["PPM total".into(), num(cap.ppm_overall.total, 0)],
        ],
        &[2.0, 1.0],
    );
    let (tom, titulo, texto) = match cap.ppk.value {
        v if v >= 1.33 => (
            Tone::Ok,
            "Processo capaz",
            "Ppk ≥ 1,33: o processo atende à especificação com folga.",
        ),
        v if v >= 1.0 => (
            Tone::Warn,
            "Capacidade no limite",
            "Ppk entre 1,00 e 1,33: atende, mas sem folga. Reduza a variação ou centralize.",
        ),
        _ => (
            Tone::Danger,
            "Processo não capaz",
            "Ppk < 1,00: o processo gera peças fora da especificação. Atue na variação e na centralização.",
        ),
    };
    r.achado(tom, titulo, texto.into());
    if let Some(cp) = cap.cp
        && cp.value - cap.cpk.value > 0.25
    {
        r.achado(
            Tone::Info,
            "Processo descentralizado",
            format!(
                "Cp {} contra Cpk {}: centralizar a média já melhora o resultado.",
                num(cp.value, 2),
                num(cap.cpk.value, 2)
            ),
        );
    }
    r.graficos.push(Chart {
        title: "Distribuição contra a especificação".into(),
        series: vec![(nome_col(a, c).to_lowercase(), xs)],
        hlines: limites_linhas(a),
        kind: ChartKind::Histogram,
        ..Chart::default()
    });
    if !cap.notes.is_empty() {
        r.textos.push(("Notas do cálculo".into(), cap.notes));
    }
}

fn msa_rr(a: &Aplicacao, r: &mut Resultado) {
    let (cp, co, cv) = (col(a, 0, 0), col(a, 1, 1), col(a, 2, 2));
    avisar_invalidas(a, r, cv);
    let ms: Vec<Measurement> = a
        .tabela
        .linhas
        .iter()
        .filter_map(|l| {
            Some(Measurement {
                part: l.get(cp)?.trim().to_string(),
                operator: l.get(co)?.trim().to_string(),
                value: numero(l.get(cv)?)?,
            })
        })
        .filter(|m| !m.part.is_empty() && !m.operator.is_empty())
        .collect();
    let rr = GageData::from_measurements(&ms).and_then(|d| {
        msa::gage_anova(
            &d,
            &GageOptions {
                tolerance: a.parametros.tolerancia,
                ..GageOptions::default()
            },
        )
    });
    let rr = match rr {
        Ok(rr) => rr,
        Err(e) => {
            r.avisos
                .push(format!("O estudo de R&R não pôde ser calculado: {e}."));
            return;
        }
    };
    let grr = rr.component(msa::GRR).map_or(f64::NAN, |c| c.pct_study_var);
    r.kpi(
        "%VE do GRR",
        format!("{} %", num(grr, 1)),
        "variação do estudo",
    );
    if let Some(t) = rr.component(msa::GRR).and_then(|c| c.pct_tolerance) {
        r.kpi("%Tolerância", format!("{} %", num(t, 1)), "");
    }
    r.kpi("ndc", rr.ndc.to_string(), "categorias distintas (≥ 5)");
    r.kpi(
        "Estudo",
        format!("{} × {} × {}", rr.parts, rr.operators, rr.reps),
        "peças × operadores × repetições",
    );
    r.achado(
        if grr <= 10.0 {
            Tone::Ok
        } else if grr <= 30.0 {
            Tone::Warn
        } else {
            Tone::Danger
        },
        "Sistema de medição",
        format!("O sistema de medição é {}.", rr.verdict()),
    );
    let tem_tol = rr.components.iter().any(|c| c.pct_tolerance.is_some());
    let rows = rr
        .components
        .iter()
        .map(|c| {
            let mut row = vec![
                c.name.clone(),
                num(c.variance, 6),
                num(c.pct_contribution, 2),
                num(c.sd, 5),
                num(c.pct_study_var, 2),
            ];
            if tem_tol {
                row.push(c.pct_tolerance.map_or("—".into(), |t| num(t, 2)));
            }
            row
        })
        .collect();
    let mut cab = vec!["Fonte", "Variância", "% contribuição", "Desvio", "%VE"];
    if tem_tol {
        cab.push("%Tolerância");
    }
    r.quadro("Componentes da variação", &cab, rows, &[]);
    let anova = rr
        .anova
        .iter()
        .map(|l| {
            vec![
                l.source.clone(),
                num(l.df, 0),
                num(l.ss, 6),
                num(l.ms, 6),
                l.f.map_or("—".into(), |f| num(f, 3)),
                l.p.map_or("—".into(), |p| num(p, 4)),
            ]
        })
        .collect();
    r.quadro("ANOVA", &["Fonte", "GL", "SQ", "QM", "F", "p"], anova, &[]);
    r.graficos.push(Chart {
        title: "Componentes da variação (%VE)".into(),
        bars: true,
        labels: rr.components.iter().map(|c| c.name.clone()).collect(),
        series: vec![(
            "%VE".into(),
            rr.components.iter().map(|c| c.pct_study_var).collect(),
        )],
        hlines: vec![("10 %".into(), 10.0), ("30 %".into(), 30.0)],
        kind: ChartKind::Bars,
        ..Chart::default()
    });
    if !rr.warnings.is_empty() {
        r.textos.push(("Notas do cálculo".into(), rr.warnings));
    }
}

fn ishikawa(a: &Aplicacao, problema: &str, r: &mut Resultado) {
    let efeito = if a.efeito.trim().is_empty() {
        problema.trim()
    } else {
        a.efeito.trim()
    };
    let cats: Vec<(String, Vec<String>)> = a
        .categorias
        .iter()
        .map(|c| {
            (
                c.nome.trim().to_string(),
                c.causas
                    .iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(n, cs)| !n.is_empty() && !cs.is_empty())
        .collect();
    if efeito.is_empty() {
        r.avisos
            .push("Escreva o efeito (o problema) na cabeça do peixe.".into());
    }
    if cats.is_empty() {
        r.avisos.push("Nenhuma causa cadastrada.".into());
        return;
    }
    let total: usize = cats.iter().map(|c| c.1.len()).sum();
    r.kpi("Causas levantadas", total.to_string(), "");
    r.kpi("Categorias com causas", cats.len().to_string(), "");
    r.ishikawa = Some((efeito.to_string(), cats));
}

fn porques(a: &Aplicacao, problema: &str, r: &mut Resultado) {
    let prob = if a.efeito.trim().is_empty() {
        problema.trim()
    } else {
        a.efeito.trim()
    };
    let mut rows = Vec::new();
    for (i, c) in a.cadeias.iter().enumerate() {
        let ps: Vec<&str> = c
            .porques
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if ps.is_empty() {
            continue;
        }
        let linhas: Vec<String> = ps
            .iter()
            .enumerate()
            .map(|(k, p)| format!("Por quê {}? {p}", k + 1))
            .collect();
        r.textos.push((format!("Cadeia {}", i + 1), linhas));
        rows.push(vec![
            (i + 1).to_string(),
            ps.last().unwrap().to_string(),
            c.acao.trim().to_string(),
        ]);
        if ps.len() < 3 {
            r.avisos.push(format!(
                "A cadeia {} parou no {}º porquê: confira se chegou à causa raiz.",
                i + 1,
                ps.len()
            ));
        }
        if c.acao.trim().is_empty() {
            r.avisos.push(format!(
                "A cadeia {} não tem ação sobre a causa raiz.",
                i + 1
            ));
        }
    }
    if rows.is_empty() {
        r.avisos.push("Nenhum porquê respondido.".into());
        return;
    }
    if prob.is_empty() {
        r.avisos.push("Escreva o problema de partida.".into());
    } else {
        r.textos
            .insert(0, ("Problema".into(), vec![prob.to_string()]));
    }
    r.kpi("Cadeias", rows.len().to_string(), "");
    r.quadro(
        "Causas raiz e ações",
        &["Cadeia", "Causa raiz", "Ação"],
        rows,
        &[0.6, 3.0, 3.0],
    );
}

fn nota(s: &str) -> Option<u8> {
    numero(s)
        .filter(|v| v.fract() == 0.0 && (1.0..=10.0).contains(v))
        .map(|v| v as u8)
}

/// O risco de uma linha da FMEA com S, O e D de 1 a 10.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiscoFmea {
    /// A linha na tabela.
    pub linha: usize,
    pub s: u8,
    pub o: u8,
    pub d: u8,
    /// S × O × D.
    pub npr: u32,
    /// A ação prioritária pela tabela de sempre (AIAG-VDA).
    pub ap: Ap,
}

/// NPR a partir do qual a linha fica vermelha na tela.
pub const NPR_ALTO: u32 = 200;
/// NPR a partir do qual a linha fica âmbar na tela.
pub const NPR_MEDIO: u32 = 100;

/// As linhas da FMEA com modo de falha e S, O e D de 1 a 10, na ordem da tabela, e quantas
/// têm modo de falha mas ficaram sem alguma nota.
pub fn riscos_fmea(t: &Tabela) -> (Vec<RiscoFmea>, usize) {
    let idx = |nome: &str, padrao: usize| t.coluna(nome).unwrap_or(padrao);
    let (cm, cs, co, cd) = (
        idx("Modo de falha", 1),
        idx("S", 3),
        idx("O", 5),
        idx("D", 8),
    );
    let tab = ApTable::builtin();
    let mut riscos = Vec::new();
    let mut incompletas = 0;
    for (i, l) in t.linhas.iter().enumerate() {
        let cel = |c: usize| l.get(c).map_or("", |s| s.trim());
        if cel(cm).is_empty() {
            continue;
        }
        match (nota(cel(cs)), nota(cel(co)), nota(cel(cd))) {
            (Some(s), Some(o), Some(d)) => riscos.push(RiscoFmea {
                linha: i,
                s,
                o,
                d,
                npr: u32::from(s) * u32::from(o) * u32::from(d),
                ap: tab.get(s, o, d).unwrap_or(Ap::Low),
            }),
            _ => incompletas += 1,
        }
    }
    (riscos, incompletas)
}

fn fmea(a: &Aplicacao, r: &mut Resultado) {
    let t = &a.tabela;
    let idx = |nome: &str, padrao: usize| t.coluna(nome).unwrap_or(padrao);
    let (ce, cm, cf, cc, ca) = (
        idx("Etapa", 0),
        idx("Modo de falha", 1),
        idx("Efeito", 2),
        idx("Causa", 4),
        idx("Ação recomendada", 9),
    );
    let (riscos, incompletas) = riscos_fmea(t);
    let mut linhas: Vec<(usize, u8, u8, u8, u32, Ap)> = riscos
        .iter()
        .map(|x| (x.linha, x.s, x.o, x.d, x.npr, x.ap))
        .collect();
    if incompletas > 0 {
        r.avisos.push(format!(
            "{incompletas} {} sem S, O e D de 1 a 10 ficaram de fora.",
            if incompletas == 1 { "linha" } else { "linhas" }
        ));
    }
    if linhas.is_empty() {
        r.avisos
            .push("Nenhum modo de falha com S, O e D preenchidos.".into());
        return;
    }
    linhas.sort_by(|x, y| y.5.cmp(&x.5).then(y.4.cmp(&x.4)));
    let altas = linhas.iter().filter(|l| l.5 == Ap::High).count();
    let criticas = linhas
        .iter()
        .filter(|l| l.1 >= a.parametros.severidade_critica)
        .count();
    r.kpi("Modos de falha", linhas.len().to_string(), "");
    r.kpi("Prioridade alta", altas.to_string(), "AP = H");
    r.kpi(
        "Severidade crítica",
        criticas.to_string(),
        &format!("S ≥ {}", a.parametros.severidade_critica),
    );
    r.kpi(
        "Maior NPR",
        linhas.iter().map(|l| l.4).max().unwrap_or(0).to_string(),
        "S × O × D",
    );
    let ap_txt = |ap: Ap| match ap {
        Ap::High => "Alta",
        Ap::Medium => "Média",
        Ap::Low => "Baixa",
    };
    let cel = |i: usize, c: usize| t.celula(i, c).trim().to_string();
    let rows = linhas
        .iter()
        .map(|&(i, s, o, d, npr, ap)| {
            vec![
                cel(i, ce),
                cel(i, cm),
                cel(i, cf),
                cel(i, cc),
                s.to_string(),
                o.to_string(),
                d.to_string(),
                npr.to_string(),
                ap_txt(ap).into(),
                cel(i, ca),
            ]
        })
        .collect();
    r.quadro(
        "Modos de falha por prioridade",
        &[
            "Etapa", "Modo", "Efeito", "Causa", "S", "O", "D", "NPR", "AP", "Ação",
        ],
        rows,
        &[1.2, 1.6, 1.6, 1.6, 0.4, 0.4, 0.4, 0.6, 0.7, 2.0],
    );
    let top: Vec<_> = linhas.iter().take(10).collect();
    r.graficos.push(Chart {
        title: "NPR dos modos de falha".into(),
        bars: true,
        labels: top
            .iter()
            .map(|&&(i, ..)| {
                let m = cel(i, cm);
                if cel(i, ce).is_empty() {
                    m
                } else {
                    format!("{}: {m}", cel(i, ce))
                }
            })
            .collect(),
        series: vec![("NPR".into(), top.iter().map(|l| f64::from(l.4)).collect())],
        kind: ChartKind::Bars,
        ..Chart::default()
    });
    let sem_acao: Vec<String> = linhas
        .iter()
        .filter(|l| {
            (l.5 == Ap::High || l.1 >= a.parametros.severidade_critica) && cel(l.0, ca).is_empty()
        })
        .map(|l| cel(l.0, cm))
        .collect();
    if !sem_acao.is_empty() {
        r.achado(
            Tone::Danger,
            "Prioridade sem ação",
            format!("Sem ação recomendada: {}.", sem_acao.join(", ")),
        );
    } else if altas > 0 {
        r.achado(
            Tone::Warn,
            "Ações de prioridade alta",
            format!("{altas} modos de prioridade alta já têm ação: acompanhe a reavaliação."),
        );
    }
}

fn a3(a: &Aplicacao, r: &mut Resultado) {
    for s in &a.secoes {
        let linhas: Vec<String> = s
            .texto
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if !linhas.is_empty() {
            r.textos.push((s.titulo.clone(), linhas));
        }
    }
    let vazias: Vec<&str> = a
        .secoes
        .iter()
        .filter(|s| s.texto.trim().is_empty())
        .map(|s| s.titulo.as_str())
        .collect();
    if r.textos.is_empty() {
        r.avisos.push("O A3 está em branco.".into());
    } else if !vazias.is_empty() {
        r.avisos
            .push(format!("Seções em branco: {}.", vazias.join(", ")));
    }
}

#[cfg(test)]
mod tests;
