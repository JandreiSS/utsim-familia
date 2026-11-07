//! Ergonomics on top of the time study (stage QS14 of `docs/plano_qualidade_e_seguranca.md`,
//! section 4.3): the user marks, per element of the study (`utsim tempos`), posture, load,
//! grip and vibration on the project's own scales of a few levels. Repetition and duration
//! come from the cycles the study measured. The project's own score combines them into a
//! grade from 0 to 10 per element and per station.
//!
//! The score is written by the project, inspired by the public logic of the observational
//! methods, without copying the table of any published method. It is **monotone**: a worse
//! posture, a heavier load, more repetition or a longer duration never lowers the grade.
//! A company that uses another method registers its weights and bands by CSV ([`Metodo`]).
//!
//! The ergonomic analysis that NR-17 asks for, when it applies, is the work of the
//! responsible professional. This module organises data and does not issue that analysis.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_diagram::tempos::{Estudo, Tipo};

/// Notice of every ergonomics report.
pub const AVISO_NR17: &str = "A nota de ergonomia do UTSim é uma pontuação própria para \
estudo, ensino e comparação de métodos. Ela não é a análise ergonômica do trabalho exigida \
pela NR-17 quando aplicável, que continua sendo responsabilidade do profissional habilitado.";

/// Steps of the posture scales: 0 neutral, 1 slight, 2 marked, 3 extreme.
pub const POSTURA: [&str; 4] = ["neutra", "leve", "acentuada", "extrema"];
/// Grip: 0 good, 1 fair, 2 poor.
pub const PEGA: [&str; 3] = ["boa", "regular", "ruim"];
/// Vibration: 0 none, 1 some, 2 strong.
pub const VIBRACAO: [&str; 3] = ["nenhuma", "alguma", "forte"];

/// What the user marks for one element of one actor.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Marcacao {
    pub ator: String,
    pub elemento: String,
    /// Posture steps, 0 to 3 ([`POSTURA`]).
    pub tronco: u8,
    pub bracos: u8,
    pub pescoco: u8,
    pub pernas: u8,
    /// Load or force handled, in kg.
    pub carga_kg: f64,
    /// 0 to 2 ([`PEGA`]).
    pub pega: u8,
    /// 0 to 2 ([`VIBRACAO`]).
    pub vibracao: u8,
}

/// Weights and bands of the score. The default is the project's own method; a company
/// registers its own by CSV ([`Metodo::de_csv`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metodo {
    pub nome: String,
    /// Weights of posture, load and vibration in the base (non-negative, sum > 0).
    pub peso_postura: f64,
    pub peso_carga: f64,
    pub peso_vibracao: f64,
    /// Load steps, kg: below the first is step 0, and so on (ascending).
    pub carga_kg: [f64; 3],
    /// Repetition steps, occurrences per hour (ascending).
    pub repeticao_h: [f64; 3],
    /// Upper limits of the bands low, moderate and high (above: very high), ascending.
    pub faixas: [f64; 3],
}

impl Default for Metodo {
    fn default() -> Self {
        Metodo {
            nome: "Pontuação própria do UTSim".into(),
            peso_postura: 0.5,
            peso_carga: 0.35,
            peso_vibracao: 0.15,
            carga_kg: [3.0, 10.0, 20.0],
            repeticao_h: [60.0, 300.0, 900.0],
            faixas: [3.0, 5.0, 7.0],
        }
    }
}

/// Band names, from the lowest.
pub const FAIXAS: [&str; 4] = ["baixa", "moderada", "alta", "muito alta"];

fn degrau(x: f64, limites: &[f64; 3]) -> u8 {
    limites.iter().filter(|l| x >= **l).count() as u8
}

fn crescente(v: &[f64; 3]) -> bool {
    v.windows(2).all(|w| w[0] <= w[1]) && v.iter().all(|x| x.is_finite())
}

impl Metodo {
    pub fn problemas(&self) -> Vec<String> {
        let mut out = Vec::new();
        let pesos = [self.peso_postura, self.peso_carga, self.peso_vibracao];
        if pesos.iter().any(|p| !(p.is_finite() && *p >= 0.0)) || pesos.iter().sum::<f64>() <= 0.0 {
            out.push("os pesos devem ser números não negativos com soma maior que zero".into());
        }
        for (nome, v) in [
            ("carga_kg", &self.carga_kg),
            ("repeticao_h", &self.repeticao_h),
            ("faixas", &self.faixas),
        ] {
            if !crescente(v) {
                out.push(format!("{nome} deve ser crescente"));
            }
        }
        out
    }

    /// Grade from 0 to 10 of one element: the base (posture, load and grip, vibration, each
    /// from 0 to 1, weighted) times the exposure factor (from 0.5 to 1, by the larger of
    /// repetition and duration share). Every term grows with its factor, so the grade is
    /// monotone.
    pub fn nota(&self, m: &Marcacao, por_hora: f64, fracao: f64) -> f64 {
        let postura = (f64::from(m.tronco.min(3)) * 2.0
            + f64::from(m.bracos.min(3)) * 2.0
            + f64::from(m.pescoco.min(3))
            + f64::from(m.pernas.min(3)))
            / 18.0;
        let carga = (f64::from(degrau(m.carga_kg.max(0.0), &self.carga_kg))
            + f64::from(m.pega.min(2)))
            / 5.0;
        let vibracao = f64::from(m.vibracao.min(2)) / 2.0;
        let soma = self.peso_postura + self.peso_carga + self.peso_vibracao;
        let base =
            (self.peso_postura * postura + self.peso_carga * carga + self.peso_vibracao * vibracao)
                / soma.max(1e-12);
        let repeticao = f64::from(degrau(por_hora.max(0.0), &self.repeticao_h)) / 3.0;
        let exposicao = 0.5 + 0.5 * repeticao.max(fracao.clamp(0.0, 1.0));
        10.0 * base * exposicao
    }

    pub fn faixa(&self, nota: f64) -> &'static str {
        FAIXAS[degrau(nota, &self.faixas) as usize]
    }

    pub fn para_csv(&self) -> String {
        format!(
            "parametro;valor\nnome;{}\npeso_postura;{}\npeso_carga;{}\npeso_vibracao;{}\n\
             carga_kg;{};{};{}\nrepeticao_h;{};{};{}\nfaixas;{};{};{}\n",
            self.nome,
            self.peso_postura,
            self.peso_carga,
            self.peso_vibracao,
            self.carga_kg[0],
            self.carga_kg[1],
            self.carga_kg[2],
            self.repeticao_h[0],
            self.repeticao_h[1],
            self.repeticao_h[2],
            self.faixas[0],
            self.faixas[1],
            self.faixas[2]
        )
    }

    /// Reads a company's method: lines `parametro;valor` (or `,`), the three-step ones with
    /// three values. Missing parameters keep the project's values. A method that is not
    /// monotone (negative weight, steps not ascending) is refused.
    pub fn de_csv(texto: &str) -> Result<Metodo, String> {
        let mut m = Metodo {
            nome: "Método da empresa (importado)".into(),
            ..Metodo::default()
        };
        for (k, linha) in texto.trim_start_matches('\u{feff}').lines().enumerate() {
            let sep = if linha.contains(';') { ';' } else { ',' };
            let c: Vec<&str> = linha.split(sep).map(str::trim).collect();
            if c.len() < 2 || c[0].is_empty() || c[0] == "parametro" {
                continue;
            }
            let num = |i: usize| -> Result<f64, String> {
                c.get(i)
                    .and_then(|v| v.replace(',', ".").parse::<f64>().ok())
                    .ok_or_else(|| {
                        format!(
                            "linha {}: {} sem valor numérico na coluna {}",
                            k + 1,
                            c[0],
                            i + 1
                        )
                    })
            };
            let tres = || -> Result<[f64; 3], String> { Ok([num(1)?, num(2)?, num(3)?]) };
            match c[0] {
                "nome" => m.nome = c[1].to_string(),
                "peso_postura" => m.peso_postura = num(1)?,
                "peso_carga" => m.peso_carga = num(1)?,
                "peso_vibracao" => m.peso_vibracao = num(1)?,
                "carga_kg" => m.carga_kg = tres()?,
                "repeticao_h" => m.repeticao_h = tres()?,
                "faixas" => m.faixas = tres()?,
                outro => return Err(format!("linha {}: parâmetro desconhecido '{outro}'", k + 1)),
            }
        }
        let p = m.problemas();
        if p.is_empty() {
            Ok(m)
        } else {
            Err(p.join("\n"))
        }
    }
}

/// The grade of one element of the study.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NotaElemento {
    pub ator: String,
    pub elemento: String,
    /// Occurrences per hour (cycles per hour times occurrences per cycle).
    pub por_hora: f64,
    /// Mean time per cycle.
    pub tempo_ciclo: f64,
    /// Share of the cycle spent in the element.
    pub fracao: f64,
    pub nota: f64,
    pub faixa: &'static str,
    /// Whether the element was marked (unmarked elements count as neutral).
    pub marcado: bool,
}

/// The grade of one actor (the work station of an operator).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NotaPosto {
    pub ator: String,
    /// Highest element grade.
    pub nota: f64,
    pub faixa: &'static str,
    /// Mean of the element grades weighted by their time.
    pub media_ponderada: f64,
    pub elementos: Vec<NotaElemento>,
    /// Standard time per unit of the study, when there is one (for the comparison).
    pub tempo_padrao: Option<f64>,
}

/// Hours per time unit of the study.
fn horas(unidade: &str) -> f64 {
    match unidade {
        "min" => 1.0 / 60.0,
        "h" => 1.0,
        _ => 1.0 / 3600.0,
    }
}

/// Grades of every operator of the study. Only work elements are graded (machine running
/// alone, waits, idle, blocked and stopped time are left out).
pub fn avaliar(estudo: &Estudo, marcacoes: &[Marcacao], metodo: &Metodo) -> Vec<NotaPosto> {
    let mut out = Vec::new();
    for g in &estudo.grupos {
        let Some(op) = g.operador.as_ref() else {
            continue;
        };
        let Some(ciclo) = g.tempo_ciclo.map(|r| r.media).filter(|c| *c > 0.0) else {
            continue;
        };
        let ciclos_h = 1.0 / (ciclo * horas(&estudo.unidade));
        let mut elementos = Vec::new();
        for e in g.elementos.iter().filter(|e| &e.ator == op) {
            if matches!(
                e.tipo,
                Tipo::Automatico
                    | Tipo::EsperaOperador
                    | Tipo::Bloqueio
                    | Tipo::Ocioso
                    | Tipo::Parado
            ) {
                continue;
            }
            let ocorrencias = e.resumo.n as f64 / g.regulares().max(1) as f64;
            let por_hora = ciclos_h * ocorrencias.max(1.0);
            let fracao = (e.resumo.media * ocorrencias.min(1.0) / ciclo).clamp(0.0, 1.0);
            let m = marcacoes
                .iter()
                .find(|m| &m.ator == op && m.elemento == e.elemento);
            let neutra = Marcacao::default();
            let nota = metodo.nota(m.unwrap_or(&neutra), por_hora, fracao);
            elementos.push(NotaElemento {
                ator: op.clone(),
                elemento: e.elemento.clone(),
                por_hora,
                tempo_ciclo: e.resumo.media,
                fracao,
                nota,
                faixa: metodo.faixa(nota),
                marcado: m.is_some(),
            });
        }
        let nota = elementos.iter().map(|e| e.nota).fold(0.0, f64::max);
        let tempo: f64 = elementos.iter().map(|e| e.tempo_ciclo).sum();
        let media_ponderada = if tempo > 0.0 {
            elementos
                .iter()
                .map(|e| e.nota * e.tempo_ciclo)
                .sum::<f64>()
                / tempo
        } else {
            0.0
        };
        out.push(NotaPosto {
            ator: op.clone(),
            nota,
            faixa: metodo.faixa(nota),
            media_ponderada,
            elementos,
            tempo_padrao: g.padrao.as_ref().and_then(|p| p.por_unidade_padrao),
        });
    }
    out
}

/// One station before and after a change of method.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Comparacao {
    pub ator: String,
    pub nota_antes: f64,
    pub nota_depois: f64,
    pub padrao_antes: Option<f64>,
    pub padrao_depois: Option<f64>,
}

/// The same pair of studies the `tempos` comparison uses, with the grade beside the
/// standard time. The marks of the "after" study may differ (the change of method).
pub fn comparar(antes: &[NotaPosto], depois: &[NotaPosto]) -> Vec<Comparacao> {
    let mut atores: BTreeMap<&str, (Option<&NotaPosto>, Option<&NotaPosto>)> = BTreeMap::new();
    for p in antes {
        atores.entry(&p.ator).or_default().0 = Some(p);
    }
    for p in depois {
        atores.entry(&p.ator).or_default().1 = Some(p);
    }
    atores
        .into_iter()
        .map(|(ator, (a, d))| Comparacao {
            ator: ator.to_string(),
            nota_antes: a.map_or(0.0, |p| p.nota),
            nota_depois: d.map_or(0.0, |p| p.nota),
            padrao_antes: a.and_then(|p| p.tempo_padrao),
            padrao_depois: d.and_then(|p| p.tempo_padrao),
        })
        .collect()
}

/// Marks read from CSV: `ator;elemento;tronco;bracos;pescoco;pernas;carga_kg;pega;vibracao`
/// (header optional).
pub fn marcacoes_csv(texto: &str) -> Result<Vec<Marcacao>, String> {
    let mut out = Vec::new();
    for (k, linha) in texto.trim_start_matches('\u{feff}').lines().enumerate() {
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with("ator") {
            continue;
        }
        let sep = if linha.contains(';') { ';' } else { ',' };
        let c: Vec<&str> = linha.split(sep).map(str::trim).collect();
        if c.len() < 9 {
            return Err(format!("linha {}: esperadas 9 colunas", k + 1));
        }
        let n = |i: usize| -> Result<u8, String> {
            c[i].parse::<u8>()
                .map_err(|_| format!("linha {}: coluna {} deve ser um inteiro", k + 1, i + 1))
        };
        out.push(Marcacao {
            ator: c[0].to_string(),
            elemento: c[1].to_string(),
            tronco: n(2)?,
            bracos: n(3)?,
            pescoco: n(4)?,
            pernas: n(5)?,
            carga_kg: c[6]
                .replace(',', ".")
                .parse()
                .map_err(|_| format!("linha {}: carga_kg deve ser um número", k + 1))?,
            pega: n(7)?,
            vibracao: n(8)?,
        });
    }
    Ok(out)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn num(v: f64) -> String {
    format!("{v:.2}").replace('.', ",")
}

/// Self-contained HTML report of the grades (and of the comparison, when given), with the
/// NR-17 notice and the legal notice.
pub fn html(
    modelo: &str,
    metodo: &Metodo,
    postos: &[NotaPosto],
    comparacao: &[Comparacao],
) -> String {
    use std::fmt::Write;
    let mut s = format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>Ergonomia: {m}</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:24px;max-width:1000px;color:#222}}\
         table{{border-collapse:collapse;margin:8px 0 20px}}td,th{{border:1px solid #bbb;padding:4px 8px}}\
         .aviso{{border:2px solid #c0392b;padding:8px 12px;background:#fdf2f0}}</style></head><body>\
         <h1>Ergonomia sobre o estudo de tempos</h1><p>Modelo <b>{m}</b>. Método: {metodo}. Origem: Simulado.</p>\
         <p class=\"aviso\">{nr17}</p><p class=\"aviso\">{legal}</p>",
        m = esc(modelo),
        metodo = esc(&metodo.nome),
        nr17 = AVISO_NR17,
        legal = crate::AVISO_LEGAL
    );
    for p in postos {
        let _ = write!(
            s,
            "<h2>{}: nota {} ({})</h2><p>Média ponderada pelo tempo: {}.</p><table><tr><th>Elemento</th>\
             <th>Por hora</th><th>Tempo por ciclo</th><th>Fração do ciclo</th><th>Nota</th><th>Faixa</th><th>Marcado</th></tr>",
            esc(&p.ator),
            num(p.nota),
            p.faixa,
            num(p.media_ponderada)
        );
        for e in &p.elementos {
            let _ = write!(
                s,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&e.elemento),
                num(e.por_hora),
                num(e.tempo_ciclo),
                num(e.fracao),
                num(e.nota),
                e.faixa,
                if e.marcado { "sim" } else { "não (neutro)" }
            );
        }
        s.push_str("</table>");
    }
    if !comparacao.is_empty() {
        s.push_str("<h2>Antes e depois</h2><table><tr><th>Posto</th><th>Nota antes</th><th>Nota depois</th><th>Tempo padrão antes</th><th>Tempo padrão depois</th></tr>");
        let opt = |v: Option<f64>| v.map_or("–".to_string(), num);
        for c in comparacao {
            let _ = write!(
                s,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&c.ator),
                num(c.nota_antes),
                num(c.nota_depois),
                opt(c.padrao_antes),
                opt(c.padrao_depois)
            );
        }
        s.push_str("</table>");
    }
    let _ = write!(
        s,
        "<p class=\"aviso\">{}</p></body></html>",
        crate::AVISO_LEGAL
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every factor, one step worse, never lowers the grade: all combinations of the
    /// posture, load, grip and vibration steps, with repetition and duration around them.
    #[test]
    fn grade_is_monotone_in_every_factor() {
        let m = Metodo::default();
        let cargas = [0.0, 5.0, 15.0, 25.0];
        let por_hora = [10.0, 100.0, 500.0, 1000.0];
        let fracoes = [0.0, 0.3, 0.7, 1.0];
        let nota = |v: [u8; 4], c: usize, p: u8, vb: u8, h: usize, f: usize| {
            let mk = Marcacao {
                tronco: v[0],
                bracos: v[1],
                pescoco: v[2],
                pernas: v[3],
                carga_kg: cargas[c],
                pega: p,
                vibracao: vb,
                ..Marcacao::default()
            };
            m.nota(&mk, por_hora[h], fracoes[f])
        };
        let mut combinacoes = 0;
        for t in 0..4u8 {
            for b in 0..4u8 {
                for pe in 0..4u8 {
                    for pn in 0..4u8 {
                        for c in 0..4 {
                            for p in 0..3u8 {
                                for vb in 0..3u8 {
                                    for h in 0..4 {
                                        for f in 0..4 {
                                            let x = nota([t, b, pe, pn], c, p, vb, h, f);
                                            assert!((0.0..=10.0).contains(&x));
                                            let mut pior = Vec::new();
                                            if t < 3 {
                                                pior.push(nota([t + 1, b, pe, pn], c, p, vb, h, f));
                                            }
                                            if b < 3 {
                                                pior.push(nota([t, b + 1, pe, pn], c, p, vb, h, f));
                                            }
                                            if pe < 3 {
                                                pior.push(nota([t, b, pe + 1, pn], c, p, vb, h, f));
                                            }
                                            if pn < 3 {
                                                pior.push(nota([t, b, pe, pn + 1], c, p, vb, h, f));
                                            }
                                            if c < 3 {
                                                pior.push(nota([t, b, pe, pn], c + 1, p, vb, h, f));
                                            }
                                            if p < 2 {
                                                pior.push(nota([t, b, pe, pn], c, p + 1, vb, h, f));
                                            }
                                            if vb < 2 {
                                                pior.push(nota([t, b, pe, pn], c, p, vb + 1, h, f));
                                            }
                                            if h < 3 {
                                                pior.push(nota([t, b, pe, pn], c, p, vb, h + 1, f));
                                            }
                                            if f < 3 {
                                                pior.push(nota([t, b, pe, pn], c, p, vb, h, f + 1));
                                            }
                                            for y in pior {
                                                assert!(
                                                    y >= x - 1e-12,
                                                    "piorar baixou a nota: {x} -> {y}"
                                                );
                                            }
                                            combinacoes += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(combinacoes, 4usize.pow(4) * 4 * 3 * 3 * 4 * 4);
        // Extremes.
        assert_eq!(nota([0; 4], 0, 0, 0, 0, 0), 0.0);
        assert!((nota([3; 4], 3, 2, 2, 3, 3) - 10.0).abs() < 1e-12);
    }

    #[test]
    fn company_method_by_csv() {
        let m = Metodo::default();
        assert_eq!(Metodo::de_csv(&m.para_csv()).unwrap().faixas, m.faixas);
        let e = Metodo::de_csv("peso_postura;1\npeso_carga;0\npeso_vibracao;0\nfaixas;2;4;6\n")
            .unwrap();
        assert_eq!(e.faixa(5.0), "alta");
        assert!(Metodo::de_csv("peso_carga;-1\n").is_err());
        assert!(Metodo::de_csv("faixas;5;4;6\n").is_err());
        assert!(Metodo::de_csv("outro;1\n").is_err());
    }

    #[test]
    fn marks_from_csv() {
        let v = marcacoes_csv(
            "ator;elemento;tronco;bracos;pescoco;pernas;carga_kg;pega;vibracao\n\
             operador;carregar torno;2;1;0;0;12,5;1;0\n",
        )
        .unwrap();
        assert_eq!(v[0].carga_kg, 12.5);
        assert_eq!(v[0].tronco, 2);
        assert!(marcacoes_csv("a;b;c\n").is_err());
    }
}
