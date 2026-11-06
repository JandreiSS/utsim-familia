//! Every measure of every candidate side by side, for choosing a distribution by looking
//! at all of them at once: the position of each candidate in each criterion, which one
//! each criterion would choose, the points of the P-P and Q-Q plots and of the density
//! curves, and a short explanation of each measure, plot and family.

use crate::auto::AutoFit;
use crate::gof::TestStat;
use crate::rank::{CandidateResult, num};
use crate::report::esc;

/// One way of comparing candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Measure {
    Aic,
    Bic,
    LogLik,
    Ks,
    Ad,
    Chi2,
}

impl Measure {
    pub const ALL: [Measure; 6] = [
        Measure::Aic,
        Measure::Bic,
        Measure::LogLik,
        Measure::Ks,
        Measure::Ad,
        Measure::Chi2,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Measure::Aic => "AIC",
            Measure::Bic => "BIC",
            Measure::LogLik => "Log-verossimilhança",
            Measure::Ks => "Kolmogorov-Smirnov",
            Measure::Ad => "Anderson-Darling",
            Measure::Chi2 => "Qui-quadrado",
        }
    }

    /// Short label for table headers.
    pub fn short(self) -> &'static str {
        match self {
            Measure::Aic => "AIC",
            Measure::Bic => "BIC",
            Measure::LogLik => "LogL",
            Measure::Ks => "K-S",
            Measure::Ad => "A-D",
            Measure::Chi2 => "χ²",
        }
    }

    /// Key of its explanation ([`explain`]).
    pub fn key(self) -> &'static str {
        match self {
            Measure::Aic => "aic",
            Measure::Bic => "bic",
            Measure::LogLik => "loglik",
            Measure::Ks => "ks",
            Measure::Ad => "ad",
            Measure::Chi2 => "chi2",
        }
    }

    /// Larger is better only for the log-likelihood.
    pub fn lower_is_better(self) -> bool {
        self != Measure::LogLik
    }

    fn test(self, c: &CandidateResult) -> Option<&TestStat> {
        match self {
            Measure::Ks => c.gof.ks.as_ref(),
            Measure::Ad => c.gof.anderson_darling.as_ref(),
            Measure::Chi2 => c.gof.chi_square.as_ref(),
            _ => None,
        }
    }

    /// The value of the measure (the statistic, for the tests), `None` when the family
    /// was not fitted or the test does not apply.
    pub fn value(self, c: &CandidateResult) -> Option<f64> {
        if c.fit_error.is_some() {
            return None;
        }
        let v = match self {
            Measure::Aic => c.aic,
            Measure::Bic => c.bic,
            Measure::LogLik => c.log_likelihood,
            _ => self.test(c)?.statistic,
        };
        v.is_finite().then_some(v)
    }

    /// The p-value of a test, `None` for the information criteria.
    pub fn p_value(self, c: &CandidateResult) -> Option<f64> {
        if c.fit_error.is_some() {
            return None;
        }
        self.test(c).map(|t| t.p_value).filter(|p| p.is_finite())
    }
}

/// Every candidate in every measure.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    /// `ranks[i][m]`: position (1 = best) of candidate `i` in measure `Measure::ALL[m]`,
    /// `None` when the measure does not apply to it.
    pub ranks: Vec<[Option<usize>; 6]>,
    /// The candidate each measure would choose: the best value among the candidates the
    /// tests accept (all fitted ones when none is accepted).
    pub winners: [Option<usize>; 6],
}

impl Comparison {
    pub fn of(candidates: &[CandidateResult]) -> Comparison {
        let mut ranks = vec![[None; 6]; candidates.len()];
        let mut winners = [None; 6];
        let any_accepted = candidates.iter().any(|c| c.accepted());
        for (m, measure) in Measure::ALL.iter().enumerate() {
            let mut have: Vec<(usize, f64)> = candidates
                .iter()
                .enumerate()
                .filter_map(|(i, c)| measure.value(c).map(|v| (i, v)))
                .collect();
            have.sort_by(|a, b| {
                let o = a.1.total_cmp(&b.1);
                if measure.lower_is_better() {
                    o
                } else {
                    o.reverse()
                }
            });
            for (pos, (i, _)) in have.iter().enumerate() {
                ranks[*i][m] = Some(pos + 1);
            }
            winners[m] = have
                .iter()
                .find(|(i, _)| !any_accepted || candidates[*i].accepted())
                .map(|(i, _)| *i);
        }
        Comparison { ranks, winners }
    }

    /// The measures that would choose candidate `i`.
    pub fn chosen_by(&self, i: usize) -> Vec<Measure> {
        Measure::ALL
            .iter()
            .enumerate()
            .filter(|(m, _)| self.winners[*m] == Some(i))
            .map(|(_, m)| *m)
            .collect()
    }

    /// Candidate indices ordered by `measure` (best first; those without it last, in their
    /// original order).
    pub fn order_by(&self, measure: Measure) -> Vec<usize> {
        let m = Measure::ALL.iter().position(|x| *x == measure).unwrap_or(0);
        let mut idx: Vec<usize> = (0..self.ranks.len()).collect();
        idx.sort_by_key(|&i| (self.ranks[i][m].unwrap_or(usize::MAX), i));
        idx
    }
}

/// Points of the P-P plot of `c` (theoretical probability, empirical probability), at
/// most `max` of them, evenly spread over the sorted data.
pub fn pp_points(data: &[f64], c: &CandidateResult, max: usize) -> Vec<[f64; 2]> {
    let Some(f) = c.fitted.as_ref() else {
        return Vec::new();
    };
    let v = sorted(data);
    let n = v.len() as f64;
    spread(v.len(), max)
        .into_iter()
        .map(|i| [f.model.cdf(v[i]), (i as f64 + 0.5) / n])
        .filter(|p| p[0].is_finite())
        .collect()
}

/// Points of the Q-Q plot of `c` (theoretical quantile, sample quantile).
pub fn qq_points(data: &[f64], c: &CandidateResult, max: usize) -> Vec<[f64; 2]> {
    let Some(f) = c.fitted.as_ref() else {
        return Vec::new();
    };
    let v = sorted(data);
    let n = v.len() as f64;
    spread(v.len(), max)
        .into_iter()
        .map(|i| [f.model.quantile((i as f64 + 0.5) / n), v[i]])
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect()
}

/// The density of `c` from `lo` to `hi` in `steps` points, scaled to a histogram of `n`
/// values in bins of width `bin` (so it is drawn over the bars). Discrete families give
/// their mass at each integer.
pub fn density_curve(
    c: &CandidateResult,
    lo: f64,
    hi: f64,
    steps: usize,
    n: usize,
    bin: f64,
    discrete: bool,
) -> Vec<[f64; 2]> {
    let Some(f) = c.fitted.as_ref() else {
        return Vec::new();
    };
    let scale = n as f64 * if discrete { 1.0 } else { bin };
    if discrete {
        let (a, b) = (lo.ceil() as i64, hi.floor() as i64);
        if b < a || b - a > 10_000 {
            return Vec::new();
        }
        return (a..=b)
            .map(|k| [k as f64, f.model.pdf(k as f64) * scale])
            .filter(|p| p[1].is_finite())
            .collect();
    }
    let steps = steps.max(2);
    (0..steps)
        .map(|k| {
            let x = lo + (hi - lo) * k as f64 / (steps - 1) as f64;
            [x, f.model.pdf(x) * scale]
        })
        .filter(|p| p[1].is_finite())
        .collect()
}

fn sorted(data: &[f64]) -> Vec<f64> {
    let mut v: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
    v.sort_by(f64::total_cmp);
    v
}

fn spread(n: usize, max: usize) -> Vec<usize> {
    if n <= max || max < 2 {
        return (0..n).collect();
    }
    (0..max).map(|k| k * (n - 1) / (max - 1)).collect()
}

/// A short explanation of a measure, a plot or a family, written for people choosing a
/// distribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Explanation {
    pub key: &'static str,
    pub title: &'static str,
    /// What it is.
    pub what: &'static str,
    /// How to read it.
    pub read: &'static str,
    /// When to use it.
    pub when: &'static str,
    /// What to be careful about.
    pub care: &'static str,
}

const fn ex(
    key: &'static str,
    title: &'static str,
    what: &'static str,
    read: &'static str,
    when: &'static str,
    care: &'static str,
) -> Explanation {
    Explanation {
        key,
        title,
        what,
        read,
        when,
        care,
    }
}

/// Every explanation: the measures, the plots, then the families (key = family name).
pub const EXPLANATIONS: &[Explanation] = &[
    ex(
        "aic",
        "AIC (critério de informação de Akaike)",
        "Mede quanto a distribuição se afasta dos dados, somando o desajuste (menos duas vezes a log-verossimilhança) e uma multa de 2 por parâmetro estimado.",
        "Menor é melhor. Só a diferença entre candidatas importa: menos de 2 de diferença é empate prático; acima de 10, a pior praticamente não tem apoio dos dados.",
        "Para comparar famílias ajustadas aos mesmos dados, quando o objetivo é descrever bem o que se observou. É o critério padrão do UTSim.",
        "Não diz se a melhor é boa, só que é a menos ruim entre as candidatas. Compare só valores calculados com os mesmos dados.",
    ),
    ex(
        "bic",
        "BIC (critério de informação bayesiano)",
        "Parecido com o AIC, mas a multa por parâmetro cresce com o tamanho da amostra (logaritmo de n por parâmetro).",
        "Menor é melhor. Diferença acima de 6 já é forte a favor da menor; abaixo de 2, empate.",
        "Quando se prefere o modelo mais simples e a amostra é grande: o BIC favorece famílias com menos parâmetros mais que o AIC.",
        "Com amostras pequenas pode escolher uma família simples demais. Como o AIC, só compara candidatas entre si.",
    ),
    ex(
        "loglik",
        "Log-verossimilhança",
        "A soma dos logaritmos da densidade da distribuição ajustada em cada valor observado: quão prováveis os dados são segundo ela.",
        "Maior é melhor (menos negativo). É a base do AIC e do BIC, sem a multa pelos parâmetros.",
        "Para ver o ajuste puro. Para escolher entre famílias com número diferente de parâmetros, prefira o AIC ou o BIC.",
        "Famílias com mais parâmetros sempre ganham aqui, mesmo sem ser melhores de fato. Valores de famílias discretas e contínuas não são comparáveis.",
    ),
    ex(
        "ks",
        "Teste de Kolmogorov-Smirnov (K-S)",
        "Compara a curva acumulada dos dados com a da distribuição e mede a maior distância vertical entre as duas.",
        "Estatística menor é melhor. Valor-p acima de 0,05: os dados não contradizem a distribuição; abaixo de 0,05, ela é rejeitada.",
        "Teste geral de aderência para dados contínuos, sensível ao meio da distribuição.",
        "É pouco sensível nas caudas. Com parâmetros estimados dos próprios dados, o UTSim calcula o valor-p por reamostragem. Empates (valores repetidos) e dados discretos deixam o teste conservador. Com amostras grandes, qualquer pequena diferença rejeita.",
    ),
    ex(
        "ad",
        "Teste de Anderson-Darling (A-D)",
        "Também compara as curvas acumuladas, mas dá mais peso às diferenças nas caudas (valores muito pequenos ou muito grandes).",
        "Estatística menor é melhor. Valor-p acima de 0,05: aceita; abaixo, rejeita.",
        "Quando as caudas importam, como tempos de reparo longos ou chegadas muito espaçadas. Costuma ser o teste mais exigente dos três.",
        "Não se aplica a famílias discretas. Valores repetidos e amostras muito grandes levam a rejeições por detalhes sem importância prática.",
    ),
    ex(
        "chi2",
        "Teste qui-quadrado (χ²)",
        "Divide os dados em classes e compara quantos valores caíram em cada uma com quantos a distribuição esperava.",
        "Estatística menor é melhor. Valor-p acima de 0,05: aceita; abaixo, rejeita.",
        "Serve para dados discretos e contínuos, e é o teste natural para contagens.",
        "Precisa de pelo menos 30 valores e o resultado depende de como as classes são formadas. Com poucos dados por classe ele perde força.",
    ),
    ex(
        "pp",
        "Gráfico P-P (probabilidade × probabilidade)",
        "Cada ponto compara a probabilidade acumulada que a distribuição dá a um valor com a fração dos dados até ele.",
        "Pontos sobre a diagonal: bom ajuste. Curva em S ou afastamento no meio indicam problema no centro da distribuição.",
        "Para ver o ajuste na região central, onde está a maior parte dos valores.",
        "Mostra pouco das caudas; para elas, use o Q-Q.",
    ),
    ex(
        "qq",
        "Gráfico Q-Q (quantil × quantil)",
        "Cada ponto compara um quantil da distribuição ajustada com o quantil correspondente dos dados.",
        "Pontos sobre a diagonal: bom ajuste. Pontos que fogem nas pontas mostram caudas mais pesadas (ou mais leves) que a distribuição.",
        "Para conferir as caudas, que pesam em filas e tempos de espera longos.",
        "Os poucos pontos extremos variam muito em amostras pequenas: não decida só por eles.",
    ),
    ex(
        "Exponencial",
        "Exponencial",
        "Distribuição sem memória com um só parâmetro, a média.",
        "Compare a média e o desvio padrão dos dados: na exponencial eles são iguais.",
        "Intervalos entre chegadas aleatórias e independentes (clientes, pedidos, falhas).",
        "Tempos de serviço raramente são exponenciais: costumam ter um mínimo e menos variação.",
    ),
    ex(
        "Exponencial deslocada",
        "Exponencial deslocada",
        "Uma exponencial que começa num valor mínimo em vez de zero.",
        "O deslocamento é o menor tempo possível; acima dele, o comportamento é exponencial.",
        "Intervalos ou tempos que têm um mínimo físico e, acima dele, variação aleatória.",
        "O mínimo é estimado dos dados: um valor muito baixo isolado puxa o deslocamento.",
    ),
    ex(
        "Erlang",
        "Erlang",
        "Soma de k etapas exponenciais iguais; uma Gama com forma inteira.",
        "Quanto maior k, menos variável e mais simétrica.",
        "Tempos de serviço formados por várias etapas parecidas.",
        "A forma é inteira: se a Gama ajustar bem com forma não inteira, prefira a Gama.",
    ),
    ex(
        "Gama",
        "Gama",
        "Família flexível de valores positivos, com forma e escala.",
        "Forma menor que 1: muitos valores pequenos; igual a 1: exponencial; maior: corcova cada vez mais simétrica.",
        "Tempos de serviço e de reparo, durações positivas com assimetria à direita.",
        "Não tem mínimo além de zero: se os dados têm um mínimo claro, veja a versão deslocada.",
    ),
    ex(
        "Gama deslocada",
        "Gama deslocada",
        "Uma Gama que começa num valor mínimo.",
        "O deslocamento é o menor tempo possível; acima dele, o formato da Gama.",
        "Tempos com um mínimo físico (montagem, transporte) e cauda à direita.",
        "Um parâmetro a mais: confira no AIC ou BIC se o ganho compensa.",
    ),
    ex(
        "Weibull",
        "Weibull",
        "Família de valores positivos muito usada em confiabilidade, com forma e escala.",
        "Forma menor que 1: taxa de falha caindo; igual a 1: exponencial; maior que 1: desgaste.",
        "Tempo até falhar, vida útil, tempos de serviço.",
        "Forma e escala trocam de papel entre programas: confira o significado no campo do UTSim.",
    ),
    ex(
        "Weibull deslocada",
        "Weibull deslocada",
        "Uma Weibull que começa num valor mínimo.",
        "O deslocamento é o menor valor possível.",
        "Vida útil ou tempos com um mínimo garantido.",
        "Um parâmetro a mais que a Weibull: confira se o ganho compensa.",
    ),
    ex(
        "Lognormal",
        "Lognormal",
        "Valores cujo logaritmo segue a Normal: positivos, com cauda longa à direita.",
        "A média fica acima da mediana; quanto maior o desvio do logaritmo, mais longa a cauda.",
        "Tempos de reparo e de tarefas manuais, que às vezes demoram muito mais que o normal.",
        "A cauda longa gera valores raros muito grandes na simulação: confira o Q-Q na ponta direita.",
    ),
    ex(
        "Lognormal deslocada",
        "Lognormal deslocada",
        "Uma lognormal que começa num valor mínimo.",
        "O deslocamento é o menor valor possível.",
        "Tempos manuais com um mínimo físico e cauda longa.",
        "Um parâmetro a mais: confira o AIC ou o BIC.",
    ),
    ex(
        "Normal",
        "Normal",
        "A curva em sino, simétrica em torno da média.",
        "Média e desvio padrão descrevem tudo; cerca de 95 % dos valores ficam a dois desvios da média.",
        "Medidas que variam pouco e de forma simétrica, como dimensões ou tempos de máquina estáveis.",
        "Pode sortear valores negativos: para tempos, só serve quando a média está muitos desvios acima de zero.",
    ),
    ex(
        "Uniforme",
        "Uniforme",
        "Todos os valores entre um mínimo e um máximo são igualmente prováveis.",
        "Só o mínimo e o máximo importam.",
        "Quando só se sabe a faixa, sem nenhum valor mais comum.",
        "Raramente descreve bem dados reais; o mínimo e o máximo saem dos extremos da amostra.",
    ),
    ex(
        "Triangular",
        "Triangular",
        "Mínimo, máximo e um valor mais provável (moda).",
        "A moda puxa a forma: perto do mínimo, cauda à direita.",
        "Estimativas de especialistas (otimista, mais provável, pessimista) quando há poucos dados.",
        "Limitada nos dois lados: não gera valores fora da faixa observada.",
    ),
    ex(
        "Beta",
        "Beta",
        "Família flexível limitada entre um mínimo e um máximo, com duas formas.",
        "As formas controlam onde os valores se concentram e a assimetria.",
        "Proporções e tempos com limites conhecidos dos dois lados.",
        "Os limites saem dos dados; com amostras pequenas eles ficam estreitos demais.",
    ),
    ex(
        "Log-logística",
        "Log-logística",
        "Valores positivos cujo logaritmo segue a logística; parecida com a lognormal, com cauda um pouco mais pesada.",
        "A escala é a mediana; a forma controla a variação.",
        "Tempos de serviço e de reparo com alguns valores muito longos.",
        "Para forma pequena a média pode nem existir: confira a cauda no Q-Q.",
    ),
    ex(
        "Pearson V",
        "Pearson V (gama inversa)",
        "O inverso de uma Gama: positiva, com cauda direita longa.",
        "Valores concentrados perto do mínimo e alguns muito grandes.",
        "Tempos de tarefa com atrasos ocasionais grandes.",
        "Cauda muito pesada: a média e a variância podem não existir para formas pequenas.",
    ),
    ex(
        "Pearson VI",
        "Pearson VI (beta prima)",
        "Razão de duas Gamas: positiva, flexível, com cauda direita.",
        "Três parâmetros dão muitas formas possíveis.",
        "Tempos de serviço quando Gama e lognormal não servem.",
        "Muitos parâmetros: confira se o BIC ainda a prefere.",
    ),
    ex(
        "Poisson",
        "Poisson",
        "Contagem de eventos num intervalo, com média igual à variância.",
        "Compare a média e a variância das contagens: na Poisson elas são iguais.",
        "Número de chegadas, defeitos ou pedidos por período.",
        "Se a variância for bem maior que a média, prefira a binomial negativa.",
    ),
    ex(
        "Geométrica",
        "Geométrica",
        "Número de tentativas até o primeiro sucesso.",
        "Valores pequenos são os mais comuns e a chance cai a cada valor.",
        "Retrabalhos até passar na inspeção, tentativas até conseguir.",
        "Só se aplica a contagens inteiras.",
    ),
    ex(
        "Binomial",
        "Binomial",
        "Número de sucessos em n tentativas independentes com a mesma chance.",
        "Média n·p; a variância é sempre menor que a média.",
        "Itens bons num lote de tamanho fixo.",
        "Precisa de um número máximo de tentativas fixo; se não houver, veja Poisson.",
    ),
    ex(
        "Binomial negativa",
        "Binomial negativa",
        "Contagens com mais variação que a Poisson.",
        "A variância é maior que a média; a diferença mostra quanta variação a mais existe.",
        "Chegadas por período que oscilam muito entre dias ou turnos.",
        "Dois parâmetros: com poucos dados, a Poisson pode ser preferível.",
    ),
];

/// The explanation of `key` (a measure key, `pp`, `qq` or a family name).
pub fn explain(key: &str) -> Option<&'static Explanation> {
    EXPLANATIONS.iter().find(|e| e.key == key)
}

/// The section of the report with every measure of every candidate side by side, the
/// position in each criterion, the choice of each criterion, the one used, and the
/// explanations (already escaped HTML).
pub fn comparison_html(fit: &AutoFit) -> String {
    let cands = &fit.report.ranking.candidates;
    let cmp = Comparison::of(cands);
    let mut h = String::new();
    h.push_str("<h2>Comparação de todas as medidas</h2>");
    h.push_str(
        "<p>Cada linha é uma distribuição candidata, ajustada aos mesmos dados. A posição em \
         cada critério vem entre parênteses (1 = melhor). A última coluna diz quais critérios \
         a escolheriam (entre as aceitas pelos testes, quando há alguma).</p>",
    );
    h.push_str("<div class=\"scroll\"><table><thead><tr><th>família</th><th>parâmetros</th>");
    for m in Measure::ALL {
        h.push_str(&format!("<th>{}</th>", esc(m.short())));
        if m.p_value_applies() {
            h.push_str(&format!("<th>{} p</th>", esc(m.short())));
        }
    }
    h.push_str("<th>escolhida por</th></tr></thead><tbody>");
    for i in cmp.order_by(Measure::Aic) {
        let c = &cands[i];
        let used = fit.chosen == Some(i);
        let class = if used {
            "best"
        } else if c.fit_error.is_some() {
            "na"
        } else if c.accepted() {
            "acc"
        } else {
            "rej"
        };
        h.push_str(&format!(
            "<tr class=\"{class}\"><td>{}{}</td>",
            esc(&c.family),
            if used { " (usada)" } else { "" }
        ));
        let params = if c.fit_error.is_some() {
            c.fit_error.clone().unwrap_or_default()
        } else {
            c.params
                .iter()
                .map(|(k, v)| format!("{k} = {}", num(*v)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        h.push_str(&format!("<td>{}</td>", esc(&params)));
        for (m, measure) in Measure::ALL.iter().enumerate() {
            let v = measure.value(c).map_or("-".into(), |v| {
                format!("{} ({})", num(v), cmp.ranks[i][m].unwrap_or(0))
            });
            h.push_str(&format!("<td>{}</td>", esc(&v)));
            if measure.p_value_applies() {
                let p = measure.p_value(c).map_or("-".into(), num);
                h.push_str(&format!("<td>{}</td>", esc(&p)));
            }
        }
        let by: Vec<&str> = cmp.chosen_by(i).iter().map(|m| m.short()).collect();
        h.push_str(&format!("<td>{}</td></tr>", esc(&by.join(", "))));
    }
    h.push_str("</tbody></table></div>");
    h.push_str("<h3>O que cada medida quer dizer</h3><dl class=\"explica\">");
    for e in EXPLANATIONS.iter().filter(|e| {
        e.key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }) {
        h.push_str(&explanation_html(e));
    }
    h.push_str("</dl><h3>As famílias candidatas</h3><dl class=\"explica\">");
    for c in cands {
        if let Some(e) = explain(&c.family) {
            h.push_str(&explanation_html(e));
        }
    }
    h.push_str("</dl>");
    h
}

fn explanation_html(e: &Explanation) -> String {
    format!(
        "<dt>{}</dt><dd><p>{}</p><p><b>Como ler:</b> {}</p><p><b>Quando usar:</b> {}</p><p><b>Cuidados:</b> {}</p></dd>",
        esc(e.title),
        esc(e.what),
        esc(e.read),
        esc(e.when),
        esc(e.care)
    )
}

impl Measure {
    /// The goodness-of-fit tests have a p-value.
    pub fn p_value_applies(self) -> bool {
        matches!(self, Measure::Ks | Measure::Ad | Measure::Chi2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto::{AutoConfig, auto_fit};
    use crate::report::FitConfig;

    /// Exponential values with mean 5 from a small fixed generator.
    fn sample(n: usize) -> Vec<f64> {
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let u = (x >> 11) as f64 / (1u64 << 53) as f64;
                -5.0 * (1.0 - u).ln()
            })
            .collect()
    }

    fn fit(data: &[f64]) -> AutoFit {
        let config = AutoConfig {
            fit: FitConfig {
                gof: crate::gof::GofConfig {
                    bootstrap: 20,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        auto_fit("teste", data, &config).unwrap()
    }

    #[test]
    fn every_measure_ranks_the_fitted_candidates_and_names_a_winner() {
        let data = sample(200);
        let f = fit(&data);
        let cands = &f.report.ranking.candidates;
        let cmp = Comparison::of(cands);
        assert_eq!(cmp.ranks.len(), cands.len());
        for (m, measure) in Measure::ALL.iter().enumerate() {
            let mut have: Vec<usize> = cmp.ranks.iter().filter_map(|r| r[m]).collect();
            have.sort();
            // Positions 1..=k, each once.
            assert_eq!(have, (1..=have.len()).collect::<Vec<_>>(), "{measure:?}");
            let w = cmp.winners[m].expect("a winner");
            assert!(measure.value(&cands[w]).is_some());
        }
        // A candidate that was not fitted has no position anywhere.
        for (i, c) in cands.iter().enumerate() {
            if c.fit_error.is_some() {
                assert!(cmp.ranks[i].iter().all(Option::is_none), "{}", c.family);
            }
        }
        // The AIC order agrees with the values; the log-likelihood is ordered the other way.
        let o = cmp.order_by(Measure::Aic);
        let aics: Vec<f64> = o
            .iter()
            .filter_map(|&i| Measure::Aic.value(&cands[i]))
            .collect();
        assert!(aics.windows(2).all(|w| w[0] <= w[1]));
        let o = cmp.order_by(Measure::LogLik);
        let ll: Vec<f64> = o
            .iter()
            .filter_map(|&i| Measure::LogLik.value(&cands[i]))
            .collect();
        assert!(ll.windows(2).all(|w| w[0] >= w[1]));
        // The automatic choice is one the tests accept.
        assert!(cands[f.chosen.unwrap()].accepted());
    }

    #[test]
    fn winners_come_from_the_accepted_when_there_are_any() {
        let data = sample(150);
        let f = fit(&data);
        let cands = &f.report.ranking.candidates;
        assert!(cands.iter().any(|c| c.accepted()));
        let cmp = Comparison::of(cands);
        for w in cmp.winners.iter().flatten() {
            assert!(cands[*w].accepted(), "{}", cands[*w].family);
        }
        let all: usize = (0..cands.len()).map(|i| cmp.chosen_by(i).len()).sum();
        assert_eq!(all, 6, "each measure chooses exactly one");
    }

    #[test]
    fn plot_points_and_curves_follow_the_fitted_model() {
        let data = sample(120);
        let f = fit(&data);
        let c = &f.report.ranking.candidates[f.chosen.unwrap()];
        let pp = pp_points(&data, c, 50);
        assert_eq!(pp.len(), 50);
        assert!(
            pp.iter()
                .all(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]))
        );
        assert!(
            pp.windows(2).all(|w| w[0][0] <= w[1][0] + 1e-12),
            "monotone"
        );
        let qq = qq_points(&data, c, 500);
        assert_eq!(qq.len(), data.len());
        let curve = density_curve(c, 0.0, 30.0, 61, data.len(), 2.0, false);
        assert_eq!(curve.len(), 61);
        // The scaled curve has about the area of the histogram (n values).
        let area: f64 = curve
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]) * (w[0][1] + w[1][1]) / 2.0)
            .sum();
        assert!(
            (area / 2.0 - data.len() as f64).abs() < data.len() as f64 * 0.1,
            "{area}"
        );
    }

    #[test]
    fn every_measure_plot_and_family_is_explained() {
        for m in Measure::ALL {
            assert!(explain(m.key()).is_some(), "{m:?}");
        }
        assert!(explain("pp").is_some() && explain("qq").is_some());
        for name in crate::auto::family_names() {
            let e = explain(name).unwrap_or_else(|| panic!("sem explicação: {name}"));
            for t in [e.what, e.read, e.when, e.care] {
                assert!(t.len() > 20, "{name}: {t}");
            }
        }
    }

    #[test]
    fn the_report_has_the_comparison_of_all_measures() {
        let data = sample(100);
        let f = fit(&data);
        let html = f.to_html("teste");
        assert!(html.contains("Comparação de todas as medidas"));
        assert!(html.contains("escolhida por"));
        assert!(html.contains("Anderson-Darling"));
        assert!(html.contains("(usada)"));
        assert!(html.contains("Como ler:"));
    }
}
