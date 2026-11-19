//! Os textos das receitas de análise nas cinco línguas da família: as chaves de
//! `idiomas/<código>/receitas.ftl` (`docs/traducao.md`). O nome da receita, a conclusão, os
//! avisos e os erros saem na língua de agora. Os rótulos dos valores (`Outcome::lines`)
//! ficam em português no resultado, porque são procurados pelo nome
//! ([`crate::analises::Outcome::value`]) e comparados ao refazer a análise: quem mostra
//! traduz com [`rotulo`].

use std::fmt::Display;

use utsim_idiomas::{Catalogo, Valor};

/// Os textos das receitas nas cinco línguas, embutidos de `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("receitas");

/// O texto de uma chave na língua de agora.
pub fn tx(chave: &'static str) -> &'static str {
    TEXTOS.t(chave)
}

/// O texto de uma chave com valores com nome.
pub fn fx(chave: &str, args: &[(&str, Valor)]) -> String {
    TEXTOS.f(chave, args)
}

/// Um valor: número quando se lê de volta igual, senão texto.
pub fn vx(x: &impl Display) -> Valor<'static> {
    let s = x.to_string();
    if let Ok(n) = s.parse::<i64>()
        && n.to_string() == s
    {
        return n.into();
    }
    if let Ok(f) = s.parse::<f64>()
        && f.is_finite()
        && f.to_string() == s
    {
        return f.into();
    }
    s.into()
}

/// Um número já com casas fixas (`{:.4}`), com a marca decimal da língua.
pub fn dx(s: String) -> Valor<'static> {
    utsim_idiomas::marca_decimal(&s).into()
}

/// O rótulo de um valor do resultado na língua de agora: `"média"` vira `"mean"` em inglês,
/// `"valor-p A"` vira `"p-value A"`. Um rótulo desconhecido (o nome de uma coluna, um
/// resultado antigo) volta como está.
pub fn rotulo(l: &str) -> String {
    if let Some((_, k)) = LINHAS.iter().find(|(pt, _)| *pt == l) {
        return tx(k).to_string();
    }
    for (antes, depois, k) in MODELOS {
        if let Some(meio) = l.strip_prefix(antes).and_then(|r| r.strip_suffix(depois))
            && !meio.is_empty()
        {
            return fx(k, &[("x", rotulo(meio).into())]);
        }
    }
    l.to_string()
}

/// The labels written as they are (`o.put("média", ..)`), with their keys.
const LINHAS: &[(&str, &str)] = &[
    (
        "IC 95 % da média (inferior)",
        "linha-ic_95_pct_da_media_inferior",
    ),
    (
        "IC 95 % da média (superior)",
        "linha-ic_95_pct_da_media_superior",
    ),
    ("n", "linha-n"),
    ("A²", "linha-a2"),
    ("A*²", "linha-a_estrela2"),
    ("valor-p", "linha-valor_p"),
    ("média", "linha-media"),
    ("sigma de curto prazo", "linha-sigma_de_curto_prazo"),
    ("sigma global", "linha-sigma_global"),
    ("Cp", "linha-cp"),
    ("Cpk", "linha-cpk"),
    ("Cpk IC inferior", "linha-cpk_ic_inferior"),
    ("Pp", "linha-pp"),
    ("Ppk", "linha-ppk"),
    ("PPM total (global)", "linha-ppm_total_global"),
    (
        "nível sigma (com deslocamento de 1,5)",
        "linha-nivel_sigma_com_deslocamento_de_1",
    ),
    ("valor-p A-D", "linha-valor_p_a_d"),
    ("subgrupos", "linha-subgrupos"),
    ("linha central", "linha-linha_central"),
    ("LIC", "linha-lic"),
    ("LSC", "linha-lsc"),
    ("pontos com sinal", "linha-pontos_com_sinal"),
    ("t", "linha-t"),
    ("gl", "linha-gl"),
    ("IC inferior", "linha-ic_inferior"),
    ("IC superior", "linha-ic_superior"),
    ("diferença das médias", "linha-diferenca_das_medias"),
    ("diferença média", "linha-diferenca_media"),
    ("grupos", "linha-grupos"),
    ("F", "linha-f"),
    ("R²", "linha-r2"),
    ("X²", "linha-x2"),
    ("p", "linha-p"),
    ("p1", "linha-p1"),
    ("p2", "linha-p2"),
    ("diferença", "linha-diferenca"),
    ("U1", "linha-u1"),
    (
        "deslocamento (Hodges-Lehmann)",
        "linha-deslocamento_hodges_lehmann",
    ),
    ("W+", "linha-w_mais"),
    ("pseudomediana", "linha-pseudomediana"),
    ("ndc", "linha-ndc"),
    ("desvio", "linha-desvio"),
    ("tendência", "linha-tendencia"),
    ("Cg", "linha-cg"),
    ("Cgk", "linha-cgk"),
    ("viés", "linha-vies"),
    ("%viés da tolerância", "linha-pct_vies_da_tolerancia"),
    ("inclinação", "linha-inclinacao"),
    ("intercepto", "linha-intercepto"),
    ("valor-p da inclinação", "linha-valor_p_da_inclinacao"),
    ("valor-p do intercepto", "linha-valor_p_do_intercepto"),
    ("%linearidade", "linha-pct_linearidade"),
    ("linearidade", "linha-linearidade"),
    ("itens", "linha-itens"),
    ("avaliadores", "linha-avaliadores"),
    ("repetições", "linha-repeticoes"),
    ("% entre avaliadores", "linha-pct_entre_avaliadores"),
    ("% todos contra o padrão", "linha-pct_todos_contra_o_padrao"),
    ("Levene (mediana)", "linha-levene_mediana"),
    ("valor-p Levene", "linha-valor_p_levene"),
    ("Bartlett", "linha-bartlett"),
    ("valor-p Bartlett", "linha-valor_p_bartlett"),
    ("H", "linha-h"),
    ("χ²", "linha-chi2"),
    ("Q", "linha-q"),
    ("R² ajustado", "linha-r2_ajustado"),
    ("SQ erro", "linha-sq_erro"),
    ("erro padrão residual", "linha-erro_padrao_residual"),
    ("valor-p do modelo", "linha-valor_p_do_modelo"),
    ("pontos", "linha-pontos"),
    ("alvo", "linha-alvo"),
    ("σ", "linha-sigma"),
    ("λ", "linha-lambda"),
    ("L", "linha-l"),
    ("LIC (estável)", "linha-lic_estavel"),
    ("LSC (estável)", "linha-lsc_estavel"),
    ("ARL sob controle", "linha-arl_sob_controle"),
    ("ARL com desvio de 1σ", "linha-arl_com_desvio_de_1sigma"),
    ("K", "linha-k"),
    ("sinais para cima", "linha-sinais_para_cima"),
    ("sinais para baixo", "linha-sinais_para_baixo"),
    (
        "média estimada depois do desvio",
        "linha-media_estimada_depois_do_desvio",
    ),
    ("poder com esse n", "linha-poder_com_esse_n"),
    ("corridas", "linha-corridas"),
    (
        "resposta prevista no ponto estacionário",
        "linha-resposta_no_ponto_estacionario",
    ),
    ("constante", "linha-constante"),
    ("desejabilidade geral", "linha-desejabilidade_geral"),
    ("curvatura", "linha-curvatura"),
    ("valor-p curvatura", "linha-valor_p_curvatura"),
    ("PSE de Lenth", "linha-pse_de_lenth"),
    ("margem de erro de Lenth", "linha-margem_de_erro_de_lenth"),
    ("desvio-padrão", "linha-desvio_padrao"),
    ("mínimo", "linha-minimo"),
    ("Q1", "linha-q1"),
    ("mediana", "linha-mediana"),
    ("Q3", "linha-q3"),
    ("máximo", "linha-maximo"),
    ("assimetria", "linha-assimetria"),
    ("curtose", "linha-curtose"),
    ("kappa de Fleiss", "linha-kappa_de_fleiss"),
];

/// The labels built with a name (`média de {k}`): text before, text after, key.
const MODELOS: &[(&str, &str, &str)] = &[
    ("melhor ponto ", " (real)", "linha-melhor_ponto_real"),
    ("melhor ponto ", "", "linha-melhor_ponto"),
    ("prevista ", "", "linha-prevista"),
    ("desejabilidade de ", "", "linha-desejabilidade_de"),
    ("R² de ", "", "linha-r2_de"),
    ("coef ", "", "linha-coef"),
    (
        "ponto estacionário ",
        " (real)",
        "linha-ponto_estacionario_real",
    ),
    ("ponto estacionário ", "", "linha-ponto_estacionario"),
    ("média de ", "", "linha-media_de"),
    ("%VE ", "", "linha-pct_ve"),
    ("%tolerância ", "", "linha-pct_tolerancia"),
    ("viés em ", "", "linha-vies_em"),
    ("% dentro de ", "", "linha-pct_dentro_de"),
    ("% ", " contra o padrão", "linha-pct_contra_o_padrao"),
    ("desvio de ", "", "linha-desvio_de"),
    ("posto médio de ", "", "linha-posto_medio_de"),
    ("mediana de ", "", "linha-mediana_de"),
    ("SQ ", "", "linha-sq"),
    ("valor-p ", "", "linha-valor_p_de"),
    ("coeficiente ", "", "linha-coeficiente"),
    ("VIF ", "", "linha-vif"),
    ("efeito ", "", "linha-efeito"),
];
