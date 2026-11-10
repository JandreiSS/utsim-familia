//! Um estudo de demonstração com todas as ferramentas preenchidas: os testes conferem os
//! relatórios com ele, as capturas das telas o mostram e o manual o usa nos exemplos.

use crate::{Aplicacao, Assunto, Cadeia, Estudo, Ferramenta, Origem, Tabela};

fn linhas(t: &mut Tabela, rows: &[&[&str]]) {
    t.linhas = rows
        .iter()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect();
}

/// Valores determinísticos em torno de `m` (sem gerador aleatório).
fn serie(n: usize, m: f64, a: f64, fase: f64) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let x = i as f64;
            m + a * ((x * 1.7 + fase).sin() * 0.6 + (x * 0.61 + fase * 2.0).cos() * 0.4)
        })
        .collect()
}

/// O estudo do furo fora de posição no torno, com todas as ferramentas.
pub fn estudo_demonstracao() -> Estudo {
    let mut e = Estudo {
        id: "demo".into(),
        titulo: "Furo fora de posição na tampa".into(),
        assunto: Assunto {
            problema: "Furo fora de posição na tampa usinada".into(),
            modelo: "modelos/inspecao_e_retrabalho.ron".into(),
            objeto: "torno".into(),
            posto: String::new(),
        },
        autor: "Ana Souza".into(),
        criado: "09/11/2026".into(),
        conclusao: "A causa raiz é o desgaste do gabarito. A troca a cada 5 mil peças e o \
                    novo plano de controle trazem o Ppk acima de 1,33.\n\nO MSA confirmou que \
                    o calibrador atende para o diâmetro."
            .into(),
        ..Estudo::default()
    };
    let mut add = |f: Ferramenta, ajustar: &dyn Fn(&mut Aplicacao)| {
        let mut a = Aplicacao::nova(&e.novo_id_aplicacao(), f);
        ajustar(&mut a);
        e.aplicacoes.push(a);
    };
    add(Ferramenta::FolhaVerificacao, &|a| {
        linhas(
            &mut a.tabela,
            &[
                &["Rebarba", "9", "11", "8", "10", "12"],
                &["Risco", "4", "5", "3", "6", "4"],
                &["Furo deslocado", "3", "2", "4", "3", "14"],
                &["Mancha", "1", "2", "2", "1", "3"],
            ],
        );
    });
    add(Ferramenta::Estratificacao, &|a| {
        a.tabela.colunas = vec!["Turno".into(), "Deslocamento (mm)".into()];
        let mut rows = Vec::new();
        for (t, m) in [("A", 0.08), ("B", 0.09), ("C", 0.21)] {
            for v in serie(8, m, 0.03, m * 10.0) {
                rows.push(vec![t.to_string(), crate::num(v, 3)]);
            }
        }
        a.tabela.linhas = rows;
    });
    add(Ferramenta::Pareto, &|a| {
        a.titulo = "Pareto dos defeitos da semana".into();
        linhas(
            &mut a.tabela,
            &[
                &["Rebarba", "50", "2"],
                &["Furo deslocado", "26", "18"],
                &["Risco", "22", "3"],
                &["Mancha", "9", "1"],
                &["Empeno", "4", "12"],
            ],
        );
    });
    add(Ferramenta::Histograma, &|a| {
        a.titulo = "Histograma do diâmetro".into();
        a.tabela.colunas = vec!["Diâmetro (mm)".into()];
        a.tabela.linhas = serie(80, 10.0, 0.18, 0.3)
            .into_iter()
            .map(|v| vec![crate::num(v, 4)])
            .collect();
        a.parametros.lie = Some(9.6);
        a.parametros.lse = Some(10.4);
        a.parametros.alvo = Some(10.0);
        a.origem = Origem::Simulado {
            modelo: "inspecao_e_retrabalho.ron".into(),
            objeto: "torno".into(),
            leitura: "característica diâmetro".into(),
            replicacoes: 1,
            quando: "09/11/2026".into(),
        };
    });
    add(Ferramenta::Dispersao, &|a| {
        a.titulo = "Temperatura × deslocamento".into();
        a.tabela.colunas = vec!["Temperatura (°C)".into(), "Deslocamento (mm)".into()];
        a.tabela.linhas = (0..25)
            .map(|i| {
                let t = 18.0 + i as f64 * 0.9;
                let d = 0.02 + 0.006 * (t - 18.0) + 0.01 * ((i as f64) * 2.3).sin();
                vec![crate::num(t, 1), crate::num(d, 4)]
            })
            .collect();
    });
    add(Ferramenta::CartaControle, &|a| {
        a.titulo = "Carta X̄-R do diâmetro".into();
        a.parametros.subgrupo = 5;
        let mut xs = serie(100, 10.0, 0.12, 1.1);
        for x in xs.iter_mut().skip(85).take(5) {
            *x += 0.35;
        }
        a.tabela.linhas = xs
            .iter()
            .enumerate()
            .map(|(i, v)| vec![format!("{}", i / 5 + 1), crate::num(*v, 4)])
            .collect();
    });
    add(Ferramenta::Capacidade, &|a| {
        a.titulo = "Capacidade do diâmetro".into();
        a.tabela.colunas = vec!["Diâmetro (mm)".into()];
        a.tabela.linhas = serie(100, 10.05, 0.2, 2.2)
            .into_iter()
            .map(|v| vec![crate::num(v, 4)])
            .collect();
        a.parametros.lie = Some(9.6);
        a.parametros.lse = Some(10.4);
        a.parametros.subgrupo = 5;
        a.origem = Origem::Importado {
            arquivo: "diametro_semana45.csv".into(),
            hash: "3f8a1c0e9b7d2a4c6e8f0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f".into(),
            aba: String::new(),
        };
    });
    add(Ferramenta::Msa, &|a| {
        a.titulo = "R&R do calibrador de diâmetro".into();
        a.parametros.tolerancia = Some(0.8);
        let mut rows = Vec::new();
        for p in 1..=10 {
            let base = 9.7 + p as f64 * 0.06;
            for (o, vies) in [("Ana", 0.0), ("Bruno", 0.004), ("Caio", -0.003)] {
                for k in 0..3 {
                    let ruido = 0.006 * ((p * 7 + k * 3) as f64 + vies * 1000.0).sin();
                    rows.push(vec![
                        format!("P{p}"),
                        o.to_string(),
                        crate::num(base + vies + ruido, 4),
                    ]);
                }
            }
        }
        a.tabela.linhas = rows;
    });
    add(Ferramenta::Ishikawa, &|a| {
        let causas: [&[&str]; 6] = [
            &["Gabarito gasto", "Folga no fuso"],
            &["Sem troca programada do gabarito"],
            &["Chapa empenada", "Lote fora"],
            &["Treino do turno C"],
            &["Calibrador vencido"],
            &["Temperatura do galpão"],
        ];
        for (c, cs) in a.categorias.iter_mut().zip(causas) {
            c.causas = cs.iter().map(|s| s.to_string()).collect();
        }
    });
    add(Ferramenta::CincoPorques, &|a| {
        a.cadeias = vec![
            Cadeia {
                porques: vec![
                    "O furo sai deslocado no turno C".into(),
                    "A peça encosta fora do batente".into(),
                    "O gabarito está gasto".into(),
                    "Não há troca programada do gabarito".into(),
                ],
                acao: "Trocar o gabarito a cada 5 mil peças e registrar no plano de manutenção"
                    .into(),
            },
            Cadeia {
                porques: vec![
                    "O operador não percebe o desvio".into(),
                    "A inspeção é só no fim do lote".into(),
                    "O plano de controle não prevê medição no posto".into(),
                ],
                acao: "Medir 1 peça a cada 50 no posto, com carta X̄-R".into(),
            },
        ];
    });
    add(Ferramenta::Fmea, &|a| {
        linhas(
            &mut a.tabela,
            &[
                &[
                    "Fixação",
                    "Peça fora do batente",
                    "Furo deslocado",
                    "8",
                    "Gabarito gasto",
                    "6",
                    "Inspeção visual",
                    "Medição no fim do lote",
                    "6",
                    "Troca programada do gabarito",
                ],
                &[
                    "Furação",
                    "Broca quebrada",
                    "Furo incompleto",
                    "7",
                    "Desgaste da broca",
                    "4",
                    "Troca por horas",
                    "Sensor de carga",
                    "3",
                    "",
                ],
                &[
                    "Furação",
                    "Diâmetro acima",
                    "Montagem solta",
                    "9",
                    "Broca errada",
                    "2",
                    "Kit por ordem",
                    "Calibrador passa-não-passa",
                    "4",
                    "",
                ],
                &[
                    "Rebarbação",
                    "Rebarba residual",
                    "Corte na montagem",
                    "5",
                    "Escova gasta",
                    "5",
                    "",
                    "Inspeção visual",
                    "7",
                    "Escova com troca semanal",
                ],
            ],
        );
    });
    add(Ferramenta::A3, &|a| {
        let textos = [
            "O refugo da tampa subiu de 2,1 % para 4,8 % em seis meses.",
            "O Pareto mostra rebarba e furo deslocado com 70 % dos defeitos.\nO turno C concentra os deslocamentos.",
            "Voltar a 2 % de refugo até o fim do trimestre.",
            "Ishikawa e 5 porquês apontam o gabarito gasto como causa raiz.",
            "Troca programada do gabarito.\nMedição no posto com carta X̄-R.",
            "Gabarito novo até 15/11 (Bruno).\nPlano de controle revisado até 20/11 (Ana).",
            "Carta de controle diária e Ppk mensal.",
        ];
        for (s, t) in a.secoes.iter_mut().zip(textos) {
            s.texto = t.into();
        }
    });
    add(Ferramenta::Qfd, &|a| {
        a.titulo = "Casa da qualidade da tampa".into();
        for r in ["Posição do furo", "Altura da rebarba", "Rugosidade"] {
            a.tabela.adicionar_coluna(r);
        }
        linhas(
            &mut a.tabela,
            &[
                &["Monta sem forçar", "5", "3", "4", "5", "1,5", "9", "3", "0"],
                &["Não corta a mão", "4", "2", "4", "4", "1,2", "0", "9", "1"],
                &["Bom acabamento", "2", "3", "3", "3", "", "0", "3", "9"],
            ],
        );
        linhas(
            &mut a.auxiliares[0],
            &[
                &["Posição do furo", "mm", "alvo", "0"],
                &["Altura da rebarba", "mm", "menor", "0,05"],
                &["Rugosidade", "µm Ra", "menor", "1,6"],
            ],
        );
        linhas(
            &mut a.auxiliares[1],
            &[&["Altura da rebarba", "Rugosidade", "+"]],
        );
    });
    add(Ferramenta::Tolerancias, &|a| {
        a.titulo = "Folga entre a tampa e o corpo".into();
        a.parametros.lie = Some(0.1);
        a.parametros.lse = Some(0.5);
        linhas(
            &mut a.tabela,
            &[
                &["Corpo", "40", "0,05", "0,05", "1", "normal"],
                &["Tampa", "38", "0,08", "0,08", "-1", "normal"],
                &["Junta", "1,7", "0,03", "0,03", "-1", "uniforme"],
            ],
        );
    });
    add(Ferramenta::PokaYoke, &|a| {
        a.titulo = "Dispositivos à prova de erro da furação".into();
        linhas(
            &mut a.tabela,
            &[
                &[
                    "Gabarito com pino",
                    "contato",
                    "controle",
                    "Furação",
                    "Furo deslocado",
                    "6",
                    "6",
                    "2",
                    "6",
                    "30",
                    "Bruno",
                ],
                &[
                    "Sensor de carga",
                    "valor fixo",
                    "alerta",
                    "Furação",
                    "Broca quebrada",
                    "4",
                    "6",
                    "4",
                    "3",
                    "7",
                    "Ana",
                ],
            ],
        );
        linhas(
            &mut a.auxiliares[0],
            &[
                &["Gabarito com pino", "15/10/2026", "sim", "Bruno", ""],
                &[
                    "Sensor de carga",
                    "03/11/2026",
                    "sim",
                    "Ana",
                    "parou a máquina",
                ],
            ],
        );
    });
    e
}
