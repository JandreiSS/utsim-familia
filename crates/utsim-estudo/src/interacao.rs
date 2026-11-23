//! Os modelos de interação das ferramentas desenhadas na tela (`utsim_visual`): o que cada
//! gesto faz nos dados de uma [`Aplicacao`]. Os dados continuam no formato de sempre: as
//! categorias e causas do Ishikawa, as cadeias dos 5 porquês (a árvore é montada a partir
//! delas), a tabela do Pareto, da FMEA e do QFD e o telhado do QFD na tabela auxiliar.
//!
//! - Ishikawa: [`mover_causa`] (arrastar um cartão para outro osso), [`acrescentar_causa`],
//!   [`editar_causa`], [`remover_causa`], [`renomear_categoria`], [`remover_categoria`];
//!   [`comecar_porques`] abre um 5 porquês a partir de uma causa;
//! - 5 porquês: [`arvore_dos_porques`] e [`porque_filho`], [`editar_no`], [`remover_no`];
//! - Pareto: [`linhas_da_barra`] (clicar numa barra filtra a tabela);
//! - QFD: [`casa_da_tabela`], [`definir_relacao`], [`definir_correlacao`].

use crate::calculo::{self, Resultado};
use crate::{Aplicacao, Cadeia, Categoria, Estudo, Ferramenta};

fn normalizar(s: &str) -> String {
    s.trim().to_lowercase()
}

// ---- Ishikawa --------------------------------------------------------------------------

/// Passa a causa `causa` da categoria `de` para o fim da categoria `para`. Devolve se mudou.
pub fn mover_causa(cats: &mut [Categoria], de: usize, causa: usize, para: usize) -> bool {
    if de == para || para >= cats.len() || cats.get(de).is_none_or(|c| causa >= c.causas.len()) {
        return false;
    }
    let texto = cats[de].causas.remove(causa);
    cats[para].causas.push(texto);
    true
}

/// Uma causa nova no fim da categoria.
pub fn acrescentar_causa(cats: &mut [Categoria], osso: usize, texto: &str) {
    if let Some(c) = cats.get_mut(osso)
        && !texto.trim().is_empty()
    {
        c.causas.push(texto.trim().to_string());
    }
}

/// Troca o texto da causa (vazio a remove).
pub fn editar_causa(cats: &mut [Categoria], osso: usize, causa: usize, texto: &str) {
    if texto.trim().is_empty() {
        remover_causa(cats, osso, causa);
    } else if let Some(c) = cats.get_mut(osso).and_then(|c| c.causas.get_mut(causa)) {
        *c = texto.trim().to_string();
    }
}

pub fn remover_causa(cats: &mut [Categoria], osso: usize, causa: usize) {
    if let Some(c) = cats.get_mut(osso)
        && causa < c.causas.len()
    {
        c.causas.remove(causa);
    }
}

pub fn renomear_categoria(cats: &mut [Categoria], osso: usize, nome: &str) {
    if let Some(c) = cats.get_mut(osso)
        && !nome.trim().is_empty()
    {
        c.nome = nome.trim().to_string();
    }
}

pub fn remover_categoria(cats: &mut Vec<Categoria>, osso: usize) {
    if osso < cats.len() {
        cats.remove(osso);
    }
}

/// As causas que já começam uma cadeia de 5 porquês no estudo (o primeiro porquê de uma
/// cadeia), sem diferença de maiúsculas.
pub fn causas_com_porques(e: &Estudo) -> Vec<String> {
    e.aplicacoes
        .iter()
        .filter(|a| a.ferramenta == Ferramenta::CincoPorques)
        .flat_map(|a| a.cadeias.iter())
        .filter_map(|c| c.porques.first())
        .map(|p| normalizar(p))
        .filter(|p| !p.is_empty())
        .collect()
}

/// Começa um 5 porquês com a causa: na primeira aplicação de 5 porquês do estudo (criada no
/// fim quando não há), numa cadeia que começa com a causa. Não repete uma cadeia que já
/// começa com ela e usa uma cadeia vazia quando há. Devolve o id da aplicação.
pub fn comecar_porques(e: &mut Estudo, causa: &str) -> String {
    let causa = causa.trim();
    let id = match e
        .aplicacoes
        .iter()
        .find(|a| a.ferramenta == Ferramenta::CincoPorques)
    {
        Some(a) => a.id.clone(),
        None => {
            let id = e.novo_id_aplicacao();
            let mut a = Aplicacao::nova(&id, Ferramenta::CincoPorques);
            a.cadeias.clear();
            e.aplicacoes.push(a);
            id
        }
    };
    let a = e.aplicacao_mut(&id).expect("acabou de achar ou criar");
    if causa.is_empty()
        || a.cadeias.iter().any(|c| {
            c.porques
                .first()
                .is_some_and(|p| normalizar(p) == normalizar(causa))
        })
    {
        return id;
    }
    match a
        .cadeias
        .iter_mut()
        .find(|c| c.porques.iter().all(|p| p.trim().is_empty()) && c.acao.trim().is_empty())
    {
        Some(c) => c.porques = vec![causa.to_string()],
        None => a.cadeias.push(Cadeia {
            porques: vec![causa.to_string()],
            acao: String::new(),
        }),
    }
    id
}

// ---- 5 porquês em árvore -----------------------------------------------------------------

/// O papel de um nó da árvore dos porquês.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Papel {
    Problema,
    /// Um porquê; `nivel` 1 é o primeiro (a posição `nivel - 1` nas cadeias).
    Porque {
        nivel: usize,
    },
    /// A ação da cadeia.
    Acao {
        cadeia: usize,
    },
}

/// Um nó da árvore.
#[derive(Clone, Debug, PartialEq)]
pub struct NoPorque {
    pub pai: Option<usize>,
    pub texto: String,
    pub papel: Papel,
    /// As cadeias que passam por este nó.
    pub cadeias: Vec<usize>,
}

/// A árvore das cadeias: o problema na raiz; cadeias que começam com os mesmos porquês
/// (mesmo texto, sem diferença de maiúsculas) dividem os nós até onde se separam; porquês
/// vazios nunca se juntam. Cada cadeia com porquês (ou com ação) termina na sua ação.
pub fn arvore_dos_porques(problema: &str, cadeias: &[Cadeia]) -> Vec<NoPorque> {
    let mut nos = vec![NoPorque {
        pai: None,
        texto: problema.to_string(),
        papel: Papel::Problema,
        cadeias: (0..cadeias.len()).collect(),
    }];
    for (c, cadeia) in cadeias.iter().enumerate() {
        let mut atual = 0;
        for (d, p) in cadeia.porques.iter().enumerate() {
            let chave = normalizar(p);
            let existente = (!chave.is_empty())
                .then(|| {
                    nos.iter().position(|n| {
                        n.pai == Some(atual)
                            && n.papel == Papel::Porque { nivel: d + 1 }
                            && normalizar(&n.texto) == chave
                    })
                })
                .flatten();
            atual = match existente {
                Some(i) => {
                    nos[i].cadeias.push(c);
                    i
                }
                None => {
                    nos.push(NoPorque {
                        pai: Some(atual),
                        texto: p.clone(),
                        papel: Papel::Porque { nivel: d + 1 },
                        cadeias: vec![c],
                    });
                    nos.len() - 1
                }
            };
        }
        if !cadeia.porques.is_empty() || !cadeia.acao.trim().is_empty() {
            nos.push(NoPorque {
                pai: Some(atual),
                texto: cadeia.acao.clone(),
                papel: Papel::Acao { cadeia: c },
                cadeias: vec![c],
            });
        }
    }
    nos
}

/// Acrescenta um porquê abaixo do nó `no`. Abaixo do problema: numa cadeia vazia, ou numa
/// nova. Abaixo de um porquê que é o último de alguma cadeia: no fim dela. Abaixo de um
/// porquê que já tem filhos: uma cadeia nova (um ramo) com os mesmos porquês até ele, logo
/// depois das cadeias do ramo.
pub fn porque_filho(cadeias: &mut Vec<Cadeia>, nos: &[NoPorque], no: usize, texto: &str) {
    let texto = texto.trim();
    let Some(n) = nos.get(no) else { return };
    if texto.is_empty() {
        return;
    }
    match n.papel {
        Papel::Problema => match cadeias.iter_mut().find(|c| c.porques.is_empty()) {
            Some(c) => c.porques.push(texto.into()),
            None => cadeias.push(Cadeia {
                porques: vec![texto.into()],
                acao: String::new(),
            }),
        },
        Papel::Porque { nivel } => {
            if let Some(&c) = n
                .cadeias
                .iter()
                .find(|&&c| cadeias.get(c).is_some_and(|x| x.porques.len() == nivel))
            {
                cadeias[c].porques.push(texto.into());
            } else if let Some(&primeira) = n.cadeias.first() {
                let mut porques: Vec<String> = cadeias[primeira].porques[..nivel].to_vec();
                porques.push(texto.into());
                let depois = n.cadeias.iter().max().map_or(cadeias.len(), |m| m + 1);
                cadeias.insert(
                    depois.min(cadeias.len()),
                    Cadeia {
                        porques,
                        acao: String::new(),
                    },
                );
            }
        }
        Papel::Acao { .. } => {}
    }
}

/// Troca o texto do nó: o problema vai para `efeito`, um porquê muda em todas as cadeias
/// que passam por ele, a ação muda na sua cadeia.
pub fn editar_no(
    efeito: &mut String,
    cadeias: &mut [Cadeia],
    nos: &[NoPorque],
    no: usize,
    texto: &str,
) {
    let Some(n) = nos.get(no) else { return };
    let texto = texto.trim().to_string();
    match n.papel {
        Papel::Problema => *efeito = texto,
        Papel::Porque { nivel } => {
            for &c in &n.cadeias {
                if let Some(p) = cadeias
                    .get_mut(c)
                    .and_then(|x| x.porques.get_mut(nivel - 1))
                {
                    p.clone_from(&texto);
                }
            }
        }
        Papel::Acao { cadeia } => {
            if let Some(c) = cadeias.get_mut(cadeia) {
                c.acao = texto;
            }
        }
    }
}

/// Remove o nó e o que está abaixo dele. As cadeias que passavam por ele param antes dele;
/// as que ficam sem ação e repetem o começo de outra (ou vazias, quando há outras) saem.
pub fn remover_no(cadeias: &mut Vec<Cadeia>, nos: &[NoPorque], no: usize) {
    let Some(n) = nos.get(no) else { return };
    match n.papel {
        Papel::Problema => {}
        Papel::Acao { cadeia } => {
            if let Some(c) = cadeias.get_mut(cadeia) {
                c.acao.clear();
            }
        }
        Papel::Porque { nivel } => {
            for &c in &n.cadeias {
                if let Some(x) = cadeias.get_mut(c) {
                    x.porques.truncate(nivel - 1);
                }
            }
            let chave = |c: &Cadeia| c.porques.iter().map(|p| normalizar(p)).collect::<Vec<_>>();
            let mut i = 0;
            while i < cadeias.len() {
                let c = &cadeias[i];
                let k = chave(c);
                let redundante = c.acao.trim().is_empty()
                    && cadeias.len() > 1
                    && (c.porques.is_empty()
                        || cadeias.iter().enumerate().any(|(j, o)| {
                            j != i && {
                                let ko = chave(o);
                                ko.len() >= k.len()
                                    && ko[..k.len()] == k[..]
                                    && (ko.len() > k.len() || j < i)
                            }
                        }));
                if redundante {
                    cadeias.remove(i);
                } else {
                    i += 1;
                }
            }
        }
    }
}

// ---- Pareto ----------------------------------------------------------------------------

/// As linhas da tabela que entram na barra `nome` do Pareto. A barra "Outros" junta as
/// categorias que não têm barra própria.
pub fn linhas_da_barra(a: &Aplicacao, nome: &str) -> Vec<usize> {
    let (p, cc) = calculo::pareto_da_tabela(a);
    let proprias: Vec<String> = p
        .rows
        .iter()
        .filter(|r| !r.others)
        .map(|r| normalizar(&r.name))
        .collect();
    let outros = p.rows.iter().any(|r| r.others && r.name == nome);
    let alvo = normalizar(nome);
    a.tabela
        .linhas
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            let c = normalizar(l.get(cc).map_or("", |s| s.as_str()));
            !c.is_empty()
                && if outros {
                    !proprias.contains(&c)
                } else {
                    c == alvo
                }
        })
        .map(|(i, _)| i)
        .collect()
}

// ---- QFD -------------------------------------------------------------------------------

/// A casa da qualidade lida da tabela, com onde cada parte fica na tabela.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CasaDaTabela {
    /// A linha da tabela de cada necessidade.
    pub linhas: Vec<usize>,
    /// A coluna da tabela de cada requisito técnico.
    pub colunas: Vec<usize>,
    pub necessidades: Vec<(String, f64)>,
    pub requisitos: Vec<String>,
    /// 0, 1, 3 ou 9 por necessidade e requisito.
    pub relacoes: Vec<Vec<u8>>,
    /// (requisito a, requisito b, -2 a 2).
    pub telhado: Vec<(usize, usize, i8)>,
    /// O peso de cada requisito, em %, como no resultado.
    pub pesos: Vec<f64>,
}

const FIXAS_QFD: [&str; 6] = calculo::COLUNAS_QFD;

/// A casa da qualidade da aplicação (as mesmas regras do resultado).
pub fn casa_da_tabela(a: &Aplicacao) -> CasaDaTabela {
    let t = &a.tabela;
    let fixas: Vec<usize> = FIXAS_QFD
        .iter()
        .enumerate()
        .filter_map(|(k, n)| t.coluna(n).or((k == 0).then_some(0)))
        .collect();
    let colunas: Vec<usize> = (0..t.colunas.len())
        .filter(|c| !fixas.contains(c) && !t.colunas[*c].trim().is_empty())
        .collect();
    let requisitos: Vec<String> = colunas
        .iter()
        .map(|&c| t.colunas[c].trim().to_string())
        .collect();
    let cn = t.coluna("Necessidade").unwrap_or(0);
    let ci = t.coluna("Importância");
    let linhas: Vec<usize> = (0..t.linhas.len())
        .filter(|&l| t.linhas[l].iter().any(|c| !c.trim().is_empty()))
        .collect();
    let necessidades = linhas
        .iter()
        .map(|&l| {
            (
                t.celula(l, cn).trim().to_string(),
                ci.and_then(|c| crate::tabela::numero(t.celula(l, c)))
                    .unwrap_or(0.0),
            )
        })
        .collect();
    let relacoes = linhas
        .iter()
        .map(|&l| {
            colunas
                .iter()
                .map(|&c| match t.celula(l, c).trim() {
                    "9" => 9,
                    "3" => 3,
                    "1" => 1,
                    _ => 0,
                })
                .collect()
        })
        .collect();
    let aux = a.auxiliar(1);
    let indice = |n: &str| {
        requisitos
            .iter()
            .position(|q| normalizar(q) == normalizar(n))
    };
    let telhado = (0..aux.linhas.len())
        .filter_map(|l| {
            let (x, y) = (indice(aux.celula(l, 0))?, indice(aux.celula(l, 1))?);
            let v = match aux.celula(l, 2).trim() {
                "++" => 2,
                "+" => 1,
                "-" => -1,
                "--" => -2,
                _ => return None,
            };
            (x != y).then_some((x, y, v))
        })
        .collect();
    let casa = calculo::projeto::casa(a, &mut Resultado::default());
    let saida = utsim_ferramentas::qfd::compute(&casa);
    let pesos = if saida.requirements.len() == requisitos.len() {
        saida.requirements.iter().map(|r| r.weight_pct).collect()
    } else {
        Vec::new()
    };
    CasaDaTabela {
        linhas,
        colunas,
        necessidades,
        requisitos,
        relacoes,
        telhado,
        pesos,
    }
}

/// Grava a relação (0 apaga) da necessidade `i` com o requisito `j` na tabela.
pub fn definir_relacao(a: &mut Aplicacao, casa: &CasaDaTabela, i: usize, j: usize, valor: u8) {
    let (Some(&l), Some(&c)) = (casa.linhas.get(i), casa.colunas.get(j)) else {
        return;
    };
    let t = &mut a.tabela;
    if let Some(linha) = t.linhas.get_mut(l) {
        if linha.len() <= c {
            linha.resize(c + 1, String::new());
        }
        linha[c] = if valor == 0 {
            String::new()
        } else {
            valor.to_string()
        };
    }
}

/// Grava a correlação (0 apaga a linha) entre os requisitos `x` e `y` no telhado.
pub fn definir_correlacao(a: &mut Aplicacao, casa: &CasaDaTabela, x: usize, y: usize, valor: i8) {
    let (Some(na), Some(nb)) = (casa.requisitos.get(x), casa.requisitos.get(y)) else {
        return;
    };
    a.completar_auxiliares();
    let Some(t) = a.auxiliares.get_mut(1) else {
        return;
    };
    let par = |l: &Vec<String>| {
        let (p, q) = (
            normalizar(l.first().map_or("", |s| s)),
            normalizar(l.get(1).map_or("", |s| s)),
        );
        (p == normalizar(na) && q == normalizar(nb)) || (p == normalizar(nb) && q == normalizar(na))
    };
    let texto = match valor {
        2 => "++",
        1 => "+",
        -1 => "-",
        -2 => "--",
        _ => "",
    };
    match t.linhas.iter().position(par) {
        Some(l) if texto.is_empty() => t.remover_linha(l),
        Some(l) => {
            let linha = &mut t.linhas[l];
            if linha.len() < 3 {
                linha.resize(3, String::new());
            }
            linha[2] = texto.into();
        }
        None if !texto.is_empty() => {
            // Uma linha vazia no fim (a de partida) é aproveitada.
            let vazia = t
                .linhas
                .iter()
                .position(|l| l.iter().all(|c| c.trim().is_empty()));
            let linha = vec![na.clone(), nb.clone(), texto.to_string()];
            match vazia {
                Some(l) => t.linhas[l] = linha,
                None => t.linhas.push(linha),
            }
        }
        None => {}
    }
}

/// Quantas seções do A3 têm texto.
pub fn secoes_preenchidas(a: &Aplicacao) -> usize {
    a.secoes
        .iter()
        .filter(|s| !s.texto.trim().is_empty())
        .count()
}

/// Uma tabela pequena para os testes.
#[cfg(test)]
fn tabela(cols: &[&str], linhas: &[&[&str]]) -> crate::Tabela {
    let mut t = crate::Tabela::nova(cols);
    t.linhas = linhas
        .iter()
        .map(|l| l.iter().map(|s| s.to_string()).collect())
        .collect();
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats() -> Vec<Categoria> {
        vec![
            Categoria {
                nome: "Máquina".into(),
                causas: vec!["Ferramenta gasta".into(), "Folga no eixo".into()],
            },
            Categoria {
                nome: "Método".into(),
                causas: vec!["Sem instrução".into()],
            },
        ]
    }

    #[test]
    fn arrastar_uma_causa_para_outro_osso_muda_a_categoria() {
        let mut c = cats();
        assert!(mover_causa(&mut c, 0, 1, 1));
        assert_eq!(c[0].causas, ["Ferramenta gasta"]);
        assert_eq!(c[1].causas, ["Sem instrução", "Folga no eixo"]);
        // Soltar no mesmo osso ou fora não muda nada.
        assert!(!mover_causa(&mut c, 1, 0, 1));
        assert!(!mover_causa(&mut c, 1, 5, 0));
        assert!(!mover_causa(&mut c, 0, 0, 9));
        assert_eq!(c[0].causas.len() + c[1].causas.len(), 3);
    }

    #[test]
    fn editar_vazio_remove_e_acrescentar_ignora_vazio() {
        let mut c = cats();
        acrescentar_causa(&mut c, 1, "  ");
        acrescentar_causa(&mut c, 1, " Treino curto ");
        assert_eq!(c[1].causas, ["Sem instrução", "Treino curto"]);
        editar_causa(&mut c, 0, 0, "Ferramenta sem troca");
        assert_eq!(c[0].causas[0], "Ferramenta sem troca");
        editar_causa(&mut c, 0, 0, "");
        assert_eq!(c[0].causas, ["Folga no eixo"]);
        renomear_categoria(&mut c, 1, "Método de trabalho");
        assert_eq!(c[1].nome, "Método de trabalho");
        remover_categoria(&mut c, 0);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn uma_causa_vira_o_comeco_de_um_5_porques() {
        let mut e = Estudo::default();
        let mut ish = Aplicacao::nova("a1", Ferramenta::Ishikawa);
        ish.categorias = cats();
        e.aplicacoes.push(ish);
        let id = comecar_porques(&mut e, "Folga no eixo");
        let a = e.aplicacao(&id).expect("criou");
        assert_eq!(a.ferramenta, Ferramenta::CincoPorques);
        assert_eq!(a.cadeias.len(), 1);
        assert_eq!(a.cadeias[0].porques, ["Folga no eixo"]);
        // De novo não repete; outra causa abre outra cadeia na mesma aplicação.
        assert_eq!(comecar_porques(&mut e, "folga no eixo "), id);
        assert_eq!(comecar_porques(&mut e, "Sem instrução"), id);
        let a = e.aplicacao(&id).expect("existe");
        assert_eq!(a.cadeias.len(), 2);
        assert_eq!(causas_com_porques(&e), ["folga no eixo", "sem instrução"]);
        assert_eq!(e.aplicacoes.len(), 2);
    }

    fn cadeia(p: &[&str], acao: &str) -> Cadeia {
        Cadeia {
            porques: p.iter().map(|s| s.to_string()).collect(),
            acao: acao.into(),
        }
    }

    #[test]
    fn cadeias_com_o_mesmo_comeco_viram_um_ramo() {
        let c = vec![
            cadeia(&["Máquina parou", "Sem óleo"], "Plano de lubrificação"),
            cadeia(&["máquina parou ", "Sensor falhou"], ""),
            cadeia(&["Falta de peça"], ""),
        ];
        let nos = arvore_dos_porques("Atraso", &c);
        let filhos = |i: usize| -> Vec<usize> {
            (0..nos.len()).filter(|&j| nos[j].pai == Some(i)).collect()
        };
        let raiz = filhos(0);
        assert_eq!(raiz.len(), 2, "máquina parou e falta de peça");
        let maq = raiz[0];
        assert_eq!(nos[maq].cadeias, [0, 1]);
        let ramos = filhos(maq);
        assert_eq!(ramos.len(), 2);
        assert!(
            ramos
                .iter()
                .all(|&r| nos[r].papel == Papel::Porque { nivel: 2 })
        );
        let acoes: Vec<&NoPorque> = nos
            .iter()
            .filter(|n| matches!(n.papel, Papel::Acao { .. }))
            .collect();
        assert_eq!(acoes.len(), 3, "cada cadeia termina na sua ação");
        assert_eq!(acoes[0].texto, "Plano de lubrificação");
        // Porquês vazios nunca se juntam.
        let v = vec![cadeia(&[""], ""), cadeia(&[""], "")];
        assert_eq!(
            arvore_dos_porques("", &v)
                .iter()
                .filter(|n| n.pai == Some(0))
                .count(),
            2
        );
    }

    #[test]
    fn acrescentar_abaixo_estende_a_cadeia_ou_abre_um_ramo() {
        let mut c = vec![cadeia(&["A", "B"], "")];
        let nos = arvore_dos_porques("P", &c);
        let b = nos.iter().position(|n| n.texto == "B").expect("B");
        porque_filho(&mut c, &nos, b, "C");
        assert_eq!(c[0].porques, ["A", "B", "C"]);
        // A já tem filho: um ramo novo com o mesmo começo.
        let nos = arvore_dos_porques("P", &c);
        let a = nos.iter().position(|n| n.texto == "A").expect("A");
        porque_filho(&mut c, &nos, a, "D");
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].porques, ["A", "D"]);
        let nos = arvore_dos_porques("P", &c);
        assert_eq!(nos.iter().filter(|n| n.texto == "A").count(), 1);
        // Abaixo do problema: usa a cadeia vazia, senão abre outra.
        let mut v = vec![Cadeia::default()];
        let nos = arvore_dos_porques("P", &v);
        porque_filho(&mut v, &nos, 0, "Primeiro");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].porques, ["Primeiro"]);
        let nos = arvore_dos_porques("P", &v);
        porque_filho(&mut v, &nos, 0, "Outro");
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn editar_um_no_muda_todas_as_cadeias_do_ramo_e_remover_corta() {
        let mut c = vec![cadeia(&["A", "B"], "x"), cadeia(&["A", "C"], "")];
        let mut efeito = String::new();
        let nos = arvore_dos_porques("P", &c);
        let a = nos.iter().position(|n| n.texto == "A").expect("A");
        editar_no(&mut efeito, &mut c, &nos, a, "A2");
        assert_eq!(
            (c[0].porques[0].as_str(), c[1].porques[0].as_str()),
            ("A2", "A2")
        );
        editar_no(&mut efeito, &mut c, &nos, 0, "Problema novo");
        assert_eq!(efeito, "Problema novo");
        let acao = nos
            .iter()
            .position(|n| n.papel == Papel::Acao { cadeia: 1 })
            .expect("ação");
        editar_no(&mut efeito, &mut c, &nos, acao, "Treinar");
        assert_eq!(c[1].acao, "Treinar");
        // Remover C: a cadeia 1 fica só com A2 e a ação; continua (tem ação).
        let nos = arvore_dos_porques("P", &c);
        let cc = nos.iter().position(|n| n.texto == "C").expect("C");
        remover_no(&mut c, &nos, cc);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].porques, ["A2"]);
        // Sem ação, ela repete o começo da outra e sai.
        c[1].acao.clear();
        let nos = arvore_dos_porques("P", &c);
        let b = nos.iter().position(|n| n.texto == "B").expect("B");
        remover_no(&mut c, &nos, b);
        assert_eq!(c.len(), 1, "a sem ação repetia o começo da outra e saiu");
        assert_eq!(c[0].porques, ["A2"]);
        assert_eq!(c[0].acao, "x");
        let nos = arvore_dos_porques("P", &c);
        let a = nos.iter().position(|n| n.texto == "A2").expect("A2");
        remover_no(&mut c, &nos, a);
        assert_eq!(c.len(), 1, "com ação, a cadeia fica");
        assert!(c[0].porques.is_empty());
        let nos = arvore_dos_porques("P", &c);
        assert_eq!(nos[1].papel, Papel::Acao { cadeia: 0 });
        assert_eq!(nos[1].pai, Some(0), "a ação fica abaixo do problema");
    }

    #[test]
    fn clicar_na_barra_filtra_as_linhas_da_categoria() {
        let mut a = Aplicacao::nova("a1", Ferramenta::Pareto);
        a.tabela = tabela(
            &["Categoria", "Quantidade"],
            &[
                &["Risco", "30"],
                &["Mancha", "10"],
                &["risco ", "25"],
                &["Bolha", "1"],
                &["Poeira", "1"],
            ],
        );
        a.parametros.outros_abaixo = 3.0;
        assert_eq!(linhas_da_barra(&a, "Risco"), [0, 2]);
        assert_eq!(linhas_da_barra(&a, "Mancha"), [1]);
        let (p, _) = calculo::pareto_da_tabela(&a);
        let outros = p.rows.iter().find(|r| r.others).expect("outros");
        assert_eq!(linhas_da_barra(&a, &outros.name), [3, 4]);
        assert!(linhas_da_barra(&a, "Nada").is_empty());
    }

    #[test]
    fn clicar_na_casa_grava_a_relacao_e_o_telhado() {
        let mut a = Aplicacao::nova("a1", Ferramenta::Qfd);
        let mut cols: Vec<&str> = calculo::COLUNAS_QFD.to_vec();
        cols.extend(["Torque", "Vedação"]);
        a.tabela = tabela(
            &cols,
            &[
                &["Fecha fácil", "5", "", "", "", "", "9", ""],
                &["", "", "", "", "", "", "", ""],
                &["Não vaza", "4", "", "", "", "", "", "1"],
            ],
        );
        let casa = casa_da_tabela(&a);
        assert_eq!(casa.requisitos, ["Torque", "Vedação"]);
        assert_eq!(casa.linhas, [0, 2], "a linha vazia fica de fora");
        assert_eq!(casa.relacoes, [vec![9, 0], vec![0, 1]]);
        assert_eq!(casa.pesos.len(), 2);
        definir_relacao(&mut a, &casa, 1, 0, 3);
        assert_eq!(a.tabela.celula(2, 6), "3");
        definir_relacao(&mut a, &casa, 0, 0, 0);
        assert_eq!(a.tabela.celula(0, 6), "");
        definir_correlacao(&mut a, &casa, 1, 0, -2);
        let casa = casa_da_tabela(&a);
        assert_eq!(casa.telhado, [(1, 0, -2)]);
        definir_correlacao(&mut a, &casa, 0, 1, 1);
        assert_eq!(
            casa_da_tabela(&a).telhado,
            [(1, 0, 1)],
            "a mesma linha, nos dois sentidos"
        );
        definir_correlacao(&mut a, &casa, 0, 1, 0);
        assert!(casa_da_tabela(&a).telhado.is_empty());
    }
}
