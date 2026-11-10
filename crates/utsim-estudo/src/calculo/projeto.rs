//! As ferramentas de projeto e de prevenção sobre os núcleos comuns: o QFD
//! (`utsim_ferramentas::qfd`), o empilhamento de tolerâncias (`utsim_quality::tolerancias`)
//! e o poka-yoke (`utsim_ferramentas::poka_yoke`). A tabela da aplicação vira o tipo do
//! núcleo, o núcleo calcula e o resultado volta como indicadores, quadros, gráficos e
//! achados.
//!
//! - QFD: cada linha é uma necessidade do cliente; cada coluna depois de "Ponto de venda" é
//!   um requisito técnico, com a relação 0, 1, 3 ou 9 na célula. A primeira tabela auxiliar
//!   dá a unidade, o sentido e o alvo de cada requisito (pelo nome); a segunda, o telhado.
//! - Tolerâncias: cada linha é uma cota; os limites do resultado são o LIE e o LSE dos
//!   parâmetros.
//! - Poka-yoke: cada linha é um dispositivo; a tabela auxiliar guarda as verificações.

use utsim_ferramentas::poka_yoke::{self, Check, Device, Devices, Effect, Function, Kind};
use utsim_ferramentas::qfd::{self, Correlation, Direction, House, Need, Requirement, RoofCell};
use utsim_ferramentas::relatorios::{Chart, ChartKind, Tone};
use utsim_quality::tolerancias::{self, Dimension, Spread, Stack};

use super::{Resultado, nota};
use crate::importar::normalizar;
use crate::tabela::numero;
use crate::{Aplicacao, Tabela, num};

/// As colunas fixas do QFD; as que vêm depois são os requisitos técnicos.
pub const COLUNAS_QFD: [&str; 6] = [
    "Necessidade",
    "Importância",
    "Atual",
    "Concorrente",
    "Meta",
    "Ponto de venda",
];

/// Os detalhes dos requisitos do QFD (primeira tabela auxiliar).
pub const COLUNAS_QFD_REQUISITOS: [&str; 4] = ["Requisito", "Unidade", "Sentido", "Alvo"];

/// O telhado do QFD (segunda tabela auxiliar). Correlação: `++`, `+`, `-` ou `--`.
pub const COLUNAS_QFD_TELHADO: [&str; 3] = ["Requisito A", "Requisito B", "Correlação"];

/// As cotas do empilhamento.
pub const COLUNAS_TOLERANCIAS: [&str; 6] = [
    "Cota",
    "Nominal",
    "Tol. inferior",
    "Tol. superior",
    "Coeficiente",
    "Distribuição",
];

/// Os dispositivos do poka-yoke.
pub const COLUNAS_POKA_YOKE: [&str; 11] = [
    "Dispositivo",
    "Tipo",
    "Função",
    "Etapa",
    "Modo de falha",
    "O antes",
    "D antes",
    "O depois",
    "D depois",
    "Frequência (dias)",
    "Responsável",
];

/// As verificações do poka-yoke (tabela auxiliar).
pub const COLUNAS_VERIFICACOES: [&str; 5] = ["Dispositivo", "Data", "Funcionou", "Quem", "Nota"];

/// Montagens simuladas no empilhamento e a semente (o resultado se repete).
pub const MONTAGENS: usize = 100_000;
pub const SEMENTE: u64 = 1;

fn cel(t: &Tabela, l: usize, c: Option<usize>) -> String {
    c.map_or(String::new(), |c| t.celula(l, c).trim().to_string())
}

fn opcional(t: &Tabela, l: usize, c: Option<usize>) -> Option<f64> {
    numero(&cel(t, l, c))
}

fn linha_vazia(t: &Tabela, l: usize) -> bool {
    t.linhas[l].iter().all(|c| c.trim().is_empty())
}

// ---- QFD ----------------------------------------------------------------------------

fn sentido(texto: &str) -> Option<Direction> {
    match normalizar(texto).as_str() {
        "" | "maior" | "maximizar" | "aumentar" | "↑" => Some(Direction::Higher),
        "menor" | "minimizar" | "diminuir" | "↓" => Some(Direction::Lower),
        "alvo" | "nominal" | "o" => Some(Direction::Target),
        _ => None,
    }
}

fn sentido_txt(d: Direction) -> &'static str {
    match d {
        Direction::Higher => "maior",
        Direction::Lower => "menor",
        Direction::Target => "alvo",
    }
}

fn correlacao(texto: &str) -> Option<Correlation> {
    match texto.trim() {
        "++" => Some(Correlation::StrongPositive),
        "+" => Some(Correlation::Positive),
        "-" | "−" => Some(Correlation::Negative),
        "--" | "−−" => Some(Correlation::StrongNegative),
        _ => None,
    }
}

/// O símbolo da relação na matriz: ● forte, ○ média, △ fraca.
fn simbolo(v: u8) -> &'static str {
    match v {
        9 => "●",
        3 => "○",
        1 => "△",
        _ => "",
    }
}

/// A casa da qualidade lida das tabelas da aplicação.
pub fn casa(a: &Aplicacao, r: &mut Resultado) -> House {
    let t = &a.tabela;
    let fixa = |nome: &str| t.coluna(nome);
    let (cn, ci, ca, cc, cm, cp) = (
        fixa("Necessidade").or(Some(0)),
        fixa("Importância"),
        fixa("Atual"),
        fixa("Concorrente"),
        fixa("Meta"),
        fixa("Ponto de venda"),
    );
    let fixas: Vec<usize> = [cn, ci, ca, cc, cm, cp].into_iter().flatten().collect();
    let reqs: Vec<usize> = (0..t.colunas.len())
        .filter(|c| !fixas.contains(c) && !t.colunas[*c].trim().is_empty())
        .collect();
    let detalhes = a.auxiliar(0);
    let mut casa = House::default();
    for &c in &reqs {
        let nome = t.colunas[c].trim().to_string();
        let mut req = Requirement {
            text: nome.clone(),
            ..Requirement::default()
        };
        if let Some(l) = (0..detalhes.linhas.len())
            .find(|&l| normalizar(detalhes.celula(l, 0)) == normalizar(&nome))
        {
            req.unit = detalhes.celula(l, 1).trim().into();
            match sentido(detalhes.celula(l, 2)) {
                Some(d) => req.direction = d,
                None => r.avisos.push(format!(
                    "O sentido de \"{nome}\" deve ser maior, menor ou alvo."
                )),
            }
            req.target = numero(detalhes.celula(l, 3));
        }
        casa.requirements.push(req);
    }
    for l in 0..detalhes.linhas.len() {
        let n = detalhes.celula(l, 0).trim();
        if !n.is_empty()
            && !casa
                .requirements
                .iter()
                .any(|q| normalizar(&q.text) == normalizar(n))
        {
            r.avisos.push(format!(
                "O requisito \"{n}\" dos detalhes não é uma coluna da matriz."
            ));
        }
    }
    let mut ruins = 0;
    for l in 0..t.linhas.len() {
        if linha_vazia(t, l) {
            continue;
        }
        let texto = cel(t, l, cn);
        let imp = opcional(t, l, ci).unwrap_or(0.0);
        if imp <= 0.0 {
            r.avisos.push(format!(
                "A necessidade \"{texto}\" está sem importância (1 a 5) e não pesa."
            ));
        }
        casa.needs.push(Need {
            text: texto,
            importance: imp,
            current: opcional(t, l, ca),
            competitor: opcional(t, l, cc),
            goal: opcional(t, l, cm),
            sales_point: opcional(t, l, cp),
        });
        casa.relationships.push(
            reqs.iter()
                .map(|&c| {
                    let s = cel(t, l, Some(c));
                    match numero(&s) {
                        None if s.is_empty() => 0,
                        Some(v) if qfd::RELATIONSHIPS.contains(&(v as u8)) && v.fract() == 0.0 => {
                            v as u8
                        }
                        _ => {
                            ruins += 1;
                            0
                        }
                    }
                })
                .collect(),
        );
    }
    if ruins > 0 {
        r.avisos.push(format!(
            "{ruins} {} da matriz não {} 0, 1, 3 ou 9 e {} como 0.",
            if ruins == 1 { "célula" } else { "células" },
            if ruins == 1 { "é" } else { "são" },
            if ruins == 1 { "contou" } else { "contaram" },
        ));
    }
    let telhado = a.auxiliar(1);
    let indice = |n: &str| {
        casa.requirements
            .iter()
            .position(|q| normalizar(&q.text) == normalizar(n))
    };
    for l in 0..telhado.linhas.len() {
        let (x, y, z) = (
            telhado.celula(l, 0).trim(),
            telhado.celula(l, 1).trim(),
            telhado.celula(l, 2),
        );
        if x.is_empty() && y.is_empty() {
            continue;
        }
        match (indice(x), indice(y), correlacao(z)) {
            (Some(i), Some(j), Some(c)) if i != j => casa.roof.push(RoofCell {
                a: i,
                b: j,
                correlation: c,
            }),
            _ => r.avisos.push(format!(
                "A linha {} do telhado precisa de dois requisitos da matriz e da correlação ++, +, - ou --.",
                l + 1
            )),
        }
    }
    casa
}

pub fn qfd(a: &Aplicacao, r: &mut Resultado) {
    let h = casa(a, r);
    if h.needs.is_empty() || h.requirements.is_empty() {
        r.avisos.push(
            "O QFD precisa de necessidades (linhas) e de requisitos técnicos (colunas depois de \"Ponto de venda\")."
                .into(),
        );
        return;
    }
    let o = qfd::compute(&h);
    r.kpi("Necessidades", h.needs.len().to_string(), "");
    r.kpi("Requisitos técnicos", h.requirements.len().to_string(), "");
    if let Some(&j) = o.ranking.first() {
        r.kpi(
            "Mais importante",
            h.requirements[j].text.clone(),
            &format!("{} % do peso", num(o.requirements[j].weight_pct, 1)),
        );
    }
    let conflitos = o
        .findings
        .iter()
        .filter(|f| matches!(f, qfd::Finding::Conflict(..)))
        .count();
    r.kpi(
        "Conflitos no telhado",
        conflitos.to_string(),
        "entre os mais importantes",
    );
    let linhas = o
        .ranking
        .iter()
        .map(|&j| {
            let q = &h.requirements[j];
            let s = &o.requirements[j];
            vec![
                s.rank.to_string(),
                q.text.clone(),
                q.unit.clone(),
                sentido_txt(q.direction).into(),
                q.target.map_or(String::new(), |v| num(v, 4)),
                num(s.weight, 2),
                format!("{} %", num(s.weight_pct, 1)),
            ]
        })
        .collect();
    r.quadro(
        "Requisitos técnicos por peso",
        &[
            "Posição",
            "Requisito",
            "Unidade",
            "Sentido",
            "Alvo",
            "Peso",
            "%",
        ],
        linhas,
        &[0.6, 2.2, 0.8, 0.8, 0.8, 0.8, 0.7],
    );
    let mut cab: Vec<String> = vec!["Necessidade".into(), "Peso %".into()];
    cab.extend(h.requirements.iter().map(|q| q.text.clone()));
    let matriz: Vec<Vec<String>> = h
        .needs
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let mut l = vec![n.text.clone(), num(o.needs[i].weight_pct, 1)];
            l.extend((0..h.requirements.len()).map(|j| simbolo(h.relationship(i, j)).into()));
            l
        })
        .collect();
    let mut larguras = vec![2.2, 0.7];
    larguras.extend(std::iter::repeat_n(0.8, h.requirements.len()));
    r.quadros.push(super::Quadro {
        nome: "Matriz de relações (● 9, ○ 3, △ 1)".into(),
        cabecalho: cab,
        linhas: matriz,
        larguras,
    });
    if h.needs
        .iter()
        .any(|n| n.current.is_some() || n.goal.is_some())
    {
        let linhas = h
            .needs
            .iter()
            .zip(&o.needs)
            .map(|(n, s)| {
                let f = |v: Option<f64>| v.map_or(String::new(), |v| num(v, 2));
                vec![
                    n.text.clone(),
                    num(n.importance, 2),
                    f(n.current),
                    f(n.competitor),
                    f(n.goal),
                    f(s.improvement_ratio),
                    f(n.sales_point),
                    format!("{} %", num(s.weight_pct, 1)),
                ]
            })
            .collect();
        r.quadro(
            "Planejamento das necessidades",
            &[
                "Necessidade",
                "Importância",
                "Atual",
                "Concorrente",
                "Meta",
                "Melhoria",
                "Ponto de venda",
                "Peso",
            ],
            linhas,
            &[2.2, 0.8, 0.6, 0.8, 0.6, 0.7, 0.8, 0.6],
        );
    }
    r.graficos.push(Chart {
        title: "Peso dos requisitos técnicos (%)".into(),
        bars: true,
        labels: o
            .ranking
            .iter()
            .map(|&j| h.requirements[j].text.clone())
            .collect(),
        series: vec![(
            "Peso %".into(),
            o.ranking
                .iter()
                .map(|&j| o.requirements[j].weight_pct)
                .collect(),
        )],
        kind: ChartKind::Pareto,
        ..Chart::default()
    });
    for f in &o.findings {
        let (tom, titulo) = match f {
            qfd::Finding::NeedUncovered(_) => (Tone::Danger, "Necessidade sem requisito"),
            qfd::Finding::Conflict(..) => (Tone::Danger, "Conflito no telhado"),
            qfd::Finding::NeedWeak(_) => (Tone::Warn, "Necessidade mal medida"),
            qfd::Finding::RequirementUnused(_) => (Tone::Warn, "Requisito sem necessidade"),
            qfd::Finding::NoTarget(_) => (Tone::Warn, "Requisito sem alvo"),
        };
        r.achado(tom, titulo, f.text(&h));
    }
}

// ---- Tolerâncias --------------------------------------------------------------------

/// A distribuição de uma cota: `normal` (±3σ), `normal 4` (±4σ), `uniforme`, `triangular`.
fn espalhamento(texto: &str) -> Option<Spread> {
    let t = normalizar(texto);
    let mut partes = t.split_whitespace();
    match partes.next() {
        None | Some("uniforme") => Some(Spread::Uniform),
        Some("triangular") => Some(Spread::Triangular),
        Some("normal") => {
            let s = match partes.next() {
                None => 3.0,
                Some(p) => numero(p.trim_end_matches('σ').trim_end_matches("sigma"))?,
            };
            (s > 0.0).then_some(Spread::Normal { sigmas: s })
        }
        _ => None,
    }
}

fn espalhamento_txt(s: Spread) -> String {
    match s {
        Spread::Normal { sigmas } => format!("normal ±{}σ", num(sigmas, 2)),
        Spread::Uniform => "uniforme".into(),
        Spread::Triangular => "triangular".into(),
    }
}

/// O empilhamento lido da tabela da aplicação.
pub fn pilha(a: &Aplicacao, r: &mut Resultado) -> Stack {
    let t = &a.tabela;
    let idx = |nome: &str, padrao: usize| t.coluna(nome).or(Some(padrao));
    let (cn, cv, ci, cs, cc, cd) = (
        idx("Cota", 0),
        idx("Nominal", 1),
        idx("Tol. inferior", 2),
        idx("Tol. superior", 3),
        idx("Coeficiente", 4),
        idx("Distribuição", 5),
    );
    let mut cotas = Vec::new();
    for l in 0..t.linhas.len() {
        if linha_vazia(t, l) {
            continue;
        }
        let nome = cel(t, l, cn);
        let rotulo = if nome.is_empty() {
            format!("linha {}", l + 1)
        } else {
            nome.clone()
        };
        let Some(nominal) = opcional(t, l, cv) else {
            r.avisos
                .push(format!("A cota {rotulo} está sem nominal e ficou de fora."));
            continue;
        };
        let inferior = opcional(t, l, ci).unwrap_or(0.0).abs();
        // Sem a tolerância superior, a cota é simétrica.
        let superior = opcional(t, l, cs).map_or(inferior, f64::abs);
        let coef = opcional(t, l, cc).unwrap_or(1.0);
        let spread = espalhamento(&cel(t, l, cd)).unwrap_or_else(|| {
            r.avisos.push(format!(
                "A distribuição da cota {rotulo} deve ser normal, uniforme ou triangular: ficou uniforme."
            ));
            Spread::Uniform
        });
        cotas.push(Dimension {
            name: rotulo,
            nominal,
            tol_lower: inferior,
            tol_upper: superior,
            coefficient: coef,
            spread,
        });
    }
    Stack {
        dimensions: cotas,
        lsl: a.parametros.lie,
        usl: a.parametros.lse,
        samples: MONTAGENS,
        seed: SEMENTE,
    }
}

pub fn tolerancias(a: &Aplicacao, r: &mut Resultado) {
    let p = pilha(a, r);
    if p.dimensions.is_empty() {
        r.avisos
            .push("Acrescente as cotas do empilhamento, com nominal e tolerâncias.".into());
        return;
    }
    let s = tolerancias::compute(&p);
    let faixa = |lo: f64, hi: f64| format!("{} a {}", num(lo, 4), num(hi, 4));
    r.kpi(
        "Nominal do resultado",
        num(s.nominal, 4),
        "Σ coeficiente × nominal",
    );
    r.kpi(
        "Pior caso",
        faixa(s.worst_low, s.worst_high),
        "todas nas pontas",
    );
    r.kpi(
        "RSS",
        faixa(s.rss_low, s.rss_high),
        "raiz da soma dos quadrados",
    );
    r.kpi(
        "Média simulada",
        num(s.mean, 4),
        &format!("desvio {}, {} montagens", num(s.sd, 4), s.samples),
    );
    if let Some(ppm) = s.ppm {
        r.kpi("Fora dos limites", num(ppm, 0), "PPM simulado");
    }
    if let Some(cp) = s.cp {
        r.kpi("Cp", num(cp, 2), "");
    }
    if let Some(cpk) = s.cpk {
        r.kpi("Cpk", num(cpk, 2), "");
    }
    let mut ordem: Vec<usize> = (0..p.dimensions.len()).collect();
    ordem.sort_by(|x, y| s.contribution_pct[*y].total_cmp(&s.contribution_pct[*x]));
    let linhas = ordem
        .iter()
        .map(|&i| {
            let d = &p.dimensions[i];
            vec![
                d.name.clone(),
                num(d.coefficient, 3),
                num(d.nominal, 4),
                format!("-{} / +{}", num(d.tol_lower, 4), num(d.tol_upper, 4)),
                espalhamento_txt(d.spread),
                format!("{} %", num(s.contribution_pct[i], 1)),
            ]
        })
        .collect();
    r.quadro(
        "Contribuição de cada cota para a variação",
        &[
            "Cota",
            "Coeficiente",
            "Nominal",
            "Tolerância",
            "Distribuição",
            "Contribuição",
        ],
        linhas,
        &[1.6, 0.8, 0.9, 1.2, 1.1, 0.9],
    );
    r.quadro(
        "Resultado do empilhamento",
        &["Método", "Mínimo", "Máximo"],
        vec![
            vec![
                "Pior caso".into(),
                num(s.worst_low, 4),
                num(s.worst_high, 4),
            ],
            vec!["RSS".into(), num(s.rss_low, 4), num(s.rss_high, 4)],
            vec![
                "Simulação (0,135 % e 99,865 %)".into(),
                num(s.p00135, 4),
                num(s.p99865, 4),
            ],
            vec![
                "Simulação (mín. e máx.)".into(),
                num(s.min, 4),
                num(s.max, 4),
            ],
        ],
        &[2.0, 1.0, 1.0],
    );
    r.graficos.push(Chart {
        title: "Resultado das montagens simuladas".into(),
        bars: true,
        labels: s
            .histogram
            .iter()
            .map(|b| num((b.from + b.to) / 2.0, 4))
            .collect(),
        series: vec![(
            "montagens".into(),
            s.histogram.iter().map(|b| b.count as f64).collect(),
        )],
        kind: ChartKind::Bars,
        ..Chart::default()
    });
    if let Some(&i) = ordem.first()
        && s.contribution_pct[i] >= 50.0
    {
        r.achado(
            Tone::Warn,
            "Uma cota domina",
            format!(
                "A cota {} responde por {} % da variação: é nela que apertar a tolerância rende mais.",
                p.dimensions[i].name,
                num(s.contribution_pct[i], 0)
            ),
        );
    }
    match (s.ppm, s.cpk) {
        (Some(ppm), _) if ppm > 0.0 => r.achado(
            Tone::Danger,
            "Montagens fora dos limites",
            format!(
                "A simulação dá {} PPM fora dos limites do resultado.",
                num(ppm, 0)
            ),
        ),
        (_, Some(cpk)) if cpk < 1.33 => r.achado(
            Tone::Warn,
            "Pouca folga",
            format!(
                "Nenhuma montagem simulada saiu, mas o Cpk é {}: abaixo de 1,33.",
                num(cpk, 2)
            ),
        ),
        _ => {}
    }
    for w in &s.warnings {
        match w {
            tolerancias::Warning::WorstCaseOutside => {
                r.achado(Tone::Warn, "Pior caso fora", w.text(&p.dimensions))
            }
            _ => r.avisos.push(w.text(&p.dimensions)),
        }
    }
}

// ---- Poka-yoke ----------------------------------------------------------------------

/// `DD/MM/AAAA` ou `AAAA-MM-DD` em `AAAA-MM-DD`.
pub fn data_iso(texto: &str) -> Option<String> {
    let t = texto.trim();
    let partes: Vec<&str> = t.split(['/', '-']).collect();
    let [a, b, c] = partes.as_slice() else {
        return None;
    };
    let (d, m, y) = if t.contains('/') {
        (a, b, c)
    } else {
        (c, b, a)
    };
    let (d, m, y): (u32, u32, u32) = (d.parse().ok()?, m.parse().ok()?, y.parse().ok()?);
    ((1..=31).contains(&d) && (1..=12).contains(&m) && y >= 1000)
        .then(|| format!("{y:04}-{m:02}-{d:02}"))
}

/// `AAAA-MM-DD` em `DD/MM/AAAA`.
fn data_br(iso: &str) -> String {
    let p: Vec<&str> = iso.split('-').collect();
    match p.as_slice() {
        [y, m, d] => format!("{d}/{m}/{y}"),
        _ => iso.to_string(),
    }
}

fn tipo(texto: &str) -> Option<Kind> {
    match normalizar(texto).replace(['_', '-'], " ").as_str() {
        "" | "contato" => Some(Kind::Contact),
        "valor fixo" | "contagem" => Some(Kind::FixedValue),
        "sequencia" | "etapas" => Some(Kind::Sequence),
        _ => None,
    }
}

fn funcao(texto: &str) -> Option<Function> {
    match normalizar(texto).as_str() {
        "" | "controle" | "parada" => Some(Function::Control),
        "alerta" | "aviso" => Some(Function::Warning),
        _ => None,
    }
}

fn sim_nao(texto: &str) -> Option<bool> {
    match normalizar(texto).as_str() {
        "sim" | "s" | "ok" | "1" | "funcionou" => Some(true),
        "nao" | "n" | "0" | "falhou" => Some(false),
        _ => None,
    }
}

/// Os dispositivos lidos das tabelas da aplicação.
pub fn dispositivos(a: &Aplicacao, r: &mut Resultado) -> Devices {
    let t = &a.tabela;
    let idx = |nome: &str, padrao: usize| t.coluna(nome).or(Some(padrao));
    let c: Vec<Option<usize>> = COLUNAS_POKA_YOKE
        .iter()
        .enumerate()
        .map(|(i, n)| idx(n, i))
        .collect();
    let mut out = Devices::default();
    for l in 0..t.linhas.len() {
        if linha_vazia(t, l) {
            continue;
        }
        let nome = cel(t, l, c[0]);
        let rotulo = if nome.is_empty() {
            format!("linha {}", l + 1)
        } else {
            nome.clone()
        };
        let mut notas = |i: usize| {
            let s = cel(t, l, c[i]);
            let v = nota(&s);
            if v.is_none() && !s.is_empty() {
                r.avisos.push(format!(
                    "{rotulo}: \"{}\" deve ser de 1 a 10.",
                    COLUNAS_POKA_YOKE[i]
                ));
            }
            v
        };
        let (ob, db, oa, da) = (notas(5), notas(6), notas(7), notas(8));
        let kind = tipo(&cel(t, l, c[1])).unwrap_or_else(|| {
            r.avisos.push(format!(
                "{rotulo}: o tipo deve ser contato, valor fixo ou sequência."
            ));
            Kind::Contact
        });
        let function = funcao(&cel(t, l, c[2])).unwrap_or_else(|| {
            r.avisos
                .push(format!("{rotulo}: a função deve ser controle ou alerta."));
            Function::Control
        });
        out.devices.push(Device {
            name: rotulo,
            kind,
            function,
            step: cel(t, l, c[3]),
            failure_mode: cel(t, l, c[4]),
            o_before: ob,
            d_before: db,
            o_after: oa,
            d_after: da,
            every_days: opcional(t, l, c[9])
                .filter(|v| *v >= 1.0)
                .map(|v| v.round() as u32),
            owner: cel(t, l, c[10]),
            checks: Vec::new(),
        });
    }
    let v = a.auxiliar(0);
    for l in 0..v.linhas.len() {
        if linha_vazia(&v, l) {
            continue;
        }
        let nome = v.celula(l, 0).trim();
        let Some(d) = out
            .devices
            .iter_mut()
            .find(|d| normalizar(&d.name) == normalizar(nome))
        else {
            r.avisos.push(format!(
                "A verificação da linha {} é de um dispositivo que não está na tabela (\"{nome}\").",
                l + 1
            ));
            continue;
        };
        let (Some(data), Some(ok)) = (data_iso(v.celula(l, 1)), sim_nao(v.celula(l, 2))) else {
            r.avisos.push(format!(
                "A verificação da linha {} precisa da data (DD/MM/AAAA) e de \"funcionou\" sim ou não.",
                l + 1
            ));
            continue;
        };
        d.checks.push(Check {
            date: data,
            worked: ok,
            who: v.celula(l, 3).trim().into(),
            note: v.celula(l, 4).trim().into(),
        });
    }
    for d in &mut out.devices {
        d.checks.sort_by(|x, y| x.date.cmp(&y.date));
    }
    out
}

/// A data de hoje (UTC) em `AAAA-MM-DD`.
fn hoje_iso() -> String {
    data_iso(&crate::hoje()).unwrap_or_default()
}

pub fn poka_yoke(a: &Aplicacao, r: &mut Resultado) {
    poka_yoke_em(a, &hoje_iso(), r);
}

/// O poka-yoke visto no dia `hoje` (`AAAA-MM-DD`).
pub fn poka_yoke_em(a: &Aplicacao, hoje: &str, r: &mut Resultado) {
    let d = dispositivos(a, r);
    if d.devices.is_empty() {
        r.avisos
            .push("Acrescente os dispositivos, ligados aos modos de falha do FMEA.".into());
        return;
    }
    let o = poka_yoke::compute(&d, hoje);
    let conta = |f: &dyn Fn(Effect) -> bool| o.devices.iter().filter(|s| f(s.effect)).count();
    r.kpi("Dispositivos", d.devices.len().to_string(), "");
    r.kpi(
        "Baixaram a ocorrência",
        conta(&|e| matches!(e, Effect::Occurrence | Effect::Both)).to_string(),
        "O depois < O antes",
    );
    r.kpi(
        "Baixaram a detecção",
        conta(&|e| matches!(e, Effect::Detection | Effect::Both)).to_string(),
        "D depois < D antes",
    );
    r.kpi(
        "Pedem atenção",
        o.needing_attention.to_string(),
        "verificação atrasada ou falhou",
    );
    let efeito = |e: Effect| match e {
        Effect::Unknown => "sem notas",
        Effect::None => "nenhum",
        Effect::Occurrence => "ocorrência",
        Effect::Detection => "detecção",
        Effect::Both => "ocorrência e detecção",
    };
    let delta = |v: Option<i32>| match v {
        None => String::new(),
        Some(0) => "0".into(),
        Some(x) => format!("{x:+}"),
    };
    let linhas = d
        .devices
        .iter()
        .zip(&o.devices)
        .map(|(dev, s)| {
            vec![
                dev.name.clone(),
                if dev.step.is_empty() {
                    dev.failure_mode.clone()
                } else {
                    format!("{}: {}", dev.step, dev.failure_mode)
                },
                match dev.function {
                    Function::Control => "controle".into(),
                    Function::Warning => "alerta".into(),
                },
                efeito(s.effect).into(),
                delta(s.o_drop.map(|x| -x)),
                delta(s.d_drop.map(|x| -x)),
                s.last_check.as_deref().map_or(String::new(), data_br),
                s.next_check.as_deref().map_or(String::new(), data_br),
                dev.owner.clone(),
            ]
        })
        .collect();
    r.quadro(
        "Dispositivos",
        &[
            "Dispositivo",
            "Modo de falha",
            "Função",
            "Efeito",
            "ΔO",
            "ΔD",
            "Última",
            "Próxima",
            "Responsável",
        ],
        linhas,
        &[1.4, 2.0, 0.7, 1.1, 0.4, 0.4, 0.8, 0.8, 1.0],
    );
    let quedas: Vec<(String, f64, f64)> = d
        .devices
        .iter()
        .zip(&o.devices)
        .filter(|(_, s)| s.o_drop.is_some() || s.d_drop.is_some())
        .map(|(dev, s)| {
            (
                dev.name.clone(),
                f64::from(s.o_drop.unwrap_or(0)),
                f64::from(s.d_drop.unwrap_or(0)),
            )
        })
        .collect();
    if !quedas.is_empty() {
        r.graficos.push(Chart {
            title: "Quanto cada dispositivo baixou as notas do FMEA".into(),
            bars: true,
            labels: quedas.iter().map(|q| q.0.clone()).collect(),
            series: vec![
                ("Ocorrência".into(), quedas.iter().map(|q| q.1).collect()),
                ("Detecção".into(), quedas.iter().map(|q| q.2).collect()),
            ],
            kind: ChartKind::Bars,
            ..Chart::default()
        });
    }
    for (dev, s) in d.devices.iter().zip(&o.devices) {
        for f in &s.findings {
            let tom = match f {
                poka_yoke::Finding::LastFailed | poka_yoke::Finding::Worse => Tone::Danger,
                poka_yoke::Finding::Late(_) => Tone::Danger,
                _ => Tone::Warn,
            };
            let texto = match f {
                poka_yoke::Finding::Late(d) => {
                    format!("Verificação atrasada desde {}.", data_br(d))
                }
                f => f.text(),
            };
            r.achado(tom, &dev.name, texto);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ferramenta;

    fn app(f: Ferramenta, linhas: &[&[&str]]) -> Aplicacao {
        let mut a = Aplicacao::nova("a1", f);
        a.tabela.linhas = linhas
            .iter()
            .map(|l| l.iter().map(|c| c.to_string()).collect())
            .collect();
        a
    }

    fn tabela(cols: &[&str], linhas: &[&[&str]]) -> Tabela {
        let mut t = Tabela::nova(cols);
        t.linhas = linhas
            .iter()
            .map(|l| l.iter().map(|c| c.to_string()).collect())
            .collect();
        t
    }

    #[test]
    fn qfd_reads_requirements_from_the_columns_and_finds_the_conflict() {
        let mut a = app(
            Ferramenta::Qfd,
            &[
                &["Fecha sem esforço", "5", "2", "4", "4", "1,5", "9", "3", ""],
                &["Não vaza", "4", "3", "3", "4", "", "", "9", "1"],
                &["Bonita", "2", "", "", "", "", "", "", ""],
            ],
        );
        for n in ["Torque de fechamento", "Pressão de vedação", "Rugosidade"] {
            a.tabela.adicionar_coluna(n);
        }
        // adicionar_coluna also widened the rows; put the values in the new columns.
        for l in &mut a.tabela.linhas {
            l.truncate(9);
        }
        a.auxiliares[0] = tabela(
            &COLUNAS_QFD_REQUISITOS,
            &[
                &["Torque de fechamento", "N·m", "menor", "1,2"],
                &["Pressão de vedação", "bar", "maior", "6"],
            ],
        );
        a.auxiliares[1] = tabela(
            &COLUNAS_QFD_TELHADO,
            &[&["Torque de fechamento", "Pressão de vedação", "--"]],
        );
        let mut r = Resultado::default();
        let h = casa(&a, &mut r);
        assert!(r.avisos.is_empty(), "{:?}", r.avisos);
        assert_eq!(h.requirements.len(), 3);
        assert_eq!(h.requirements[0].direction, Direction::Lower);
        assert_eq!(h.relationships[1], [0, 9, 1]);
        let r = crate::calculo::calcular(&a, "");
        assert!(r.avisos.is_empty(), "{:?}", r.avisos);
        assert_eq!(r.quadros[0].linhas[0][1], "Torque de fechamento");
        assert!(r.achados.iter().any(|x| x.titulo == "Conflito no telhado"));
        assert!(
            r.achados
                .iter()
                .any(|x| x.titulo == "Necessidade sem requisito" && x.texto.contains("Bonita"))
        );
    }

    #[test]
    fn qfd_flags_bad_cells_and_unknown_roof_names() {
        let mut a = app(Ferramenta::Qfd, &[&["Leve", "3", "", "", "", "", "5"]]);
        a.tabela.adicionar_coluna("Massa");
        a.auxiliares[1] = tabela(&COLUNAS_QFD_TELHADO, &[&["Massa", "Cor", "+"]]);
        let r = crate::calculo::calcular(&a, "");
        assert_eq!(r.avisos.len(), 2, "{:?}", r.avisos);
    }

    #[test]
    fn stack_up_by_three_methods() {
        let mut a = app(
            Ferramenta::Tolerancias,
            &[
                &["Carcaça", "50", "0,1", "0,1", "1", "normal"],
                &["Eixo", "30", "0,05", "", "-1", "normal 3"],
                &["Arruela", "2", "0,02", "0,02", "-1", "uniforme"],
            ],
        );
        a.parametros.lie = Some(17.7);
        a.parametros.lse = Some(18.3);
        let mut r = Resultado::default();
        let p = pilha(&a, &mut r);
        assert!(r.avisos.is_empty(), "{:?}", r.avisos);
        assert_eq!(p.dimensions[1].tol_upper, 0.05);
        let r = crate::calculo::calcular(&a, "");
        assert!(r.avisos.is_empty(), "{:?}", r.avisos);
        let pior = r
            .indicadores
            .iter()
            .find(|k| k.label == "Pior caso")
            .unwrap();
        assert_eq!(pior.value, "17,83 a 18,17");
        assert_eq!(r.quadros[0].linhas[0][0], "Carcaça");
        // Deterministic.
        assert_eq!(r, crate::calculo::calcular(&a, ""));
        assert_eq!(
            espalhamento("Normal 4σ"),
            Some(Spread::Normal { sigmas: 4.0 })
        );
        assert_eq!(espalhamento("gama"), None);
    }

    #[test]
    fn poka_yoke_effect_and_checks() {
        let mut a = app(
            Ferramenta::PokaYoke,
            &[
                &[
                    "Pino-guia",
                    "contato",
                    "controle",
                    "Montagem",
                    "Tampa invertida",
                    "6",
                    "5",
                    "1",
                    "5",
                    "30",
                    "Ana",
                ],
                &[
                    "Contador de parafusos",
                    "valor fixo",
                    "alerta",
                    "Aperto",
                    "Parafuso faltando",
                    "4",
                    "7",
                    "4",
                    "7",
                    "",
                    "",
                ],
            ],
        );
        a.auxiliares[0] = tabela(
            &COLUNAS_VERIFICACOES,
            &[
                &["Pino-guia", "01/09/2026", "sim", "Ana", ""],
                &["pino-guia", "2026-10-01", "não", "Bruno", "pino gasto"],
            ],
        );
        let mut r = Resultado::default();
        poka_yoke_em(&a, "2026-11-10", &mut r);
        assert!(r.avisos.is_empty(), "{:?}", r.avisos);
        assert_eq!(r.quadros[0].linhas[0][3], "ocorrência");
        assert_eq!(r.quadros[0].linhas[0][4], "-5");
        assert_eq!(r.quadros[0].linhas[0][6], "01/10/2026");
        let pede = r
            .indicadores
            .iter()
            .find(|k| k.label == "Pedem atenção")
            .unwrap();
        assert_eq!(pede.value, "1");
        assert!(
            r.achados
                .iter()
                .any(|x| x.titulo == "Pino-guia" && x.tom == Tone::Danger)
        );
        assert!(
            r.achados
                .iter()
                .any(|x| x.titulo == "Contador de parafusos" && x.texto.contains("detecção"))
        );
        assert_eq!(data_iso("9/11/2026").as_deref(), Some("2026-11-09"));
        assert_eq!(data_iso("2026-13-01"), None);
    }
}
