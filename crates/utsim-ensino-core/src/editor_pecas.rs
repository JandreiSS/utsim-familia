//! Stage E of the mission editor (`docs/plano_ensino_e_jogo.md`, 6.2): missions of pieces.
//! The student assigns tasks to the stations of a line ([`Balanceamento`]) or places the
//! machines of a shop on a grid ([`Arranjo`]) instead of turning levers. The editor
//! suggests the problem from the model (its stations, its machines and its routes), the
//! teacher fills the tasks, the times, the precedences, the flows and the volumes, and the
//! calibration finds the optimum by the exact search of the piece.

use std::collections::{BTreeMap, BTreeSet};

use utsim_model::{Model, ObjectKind};

use crate::editor::{Edicao, ItemMetrica};
use crate::missao::{Decisoes, Metrica, Objetivo, Termo, Tolerancia};
use crate::pecas::{Arranjo, Balanceamento, Fluxo, MaquinaArranjo, Tarefa};

/// What the student decides in a mission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoDecisao {
    /// Levers (quantities, values, choices): stages A to C.
    Alavancas,
    /// Tasks assigned to stations.
    Balanceamento,
    /// Machines placed on a grid.
    Arranjo,
}

impl TipoDecisao {
    pub const TODOS: [TipoDecisao; 3] = [
        TipoDecisao::Alavancas,
        TipoDecisao::Balanceamento,
        TipoDecisao::Arranjo,
    ];
}

/// The name of the measure the editor adds to a mission of pieces.
pub const METRICA_BALANCEAMENTO: &str = "eficiencia";
pub const METRICA_ARRANJO: &str = "momento";

/// The prefix of the station objects the editor looks for in the model.
pub const PREFIXO_ESTACAO: &str = "est";

/// The stations `est1`, `est2`... of the model: how many come in a row from 1.
pub fn estacoes_do_modelo(modelo: &Model, prefixo: &str) -> u32 {
    let nomes: BTreeSet<&str> = modelo
        .objects
        .iter()
        .filter(|o| matches!(o.kind, ObjectKind::Processor { .. }))
        .map(|o| o.name.as_str())
        .collect();
    (1..)
        .take_while(|i| nomes.contains(format!("{prefixo}{i}").as_str()))
        .count() as u32
}

/// The machine an object stands for: its name up to `__` (`serra__eixos_1` is `serra`).
pub fn base_do_objeto(nome: &str) -> &str {
    nome.split("__").next().unwrap_or(nome)
}

/// The machines of the model, in the order they first appear.
pub fn maquinas_do_modelo(modelo: &Model) -> Vec<String> {
    let mut vistas = BTreeSet::new();
    let mut out = Vec::new();
    for o in &modelo.objects {
        let b = base_do_objeto(&o.name);
        if vistas.insert(b.to_string()) {
            out.push(b.to_string());
        }
    }
    out
}

/// A task id after `n` others: `A` to `Z`, then `T27`, `T28`...
fn id_tarefa(n: usize) -> String {
    if n < 26 {
        ((b'A' + n as u8) as char).to_string()
    } else {
        format!("T{}", n + 1)
    }
}

fn media_chegada(modelo: &Model) -> Option<f64> {
    modelo.objects.iter().find_map(|o| match &o.kind {
        ObjectKind::Source { interarrival, .. } => Some(interarrival.mean()),
        _ => None,
    })
}

fn arredonda(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// A balancing problem drawn from the model: the stations `est1`... in a row, the takt from
/// the mean time between arrivals, one task per station with its current time (two
/// examples when the stations have no time yet).
pub fn balanceamento_sugerido(modelo: &Model) -> Balanceamento {
    let estacoes = estacoes_do_modelo(modelo, PREFIXO_ESTACAO);
    let takt = media_chegada(modelo)
        .filter(|x| x.is_finite() && *x > 0.0)
        .map(arredonda)
        .unwrap_or(60.0);
    let tempos: Vec<f64> = (1..=estacoes)
        .filter_map(|i| {
            let nome = format!("{PREFIXO_ESTACAO}{i}");
            modelo.objects.iter().find_map(|o| match &o.kind {
                ObjectKind::Processor { process, .. } if o.name == nome => Some(process.mean()),
                _ => None,
            })
        })
        .filter(|t| t.is_finite() && *t > 0.0)
        .map(arredonda)
        .collect();
    let mut tarefas: Vec<Tarefa> = tempos
        .iter()
        .enumerate()
        .map(|(i, t)| Tarefa {
            id: id_tarefa(i),
            nome: format!("Tarefa {}", id_tarefa(i)),
            tempo: *t,
            depois_de: if i == 0 {
                Vec::new()
            } else {
                vec![id_tarefa(i - 1)]
            },
            tempos: BTreeMap::new(),
        })
        .collect();
    if tarefas.is_empty() {
        tarefas = vec![
            Tarefa {
                id: "A".into(),
                nome: "Tarefa A".into(),
                tempo: arredonda(takt / 2.0),
                depois_de: Vec::new(),
                tempos: BTreeMap::new(),
            },
            Tarefa {
                id: "B".into(),
                nome: "Tarefa B".into(),
                tempo: arredonda(takt / 2.0),
                depois_de: vec!["A".into()],
                tempos: BTreeMap::new(),
            },
        ];
    }
    let ultima = estacoes.max(1);
    let inicial = tarefas
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.clone(), (i as u32 + 1).min(ultima)))
        .collect();
    Balanceamento {
        tarefas,
        takt,
        estacoes,
        cv: 0.0,
        permite_passar_takt: false,
        editavel: true,
        inicial,
        prefixo: PREFIXO_ESTACAO.into(),
        mix: Vec::new(),
        rotulo: "modelo".into(),
    }
}

/// The route of every source of the model: the machines its items go through, following
/// the first connection out of each object, until a sink or a loop.
fn roteiros(modelo: &Model) -> Vec<(String, Vec<String>, Option<f64>)> {
    let mut saidas: BTreeMap<&str, &str> = BTreeMap::new();
    for (a, b) in &modelo.connections {
        saidas.entry(a.as_str()).or_insert(b.as_str());
    }
    let mut out = Vec::new();
    for o in &modelo.objects {
        let ObjectKind::Source { interarrival, .. } = &o.kind else {
            continue;
        };
        let mut roteiro: Vec<String> = Vec::new();
        let mut vistos = BTreeSet::new();
        let mut atual = o.name.as_str();
        loop {
            if !vistos.insert(atual) {
                break;
            }
            let b = base_do_objeto(atual);
            if roteiro.last().map(String::as_str) != Some(b) {
                roteiro.push(b.to_string());
            }
            match saidas.get(atual) {
                Some(p) => atual = p,
                None => break,
            }
        }
        let familia = o
            .name
            .split_once("__")
            .map(|(_, f)| f.trim_end_matches("_chegada").to_string())
            .unwrap_or_else(|| o.name.clone());
        let m = interarrival.mean();
        out.push((familia, roteiro, (m.is_finite() && m > 0.0).then_some(m)));
    }
    out
}

/// A layout problem drawn from the model: one machine per name before `__`, the machines
/// of sources and sinks fixed (the dock), a grid with a spare row and column, the machines
/// placed in order, and one flow per source with a volume proportional to its arrivals
/// (arrivals per 100 time units).
pub fn arranjo_sugerido(modelo: &Model) -> Arranjo {
    let nomes = maquinas_do_modelo(modelo);
    let fixas: BTreeSet<&str> = modelo
        .objects
        .iter()
        .filter(|o| matches!(o.kind, ObjectKind::Source { .. } | ObjectKind::Sink))
        .map(|o| base_do_objeto(&o.name))
        .collect();
    let n = nomes.len().max(1) as u32;
    let colunas = (n as f64).sqrt().ceil() as u32 + 1;
    let linhas = n.div_ceil(colunas) + 1;
    let maquinas: Vec<MaquinaArranjo> = nomes
        .iter()
        .map(|m| MaquinaArranjo {
            id: m.clone(),
            nome: m.clone(),
            fixa: fixas.contains(m.as_str()),
        })
        .collect();
    let inicial = nomes
        .iter()
        .enumerate()
        .map(|(i, m)| (m.clone(), i as u32))
        .collect();
    let fluxos = roteiros(modelo)
        .into_iter()
        .filter(|(_, r, _)| r.len() >= 2)
        .enumerate()
        .map(|(i, (familia, roteiro, media))| Fluxo {
            nome: if familia.is_empty() {
                format!("Fluxo {}", i + 1)
            } else {
                familia
            },
            volume: media.map_or(1.0, |m| arredonda(100.0 / m)),
            roteiro,
        })
        .collect();
    Arranjo {
        colunas,
        linhas,
        celula: 10.0,
        maquinas,
        fluxos,
        inicial,
    }
}

/// The measure the editor adds for a balancing problem.
pub fn metrica_balanceamento() -> Metrica {
    Metrica {
        nome: METRICA_BALANCEAMENTO.into(),
        rotulo: "Eficiência do balanceamento".into(),
        unidade: String::new(),
        valor: vec![Termo {
            chave: "balanceamento.eficiencia".into(),
            fator: 1.0,
        }],
        divisor: Vec::new(),
        objetivo: Objetivo::Maior,
        meta: None,
        peso: 1.0,
        tolerancia: Tolerancia::Absoluta(0.0),
        casas: 3,
    }
}

/// The measure the editor adds for a layout problem.
pub fn metrica_arranjo() -> Metrica {
    Metrica {
        nome: METRICA_ARRANJO.into(),
        rotulo: "Momento de transporte (viagens × distância)".into(),
        unidade: "m".into(),
        valor: vec![Termo {
            chave: "arranjo.momento".into(),
            fator: 1.0,
        }],
        divisor: Vec::new(),
        objetivo: Objetivo::Menor,
        meta: None,
        peso: 1.0,
        tolerancia: Tolerancia::Relativa(0.0),
        casas: 0,
    }
}

impl Edicao {
    /// What the student decides in this mission.
    pub fn tipo_decisao(&self) -> TipoDecisao {
        if self.base.arranjo.is_some() {
            TipoDecisao::Arranjo
        } else if self.base.balanceamento.as_ref().is_some_and(|b| b.editavel) {
            TipoDecisao::Balanceamento
        } else {
            TipoDecisao::Alavancas
        }
    }

    /// Whether the student assigns tasks or places machines.
    pub fn e_pecas(&self) -> bool {
        self.tipo_decisao() != TipoDecisao::Alavancas
    }

    /// Changes what the student decides. Pieces drop the levers out of the mission and
    /// bring their measure (marked, with weight); going back to levers drops the pieces and
    /// their measures. The reference is cleared either way (calibrate again).
    pub fn trocar_tipo_decisao(&mut self, t: TipoDecisao) {
        if t == self.tipo_decisao() {
            return;
        }
        self.base.referencia = Decisoes::new();
        self.base.arranjo = None;
        if self.base.balanceamento.as_ref().is_some_and(|b| b.editavel) {
            self.base.balanceamento = None;
        }
        self.metricas.retain(|m| {
            m.metrica.nome != METRICA_BALANCEAMENTO && m.metrica.nome != METRICA_ARRANJO
        });
        let metrica = match t {
            TipoDecisao::Alavancas => return,
            TipoDecisao::Balanceamento => {
                self.base.balanceamento = Some(balanceamento_sugerido(&self.modelo));
                metrica_balanceamento()
            }
            TipoDecisao::Arranjo => {
                self.base.arranjo = Some(arranjo_sugerido(&self.modelo));
                metrica_arranjo()
            }
        };
        for a in &mut self.alavancas {
            a.usar = false;
        }
        for m in &mut self.metricas {
            m.usar = false;
        }
        self.metricas.insert(
            0,
            ItemMetrica {
                usar: true,
                metrica,
            },
        );
    }

    /// Adds a task after the last one, at the last station in use.
    pub fn nova_tarefa(&mut self) {
        let Some(b) = &mut self.base.balanceamento else {
            return;
        };
        let id = (0..)
            .map(id_tarefa)
            .find(|id| !b.tarefas.iter().any(|t| &t.id == id))
            .expect("some id is free");
        let estacao = b.inicial.values().copied().max().unwrap_or(1).max(1);
        b.inicial.insert(id.clone(), estacao);
        b.tarefas.push(Tarefa {
            nome: format!("Tarefa {id}"),
            id,
            tempo: 10.0,
            depois_de: Vec::new(),
            tempos: BTreeMap::new(),
        });
    }

    /// Removes a task, and it from the precedences of the others.
    pub fn remover_tarefa(&mut self, id: &str) {
        let Some(b) = &mut self.base.balanceamento else {
            return;
        };
        b.tarefas.retain(|t| t.id != id);
        for t in &mut b.tarefas {
            t.depois_de.retain(|p| p != id);
        }
        b.inicial.remove(id);
    }

    /// The machines of the model not in the layout yet.
    pub fn maquinas_livres(&self) -> Vec<String> {
        let Some(a) = &self.base.arranjo else {
            return Vec::new();
        };
        maquinas_do_modelo(&self.modelo)
            .into_iter()
            .filter(|m| !a.maquinas.iter().any(|x| &x.id == m))
            .collect()
    }

    /// Adds a machine of the model on the first free cell (the grid grows a row when full).
    pub fn nova_maquina(&mut self, id: &str) {
        let Some(a) = &mut self.base.arranjo else {
            return;
        };
        if a.maquinas.iter().any(|m| m.id == id) {
            return;
        }
        let ocupadas: BTreeSet<u32> = a.inicial.values().copied().collect();
        if ocupadas.len() as u32 >= a.celulas() {
            a.linhas += 1;
        }
        let celula = (0..a.celulas())
            .find(|c| !ocupadas.contains(c))
            .expect("a free cell");
        a.maquinas.push(MaquinaArranjo {
            id: id.to_string(),
            nome: id.to_string(),
            fixa: false,
        });
        a.inicial.insert(id.to_string(), celula);
    }

    /// Removes a machine, and it from the routes.
    pub fn remover_maquina(&mut self, id: &str) {
        let Some(a) = &mut self.base.arranjo else {
            return;
        };
        a.maquinas.retain(|m| m.id != id);
        a.inicial.remove(id);
        for f in &mut a.fluxos {
            f.roteiro.retain(|m| m != id);
            f.roteiro.dedup();
        }
    }

    /// Adds a flow through the first two machines.
    pub fn novo_fluxo(&mut self) {
        let Some(a) = &mut self.base.arranjo else {
            return;
        };
        let roteiro = a.maquinas.iter().take(2).map(|m| m.id.clone()).collect();
        a.fluxos.push(Fluxo {
            nome: format!("Fluxo {}", a.fluxos.len() + 1),
            volume: 10.0,
            roteiro,
        });
    }

    /// The problems of the pieces (empty for a mission of levers).
    pub fn erros_das_pecas(&self) -> Vec<String> {
        let mut e = Vec::new();
        if let Some(b) = self.base.balanceamento.as_ref().filter(|b| b.editavel) {
            if b.tarefas.is_empty() {
                e.push("balanceamento: acrescente as tarefas".into());
            }
            for t in &b.tarefas {
                if t.nome.trim().is_empty() {
                    e.push(format!("balanceamento: tarefa '{}' sem nome", t.id));
                }
                if t.depois_de.contains(&t.id) {
                    e.push(format!("balanceamento: '{}' depende de si mesma", t.id));
                }
            }
            let no_modelo = estacoes_do_modelo(&self.modelo, &b.prefixo);
            if no_modelo == 0 {
                e.push(format!(
                    "balanceamento: o modelo precisa de estações {p}1, {p}2... (processadores com esses nomes)",
                    p = b.prefixo
                ));
            } else if b.estacoes > no_modelo {
                e.push(format!(
                    "balanceamento: o modelo só tem {no_modelo} estações ({}1 a {}{no_modelo})",
                    b.prefixo, b.prefixo
                ));
            }
            if b.cv.is_nan() || b.cv < 0.0 {
                e.push("balanceamento: variação negativa".into());
            }
            if let Some(t) = b.tarefas.iter().find(|t| t.tempo > b.takt + 1e-9)
                && !b.permite_passar_takt
            {
                e.push(format!(
                    "balanceamento: \"{}\" sozinha passa do takt, então nenhuma atribuição cabe",
                    t.nome
                ));
            }
            e.extend(b.validar());
        }
        if let Some(a) = &self.base.arranjo {
            if a.maquinas.len() < 2 {
                e.push("arranjo: são precisas duas máquinas".into());
            }
            if a.fluxos.is_empty() {
                e.push("arranjo: acrescente ao menos um fluxo".into());
            }
            if a.celula.is_nan() || a.celula <= 0.0 {
                e.push("arranjo: a célula precisa de um tamanho".into());
            }
            for f in &a.fluxos {
                if f.nome.trim().is_empty() {
                    e.push("arranjo: fluxo sem nome".into());
                }
                if f.roteiro.len() < 2 {
                    e.push(format!(
                        "arranjo: o fluxo \"{}\" precisa passar por duas máquinas",
                        f.nome
                    ));
                }
                if f.volume.is_nan() || f.volume <= 0.0 {
                    e.push(format!("arranjo: o fluxo \"{}\" sem volume", f.nome));
                }
            }
            let objetos = maquinas_do_modelo(&self.modelo);
            for m in &a.maquinas {
                if !objetos.contains(&m.id) {
                    e.push(format!(
                        "arranjo: o modelo não tem objetos de \"{}\" para mover",
                        m.id
                    ));
                }
            }
            if a.maquinas.iter().all(|m| m.fixa) && !a.maquinas.is_empty() {
                e.push("arranjo: todas as máquinas estão fixas".into());
            }
            e.extend(a.validar());
        }
        e
    }

    /// Warnings of the pieces: the initial state that already is the optimum, a search too
    /// big for the exact optimum.
    pub fn avisos_das_pecas(&self) -> Vec<String> {
        let mut v = Vec::new();
        if !self.erros_das_pecas().is_empty() {
            return v;
        }
        if let Some(b) = self.base.balanceamento.as_ref().filter(|b| b.editavel) {
            match b.otimo() {
                None => v.push(
                    "balanceamento: a busca exata não achou atribuição dentro do takt".into(),
                ),
                Some(o) => {
                    let (ri, ro) = (b.resumo(&b.inicial), b.resumo(&o));
                    if b.violacao(&b.inicial).is_none()
                        && ri.estacoes_usadas == ro.estacoes_usadas
                        && (ri.ciclo - ro.ciclo).abs() < 1e-9
                    {
                        v.push("balanceamento: o estado inicial já é o ótimo".into());
                    }
                }
            }
        }
        if let Some(a) = &self.base.arranjo {
            match a.otimo() {
                None => v.push(
                    "arranjo: a grade é grande demais para a busca exata: use menos células ou fixe máquinas"
                        .into(),
                ),
                Some(o) => {
                    if (a.momento(&a.inicial) - a.momento(&o)).abs() < 1e-9 {
                        v.push("arranjo: o estado inicial já é o ótimo".into());
                    }
                }
            }
        }
        v
    }
}
