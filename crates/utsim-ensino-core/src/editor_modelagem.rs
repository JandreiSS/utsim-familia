//! Stage D of the mission editor (`docs/plano_ensino_e_jogo.md`, 6.2): modelling missions.
//! The model the teacher opened becomes the reference model (the answer key), the student
//! starts from another model the teacher loads, and the check criteria come suggested from
//! the reference model. The check runs the reference model against its own criteria (it
//! must pass all of them) and the starting model (it must not pass all of them).

use utsim_model::{Model, ObjectKind};

use crate::construtor::{sugerir_alavancas, sugerir_metricas};
use crate::editor::{Edicao, ItemAlavanca, ItemMetrica};
use crate::missao::{Termo, Tolerancia};
use crate::modelagem::{Criterio, Modelagem, ResultadoCriterio, estrutura, tem_ciclo, tipo};

/// The file name of the reference model inside the mission folder.
pub const ARQUIVO_GABARITO: &str = "gabarito.ron";

/// The object types a criterion can count, as written in the model.
pub const TIPOS_DE_OBJETO: &[&str] = &[
    "Processor",
    "Queue",
    "Combiner",
    "Separator",
    "Source",
    "Sink",
    "Transporter",
    "Dispatcher",
    "Conveyor",
    "Rack",
];

/// The kinds of criterion the editor offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoCriterio {
    Medida,
    Objetos,
    Quebras,
    Retrabalho,
    Capacidade,
    Operadores,
}

impl TipoCriterio {
    pub const TODOS: [TipoCriterio; 6] = [
        TipoCriterio::Medida,
        TipoCriterio::Objetos,
        TipoCriterio::Quebras,
        TipoCriterio::Retrabalho,
        TipoCriterio::Capacidade,
        TipoCriterio::Operadores,
    ];

    pub fn de(c: &Criterio) -> TipoCriterio {
        match c {
            Criterio::Medida { .. } => TipoCriterio::Medida,
            Criterio::Objetos { .. } => TipoCriterio::Objetos,
            Criterio::Quebras { .. } => TipoCriterio::Quebras,
            Criterio::Retrabalho { .. } => TipoCriterio::Retrabalho,
            Criterio::Capacidade { .. } => TipoCriterio::Capacidade,
            Criterio::Operadores { .. } => TipoCriterio::Operadores,
        }
    }

    /// A new criterion of this kind, filled from the reference model `g` when it helps.
    pub fn padrao(self, g: Option<&Model>) -> Criterio {
        let saida = g
            .and_then(|g| {
                g.objects
                    .iter()
                    .find(|o| matches!(o.kind, ObjectKind::Sink))
                    .map(|o| o.name.clone())
            })
            .unwrap_or_else(|| "saida".into());
        match self {
            TipoCriterio::Medida => Criterio::Medida {
                rotulo: format!("Itens que chegam a {saida}"),
                valor: vec![Termo {
                    chave: format!("{saida}.input"),
                    fator: 1.0,
                }],
                tolerancia: Tolerancia::Relativa(0.05),
                peso: 1.0,
            },
            TipoCriterio::Objetos => Criterio::Objetos {
                rotulo: "Processadores do processo".into(),
                tipo: "Processor".into(),
                minimo: 1,
                peso: 1.0,
            },
            TipoCriterio::Quebras => Criterio::Quebras {
                rotulo: "O modelo tem quebras".into(),
                peso: 1.0,
            },
            TipoCriterio::Retrabalho => Criterio::Retrabalho {
                rotulo: "As peças reprovadas voltam (retrabalho)".into(),
                peso: 1.0,
            },
            TipoCriterio::Capacidade => Criterio::Capacidade {
                rotulo: "Postos em paralelo".into(),
                minimo: 2,
                peso: 1.0,
            },
            TipoCriterio::Operadores => Criterio::Operadores {
                rotulo: "O processo depende de operadores".into(),
                peso: 1.0,
            },
        }
    }
}

/// The criteria suggested by the reference model `g`: the count of each type of object it
/// has, its breakdowns, rework, parallel servers and operators, and the measures of its
/// exits and processors.
pub fn sugerir_criterios(g: &Model) -> Vec<Criterio> {
    let mut out = Vec::new();
    let mut tipos: Vec<(&str, u32)> = Vec::new();
    for o in &g.objects {
        let t = tipo(&o.kind);
        if matches!(t, "Source" | "Sink") {
            continue;
        }
        match tipos.iter_mut().find(|(x, _)| *x == t) {
            Some((_, n)) => *n += 1,
            None => tipos.push((t, 1)),
        }
    }
    for (t, n) in tipos {
        out.push(Criterio::Objetos {
            rotulo: format!("Ao menos {n} objeto(s) do tipo {t}"),
            tipo: t.into(),
            minimo: n,
            peso: 1.0,
        });
    }
    let com_quebra: Vec<&str> = g
        .objects
        .iter()
        .filter(|o| o.downtime.is_some())
        .map(|o| o.name.as_str())
        .collect();
    if !com_quebra.is_empty() {
        out.push(Criterio::Quebras {
            rotulo: format!("Há quebras ({})", com_quebra.join(", ")),
            peso: 1.0,
        });
    }
    if tem_ciclo(g) {
        out.push(TipoCriterio::Retrabalho.padrao(Some(g)));
    }
    let maior = g
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            ObjectKind::Processor { capacity, .. } => Some((*capacity as u32, o.name.as_str())),
            _ => None,
        })
        .max_by_key(|(c, _)| *c);
    if let Some((c, nome)) = maior
        && c > 1
    {
        out.push(Criterio::Capacidade {
            rotulo: format!("{nome} atende {c} itens ao mesmo tempo"),
            minimo: c,
            peso: 1.0,
        });
    }
    if g.objects.iter().any(|o| {
        matches!(
            &o.kind,
            ObjectKind::Processor {
                operators: Some(_),
                ..
            }
        )
    }) {
        out.push(TipoCriterio::Operadores.padrao(Some(g)));
    }
    for m in sugerir_metricas(g) {
        if !m.divisor.is_empty() {
            continue;
        }
        out.push(Criterio::Medida {
            rotulo: m.rotulo,
            valor: m.valor,
            tolerancia: Tolerancia::Relativa(0.05),
            peso: if m.peso > 0.0 { 1.0 } else { 2.0 },
        });
    }
    for o in &g.objects {
        if matches!(o.kind, ObjectKind::Processor { .. }) {
            out.push(Criterio::Medida {
                rotulo: format!("Utilização de {}", o.name),
                valor: vec![Termo {
                    chave: format!("{}.utilization", o.name),
                    fator: 1.0,
                }],
                tolerancia: Tolerancia::Absoluta(0.05),
                peso: 1.0,
            });
        }
    }
    out
}

/// What the check of a modelling mission found: the verdict of each criterion for the
/// reference model and for the student's starting model.
#[derive(Clone, Debug, PartialEq)]
pub struct ConferenciaModelagem {
    pub gabarito: Vec<ResultadoCriterio>,
    pub inicial: Vec<ResultadoCriterio>,
}

impl ConferenciaModelagem {
    /// The problems: a criterion the reference model fails, or a starting model that
    /// already passes everything (the mission teaches nothing).
    pub fn erros(&self) -> Vec<String> {
        let mut e: Vec<String> = self
            .gabarito
            .iter()
            .filter(|k| !k.ok)
            .map(|k| format!("o gabarito falha em \"{}\": {}", k.rotulo, k.detalhe))
            .collect();
        if !self.inicial.is_empty() && self.inicial.iter().all(|k| k.ok) {
            e.push(
                "o modelo inicial já cumpre todos os critérios: a missão não ensina nada".into(),
            );
        }
        e
    }
}

impl Edicao {
    /// Whether the mission is a modelling one.
    pub fn e_modelagem(&self) -> bool {
        self.base.modelagem.is_some()
    }

    /// The reference model, read from its text.
    pub fn gabarito(&self) -> Option<Model> {
        self.gabarito_ron
            .as_deref()
            .and_then(|t| Model::parse(t).ok())
    }

    /// Turns the mission into a modelling one: the model being edited becomes the
    /// reference model, with the suggested criteria. Levers and measures leave the
    /// mission (a modelling mission has none). The student's starting model is the same
    /// until the teacher loads another one ([`Self::trocar_modelo_inicial`]).
    pub fn virar_modelagem(&mut self) {
        if self.e_modelagem() {
            return;
        }
        self.gabarito_ron = Some(self.modelo_ron.clone());
        self.base.modelagem = Some(Modelagem {
            gabarito: ARQUIVO_GABARITO.into(),
            replicacoes: None,
            criterios: sugerir_criterios(&self.modelo),
        });
        for a in &mut self.alavancas {
            a.usar = false;
        }
        for m in &mut self.metricas {
            m.usar = false;
        }
        self.base.referencia.clear();
    }

    /// Back to a mission of levers: the reference model becomes the model of the mission
    /// again, and the criteria are dropped.
    pub fn deixar_modelagem(&mut self) {
        if let Some(g) = self.gabarito_ron.take()
            && g != self.modelo_ron
        {
            // The answer key is the complete model: the levers come from it.
            let _ = self.trocar_modelo_inicial(&g);
        }
        self.base.modelagem = None;
    }

    /// Replaces the model of the mission (in a modelling mission, the one the student
    /// starts from). The levers and measures are suggested again from it, unmarked.
    pub fn trocar_modelo_inicial(&mut self, ron: &str) -> Result<(), String> {
        let m = Model::parse(ron).map_err(|e| format!("modelo: {e}"))?;
        self.alavancas = sugerir_alavancas(&m)
            .into_iter()
            .map(|alavanca| ItemAlavanca {
                usar: false,
                alavanca,
            })
            .collect();
        self.metricas = sugerir_metricas(&m)
            .into_iter()
            .map(|metrica| ItemMetrica {
                usar: false,
                metrica,
            })
            .collect();
        self.base.referencia.clear();
        self.modelo_ron = ron.to_string();
        self.modelo = m;
        Ok(())
    }

    /// Replaces the reference model.
    pub fn trocar_gabarito(&mut self, ron: &str) -> Result<(), String> {
        Model::parse(ron).map_err(|e| format!("gabarito: {e}"))?;
        self.gabarito_ron = Some(ron.to_string());
        Ok(())
    }

    /// The criteria suggested again from the reference model, replacing the current ones.
    pub fn sugerir_criterios(&mut self) {
        if let (Some(g), Some(md)) = (self.gabarito(), &mut self.base.modelagem) {
            md.criterios = sugerir_criterios(&g);
        }
    }

    /// The problems of the modelling fields (empty for other missions). The structure of
    /// the reference model is checked here; its measures need the runs of
    /// [`Self::conferir_modelagem`].
    pub fn erros_da_modelagem(&self) -> Vec<String> {
        let Some(md) = &self.base.modelagem else {
            return Vec::new();
        };
        let mut e = Vec::new();
        let g = match self.gabarito_ron.as_deref().map(Model::parse) {
            None => {
                e.push("a missão de modelagem precisa de um gabarito".into());
                None
            }
            Some(Err(x)) => {
                e.push(format!("gabarito: {x}"));
                None
            }
            Some(Ok(g)) => Some(g),
        };
        if md.replicacoes.is_some_and(|r| r < 2) {
            e.push("são precisas ao menos 2 replicações na conferência".into());
        }
        if md.criterios.is_empty() {
            e.push("escolha ao menos um critério de correção".into());
        }
        for (i, c) in md.criterios.iter().enumerate() {
            let nome = if c.rotulo().trim().is_empty() {
                e.push(format!("critério {} sem rótulo", i + 1));
                format!("critério {}", i + 1)
            } else {
                c.rotulo().to_string()
            };
            if c.peso().is_nan() || c.peso() < 0.0 {
                e.push(format!("{nome}: peso negativo"));
            }
            match c {
                Criterio::Medida {
                    valor, tolerancia, ..
                } => {
                    if valor.is_empty() || valor.iter().any(|t| t.chave.trim().is_empty()) {
                        e.push(format!("{nome}: a medida precisa de uma chave"));
                    }
                    let t = match tolerancia {
                        Tolerancia::Relativa(x) | Tolerancia::Absoluta(x) => *x,
                    };
                    if t.is_nan() || t < 0.0 {
                        e.push(format!("{nome}: tolerância negativa"));
                    }
                }
                Criterio::Objetos {
                    tipo: t, minimo, ..
                } => {
                    if !TIPOS_DE_OBJETO.contains(&t.as_str()) {
                        e.push(format!("{nome}: o tipo '{t}' não existe"));
                    }
                    if *minimo == 0 {
                        e.push(format!("{nome}: um mínimo de zero não confere nada"));
                    }
                }
                Criterio::Capacidade { minimo, .. } if *minimo < 2 => {
                    e.push(format!("{nome}: a capacidade mínima precisa ser 2 ou mais"));
                }
                _ => {}
            }
            if let Some(g) = &g
                && let Some((false, detalhe)) = estrutura(c, g)
            {
                e.push(format!("o gabarito não cumpre \"{nome}\": {detalhe}"));
            }
        }
        e
    }

    /// Warnings that do not stop writing: the student gets the answer key as the
    /// starting model, or the starting model already meets a structural criterion.
    pub fn avisos_da_modelagem(&self) -> Vec<String> {
        let Some(md) = &self.base.modelagem else {
            return Vec::new();
        };
        let mut v = Vec::new();
        if self.gabarito_ron.as_deref() == Some(self.modelo_ron.as_str()) {
            v.push(
                "o aluno recebe o próprio gabarito como modelo inicial: carregue um modelo inicial mais simples"
                    .to_string(),
            );
            return v;
        }
        for c in &md.criterios {
            if let Some((true, _)) = estrutura(c, &self.modelo) {
                v.push(format!("o modelo inicial já cumpre \"{}\"", c.rotulo()));
            }
        }
        v
    }

    /// Runs the check of a modelling mission: the reference model and the starting
    /// model graded by the criteria, each with its replications. It takes as long as
    /// two runs of the mission.
    pub fn conferir_modelagem(&self) -> Result<ConferenciaModelagem, Vec<String>> {
        if !self.e_modelagem() {
            return Err(vec!["a missão não é de modelagem".into()]);
        }
        let p = self.carregar()?;
        let m = &p.missoes[0];
        let pesos = crate::pacote::Pesos::default();
        let corrigir = |ron: &str| -> Result<Vec<ResultadoCriterio>, Vec<String>> {
            let t = crate::nota::Tentativa {
                modelo: Some(ron.to_string()),
                ..Default::default()
            };
            crate::modelagem::corrigir_modelagem(m, &pesos, &t)
                .map(|r| r.criterios)
                .map_err(|e| vec![e.to_string()])
        };
        let gabarito = corrigir(self.gabarito_ron.as_deref().unwrap_or_default())?;
        let inicial = corrigir(&self.modelo_ron)?;
        Ok(ConferenciaModelagem { gabarito, inicial })
    }
}
