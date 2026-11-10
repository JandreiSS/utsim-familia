//! The Qualidade, Segurança and Manutenção tabs of a node (owner's direction of 2026-11-08,
//! `docs/plano_integracao_familia.md`, section 3), shared by UTSim, UTSim Qualidade and UTSim
//! Segurança do Trabalho: the same fields ([`campos`]) and the same editing ([`editar`],
//! [`adicionar`], [`remover`]), so an edit made in one app means the same in the others.
//!
//! Every edit returns one `Set*` command that replaces the whole block (one undo step). The
//! path of a field is the field of the object in the model file, with list positions in
//! brackets: `inspection.detect`, `safety.hazards[0].severity`, `downtime.mtbf`. A path
//! that names a block alone (`quality`, `safety`...) is the switch that adds or removes it.

use serde_json::Value;
use utsim_core::Dist;
use utsim_model::quality::PPE_NAMES;
use utsim_model::{Model, ObjectDef, ObjectKind};

use crate::Command;

/// A tab of the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aba {
    Qualidade,
    Seguranca,
    Manutencao,
}

impl Aba {
    pub const TODAS: [Aba; 3] = [Aba::Qualidade, Aba::Seguranca, Aba::Manutencao];

    pub fn nome(self) -> &'static str {
        match self {
            Aba::Qualidade => "Qualidade",
            Aba::Seguranca => "Segurança",
            Aba::Manutencao => "Manutenção",
        }
    }
}

/// How a field is edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tipo {
    /// Switch that adds or removes a block (`sim` / `não`).
    Bloco,
    Booleano,
    Inteiro,
    Real,
    /// Optional number: empty means absent.
    RealOpcional,
    InteiroOpcional,
    Texto,
    /// One of `escolhas`.
    Escolha,
    /// Several of `escolhas` (or others), separated by commas.
    Conjunto,
    /// A distribution in the model's notation (`Exp(mean: 600.0)`).
    Distribuicao,
    /// A list: its items follow, one level deeper; [`adicionar`] and [`remover`] act on it.
    Lista,
    /// Heading of a list item.
    Item,
    /// Read only.
    Info,
}

/// One field of a tab.
#[derive(Clone, Debug, PartialEq)]
pub struct Campo {
    pub caminho: String,
    pub rotulo: String,
    pub tipo: Tipo,
    /// Current value as text (`sim`/`não` for switches, empty for absent).
    pub valor: String,
    pub escolhas: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub unidade: String,
    pub ajuda: String,
    /// Nesting level (0 the block, 1 its fields, 2 the fields of a list item...).
    pub profundidade: u8,
}

fn campo(caminho: impl Into<String>, rotulo: &str, tipo: Tipo, valor: String, prof: u8) -> Campo {
    Campo {
        caminho: caminho.into(),
        rotulo: rotulo.into(),
        tipo,
        valor,
        escolhas: Vec::new(),
        min: None,
        max: None,
        unidade: String::new(),
        ajuda: String::new(),
        profundidade: prof,
    }
}

impl Campo {
    fn faixa(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }
    fn escolhas(mut self, e: &[&str]) -> Self {
        self.escolhas = e.iter().map(|s| s.to_string()).collect();
        self
    }
    fn ajuda(mut self, a: &str) -> Self {
        self.ajuda = a.into();
        self
    }
    fn unidade(mut self, u: &str) -> Self {
        self.unidade = u.into();
        self
    }
}

fn num(v: &Value) -> String {
    match v {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", f as i64)
            } else {
                format!("{f}")
            }
        }
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "sim" } else { "não" }.into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn dist_text(v: &Value) -> String {
    serde_json::from_value::<Dist>(v.clone())
        .ok()
        .and_then(|d| ron::to_string(&d).ok())
        .unwrap_or_default()
}

fn sim(b: bool) -> String {
    if b { "sim" } else { "não" }.into()
}

fn bloco(obj: &ObjectDef, nome: &str) -> Value {
    let v = match nome {
        "quality" => serde_json::to_value(&obj.quality),
        "inspection" => serde_json::to_value(&obj.inspection),
        "rework" => serde_json::to_value(&obj.rework),
        "safety" => serde_json::to_value(&obj.safety),
        "downtime" => serde_json::to_value(&obj.downtime),
        "sink_role" => serde_json::to_value(obj.sink_role),
        "ppe" => serde_json::to_value(&obj.ppe),
        "operation" => serde_json::to_value(&obj.operation),
        _ => Ok(Value::Null),
    };
    v.unwrap_or(Value::Null)
}

fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    v.get(k).unwrap_or(&Value::Null)
}

fn lista<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn processa(obj: &ObjectDef) -> bool {
    matches!(
        obj.kind,
        ObjectKind::Processor { .. } | ObjectKind::Combiner { .. } | ObjectKind::Separator { .. }
    )
}

/// The fields of tab `aba` of object `objeto`.
pub fn campos(model: &Model, objeto: &str, aba: Aba) -> Result<Vec<Campo>, String> {
    let obj = model
        .objects
        .iter()
        .find(|o| o.name == objeto)
        .ok_or_else(|| format!("o objeto '{objeto}' não existe"))?;
    let mut out = Vec::new();
    match aba {
        Aba::Qualidade => qualidade(obj, &mut out),
        Aba::Seguranca => seguranca(model, obj, &mut out),
        Aba::Manutencao => manutencao(obj, &mut out),
    }
    Ok(out)
}

fn qualidade(obj: &ObjectDef, out: &mut Vec<Campo>) {
    if matches!(obj.kind, ObjectKind::Sink) {
        let papel = num(&bloco(obj, "sink_role"));
        out.push(
            campo(
                "sink_role",
                "Papel nos relatórios de qualidade",
                Tipo::Escolha,
                papel,
                0,
            )
            .escolhas(&["", "Customer", "Scrap"])
            .ajuda(
                "Customer: o que chega aqui é entregue ao cliente (conta o escape). Scrap: refugo.",
            ),
        );
        return;
    }
    if !processa(obj) {
        out.push(campo(
            "",
            "Este tipo de objeto não tem dados de qualidade.",
            Tipo::Info,
            String::new(),
            0,
        ));
        return;
    }
    let q = bloco(obj, "quality");
    out.push(
        campo(
            "quality",
            "Gera defeitos ou mede características",
            Tipo::Bloco,
            sim(!q.is_null()),
            0,
        )
        .ajuda("Defeitos sorteados por peça e características medidas, com o plano de controle."),
    );
    if !q.is_null() {
        out.push(campo(
            "quality.defects",
            "Defeitos",
            Tipo::Lista,
            String::new(),
            1,
        ));
        for (i, d) in lista(&q, "defects").iter().enumerate() {
            let p = format!("quality.defects[{i}]");
            out.push(campo(
                &p,
                &num(get(d, "name")),
                Tipo::Item,
                String::new(),
                2,
            ));
            out.push(campo(
                format!("{p}.name"),
                "Tipo de defeito",
                Tipo::Texto,
                num(get(d, "name")),
                3,
            ));
            out.push(
                campo(
                    format!("{p}.probability"),
                    "Probabilidade por peça",
                    Tipo::Real,
                    num(get(d, "probability")),
                    3,
                )
                .faixa(0.0, 1.0),
            );
            let n = lista(d, "factors").len();
            if n > 0 {
                out.push(campo(
                    format!("{p}.factors"),
                    "Fatores (editar no arquivo)",
                    Tipo::Info,
                    n.to_string(),
                    3,
                ));
            }
        }
        out.push(campo(
            "quality.characteristics",
            "Características medidas",
            Tipo::Lista,
            String::new(),
            1,
        ));
        for (i, c) in lista(&q, "characteristics").iter().enumerate() {
            let p = format!("quality.characteristics[{i}]");
            out.push(campo(
                &p,
                &num(get(c, "name")),
                Tipo::Item,
                String::new(),
                2,
            ));
            out.push(campo(
                format!("{p}.name"),
                "Nome",
                Tipo::Texto,
                num(get(c, "name")),
                3,
            ));
            out.push(campo(
                format!("{p}.value"),
                "Valor",
                Tipo::Distribuicao,
                dist_text(get(c, "value")),
                3,
            ));
            out.push(campo(
                format!("{p}.drift"),
                "Deriva por peça",
                Tipo::Real,
                num(get(c, "drift")),
                3,
            ));
            out.push(campo(
                format!("{p}.lsl"),
                "Limite inferior (LIE)",
                Tipo::RealOpcional,
                num(get(c, "lsl")),
                3,
            ));
            out.push(campo(
                format!("{p}.usl"),
                "Limite superior (LSE)",
                Tipo::RealOpcional,
                num(get(c, "usl")),
                3,
            ));
        }
        out.push(campo(
            "quality.control",
            "Plano de controle",
            Tipo::Lista,
            String::new(),
            1,
        ));
        for (i, c) in lista(&q, "control").iter().enumerate() {
            let p = format!("quality.control[{i}]");
            out.push(campo(
                &p,
                &num(get(c, "characteristic")),
                Tipo::Item,
                String::new(),
                2,
            ));
            let caracs: Vec<String> = lista(&q, "characteristics")
                .iter()
                .map(|c| num(get(c, "name")))
                .collect();
            let mut ch = campo(
                format!("{p}.characteristic"),
                "Característica",
                Tipo::Escolha,
                num(get(c, "characteristic")),
                3,
            );
            ch.escolhas = caracs;
            out.push(ch);
            out.push(campo(
                format!("{p}.method"),
                "Método e instrumento",
                Tipo::Texto,
                num(get(c, "method")),
                3,
            ));
            out.push(
                campo(
                    format!("{p}.msa_ok"),
                    "Sistema de medição aprovado",
                    Tipo::Escolha,
                    num(get(c, "msa_ok")),
                    3,
                )
                .escolhas(&["", "sim", "não"]),
            );
            out.push(campo(
                format!("{p}.frequency"),
                "Frequência",
                Tipo::Texto,
                num(get(c, "frequency")),
                3,
            ));
            out.push(
                campo(
                    format!("{p}.sample_size"),
                    "Tamanho da amostra",
                    Tipo::InteiroOpcional,
                    num(get(c, "sample_size")),
                    3,
                )
                .faixa(1.0, 1000.0),
            );
            out.push(
                campo(
                    format!("{p}.chart"),
                    "Carta de controle",
                    Tipo::Escolha,
                    num(get(c, "chart")),
                    3,
                )
                .escolhas(&["", "imr", "xbarr", "xbars", "p", "np", "c", "u"]),
            );
            out.push(campo(
                format!("{p}.reaction"),
                "Reação a um sinal",
                Tipo::Texto,
                num(get(c, "reaction")),
                3,
            ));
            out.push(campo(
                format!("{p}.fmea"),
                "Linha do FMEA",
                Tipo::Texto,
                num(get(c, "fmea")),
                3,
            ));
        }
        out.push(
            campo(
                "quality.unit_cost",
                "Valor agregado por peça",
                Tipo::RealOpcional,
                num(get(&q, "unit_cost")),
                1,
            )
            .faixa(0.0, f64::MAX),
        );
        out.push(
            campo(
                "quality.escape_cost",
                "Custo de cada defeito no cliente",
                Tipo::RealOpcional,
                num(get(&q, "escape_cost")),
                1,
            )
            .faixa(0.0, f64::MAX),
        );
    }
    let i = bloco(obj, "inspection");
    out.push(
        campo(
            "inspection",
            "É uma estação de inspeção",
            Tipo::Bloco,
            sim(!i.is_null()),
            0,
        )
        .ajuda("Saída por rótulo, nas portas aprovado, retrabalho e refugo."),
    );
    if !i.is_null() {
        out.push(
            campo(
                "inspection.every",
                "Inspeciona uma peça a cada",
                Tipo::Inteiro,
                num(get(&i, "every")),
                1,
            )
            .faixa(1.0, 1e9),
        );
        out.push(
            campo(
                "inspection.detect",
                "Probabilidade de detectar",
                Tipo::Real,
                num(get(&i, "detect")),
                1,
            )
            .faixa(0.0, 1.0),
        );
        out.push(
            campo(
                "inspection.false_reject",
                "Alarme falso",
                Tipo::Real,
                num(get(&i, "false_reject")),
                1,
            )
            .faixa(0.0, 1.0),
        );
        out.push(
            campo(
                "inspection.on_reject",
                "Peça rejeitada vai para",
                Tipo::Escolha,
                num(get(&i, "on_reject")),
                1,
            )
            .escolhas(&["Rework", "Scrap"]),
        );
        out.push(
            campo(
                "inspection.max_rework",
                "Passagens de retrabalho permitidas",
                Tipo::InteiroOpcional,
                num(get(&i, "max_rework")),
                1,
            )
            .faixa(0.0, 100.0),
        );
        out.push(
            campo(
                "inspection.cost",
                "Custo por peça inspecionada",
                Tipo::RealOpcional,
                num(get(&i, "cost")),
                1,
            )
            .faixa(0.0, f64::MAX),
        );
        out.push(campo(
            "inspection.measure",
            "Medições",
            Tipo::Lista,
            String::new(),
            1,
        ));
        for (k, m) in lista(&i, "measure").iter().enumerate() {
            let p = format!("inspection.measure[{k}]");
            out.push(campo(
                &p,
                &num(get(m, "characteristic")),
                Tipo::Item,
                String::new(),
                2,
            ));
            out.push(campo(
                format!("{p}.characteristic"),
                "Característica",
                Tipo::Texto,
                num(get(m, "characteristic")),
                3,
            ));
            out.push(
                campo(
                    format!("{p}.sigma"),
                    "Desvio do sistema de medição",
                    Tipo::Real,
                    num(get(m, "sigma")),
                    3,
                )
                .faixa(0.0, f64::MAX),
            );
        }
    }
    let r = bloco(obj, "rework");
    out.push(campo(
        "rework",
        "É um posto de retrabalho",
        Tipo::Bloco,
        sim(!r.is_null()),
        0,
    ));
    if !r.is_null() {
        out.push(
            campo(
                "rework.fix",
                "Probabilidade de corrigir",
                Tipo::Real,
                num(get(&r, "fix")),
                1,
            )
            .faixa(0.0, 1.0),
        );
        out.push(
            campo(
                "rework.cost",
                "Custo por passagem",
                Tipo::RealOpcional,
                num(get(&r, "cost")),
                1,
            )
            .faixa(0.0, f64::MAX),
        );
    }
}

fn seguranca(model: &Model, obj: &ObjectDef, out: &mut Vec<Campo>) {
    let mut epi = campo(
        "ppe",
        "EPI exigido no posto",
        Tipo::Conjunto,
        obj.ppe.join(", "),
        0,
    );
    epi.escolhas = PPE_NAMES.iter().map(|s| s.to_string()).collect();
    out.push(epi);
    let pos = [obj.position[0], obj.position[1]];
    let zonas: Vec<&str> = model
        .zones
        .iter()
        .filter(|z| utsim_model::exposure::contains(&z.polygon, pos))
        .map(|z| z.name.as_str())
        .collect();
    out.push(
        campo(
            "",
            "Zonas de risco que contêm o posto",
            Tipo::Info,
            zonas.join(", "),
            0,
        )
        .ajuda("As zonas são desenhadas no piso, com a ferramenta Zona do editor."),
    );
    operacao(model, obj, out);
    let s = bloco(obj, "safety");
    out.push(campo(
        "safety",
        "Perigos e ergonomia do posto",
        Tipo::Bloco,
        sim(!s.is_null()),
        0,
    ));
    if s.is_null() {
        return;
    }
    out.push(campo(
        "safety.hazards",
        "Perigos",
        Tipo::Lista,
        String::new(),
        1,
    ));
    for (i, h) in lista(&s, "hazards").iter().enumerate() {
        let p = format!("safety.hazards[{i}]");
        out.push(campo(
            &p,
            &num(get(h, "description")),
            Tipo::Item,
            String::new(),
            2,
        ));
        out.push(
            campo(
                format!("{p}.group"),
                "Grupo",
                Tipo::Escolha,
                num(get(h, "group")),
                3,
            )
            .escolhas(&[
                "Physical",
                "Chemical",
                "Biological",
                "Ergonomic",
                "Accident",
            ]),
        );
        out.push(campo(
            format!("{p}.description"),
            "Perigo",
            Tipo::Texto,
            num(get(h, "description")),
            3,
        ));
        out.push(campo(
            format!("{p}.source"),
            "Fonte",
            Tipo::Texto,
            num(get(h, "source")),
            3,
        ));
        out.push(campo(
            format!("{p}.circumstance"),
            "Circunstância",
            Tipo::Texto,
            num(get(h, "circumstance")),
            3,
        ));
        out.push(campo(
            format!("{p}.injuries"),
            "Lesões possíveis",
            Tipo::Texto,
            num(get(h, "injuries")),
            3,
        ));
        out.push(campo(
            format!("{p}.controls"),
            "Controles existentes",
            Tipo::Texto,
            num(get(h, "controls")),
            3,
        ));
        let max = f64::from(utsim_model::safety::MATRIX_STEPS - 1);
        out.push(
            campo(
                format!("{p}.probability"),
                "Probabilidade (degrau)",
                Tipo::Inteiro,
                num(get(h, "probability")),
                3,
            )
            .faixa(0.0, max),
        );
        out.push(
            campo(
                format!("{p}.severity"),
                "Severidade (degrau)",
                Tipo::Inteiro,
                num(get(h, "severity")),
                3,
            )
            .faixa(0.0, max),
        );
    }
    out.push(campo(
        "safety.ergonomics",
        "Ergonomia por elemento",
        Tipo::Lista,
        String::new(),
        1,
    ));
    for (i, m) in lista(&s, "ergonomics").iter().enumerate() {
        let p = format!("safety.ergonomics[{i}]");
        out.push(campo(
            &p,
            &num(get(m, "element")),
            Tipo::Item,
            String::new(),
            2,
        ));
        out.push(campo(
            format!("{p}.actor"),
            "Pessoa",
            Tipo::Texto,
            num(get(m, "actor")),
            3,
        ));
        out.push(campo(
            format!("{p}.element"),
            "Elemento",
            Tipo::Texto,
            num(get(m, "element")),
            3,
        ));
        for (k, r) in [
            ("trunk", "Tronco"),
            ("arms", "Braços"),
            ("neck", "Pescoço"),
            ("legs", "Pernas"),
        ] {
            out.push(
                campo(format!("{p}.{k}"), r, Tipo::Inteiro, num(get(m, k)), 3)
                    .faixa(0.0, 3.0)
                    .ajuda("0 neutra, 1 leve, 2 acentuada, 3 extrema"),
            );
        }
        out.push(
            campo(
                format!("{p}.load_kg"),
                "Carga",
                Tipo::Real,
                num(get(m, "load_kg")),
                3,
            )
            .faixa(0.0, 1000.0)
            .unidade("kg"),
        );
        out.push(
            campo(
                format!("{p}.grip"),
                "Pega",
                Tipo::Inteiro,
                num(get(m, "grip")),
                3,
            )
            .faixa(0.0, 2.0)
            .ajuda("0 boa, 1 regular, 2 ruim"),
        );
        out.push(
            campo(
                format!("{p}.vibration"),
                "Vibração",
                Tipo::Inteiro,
                num(get(m, "vibration")),
                3,
            )
            .faixa(0.0, 2.0)
            .ajuda("0 nenhuma, 1 alguma, 2 forte"),
        );
    }
}

/// The operation rule of a process (`operation`): who may operate it (skills and protective
/// equipment) and the time to get in and out.
fn operacao(model: &Model, obj: &ObjectDef, out: &mut Vec<Campo>) {
    if !matches!(obj.kind, ObjectKind::Processor { .. }) {
        return;
    }
    let op = bloco(obj, "operation");
    out.push(
        campo(
            "operation",
            "Regra de operação (quem pode operar)",
            Tipo::Bloco,
            sim(!op.is_null()),
            0,
        )
        .ajuda("Sem a regra, qualquer pessoa da equipe opera o posto, sem tempo de entrar e sair."),
    );
    let Some(regra) = obj.operation.as_deref() else {
        return;
    };
    let mut habilidades = campo(
        "operation.skills",
        "Habilidades exigidas",
        Tipo::Conjunto,
        regra.skills.join(", "),
        1,
    )
    .ajuda("Todo operador precisa de todas, além das exigidas por cada equipe.");
    for o in &model.objects {
        for s in o.person.iter().flat_map(|p| p.skills.iter()) {
            if !habilidades.escolhas.contains(s) {
                habilidades.escolhas.push(s.clone());
            }
        }
    }
    habilidades.escolhas.sort();
    out.push(habilidades);
    out.push(
        campo(
            "operation.require_ppe",
            "Exigir o EPI do posto e das zonas",
            Tipo::Booleano,
            sim(regra.require_ppe),
            1,
        )
        .ajuda("O EPI que a pessoa veste é o EPI do nó dela."),
    );
    if regra.require_ppe {
        out.push(campo(
            "",
            "EPI exigido de quem opera",
            Tipo::Info,
            model.required_ppe(obj).join(", "),
            1,
        ));
    }
    for (chave, rotulo) in [("enter", "Tempo para entrar"), ("leave", "Tempo para sair")] {
        out.push(
            campo(
                format!("operation.{chave}"),
                rotulo,
                Tipo::Distribuicao,
                dist_text(get(&op, chave)),
                1,
            )
            .unidade("s")
            .ajuda("Vazio: sem tempo. Exemplo: Const(15.0)"),
        );
    }
}

fn manutencao(obj: &ObjectDef, out: &mut Vec<Campo>) {
    let d = bloco(obj, "downtime");
    out.push(campo(
        "downtime",
        "Quebra e tem manutenção",
        Tipo::Bloco,
        sim(!d.is_null()),
        0,
    ));
    if d.is_null() {
        return;
    }
    out.push(campo(
        "downtime.mtbf",
        "Tempo entre falhas (MTBF)",
        Tipo::Distribuicao,
        dist_text(get(&d, "mtbf")),
        1,
    ));
    out.push(campo(
        "downtime.mttr",
        "Tempo de reparo (MTTR)",
        Tipo::Distribuicao,
        dist_text(get(&d, "mttr")),
        1,
    ));
    out.push(
        campo(
            "downtime.basis",
            "O tempo entre falhas conta",
            Tipo::Escolha,
            num(get(&d, "basis")),
            1,
        )
        .escolhas(&["Clock", "Busy", "Cycles"])
        .ajuda("Clock: tempo de relógio. Busy: só o tempo trabalhando. Cycles: peças processadas."),
    );
    out.push(campo(
        "downtime.repair_crew",
        "Equipe de reparo",
        Tipo::Texto,
        num(get(&d, "repair_crew")),
        1,
    ));
    let n = lista(&d, "preventive").len();
    out.push(campo(
        "downtime.preventive",
        "Preventivas (editar no arquivo)",
        Tipo::Info,
        n.to_string(),
        1,
    ));
    if !get(&d, "wear").is_null() {
        out.push(campo(
            "downtime.wear",
            "Desgaste (editar no arquivo)",
            Tipo::Info,
            "sim".into(),
            1,
        ));
    }
}

/// Splits `a.b[2].c` into keys and indexes.
enum Passo {
    Chave(String),
    Indice(usize),
}

fn passos(caminho: &str) -> Result<Vec<Passo>, String> {
    let mut out = Vec::new();
    for parte in caminho.split('.') {
        let (nome, resto) = match parte.find('[') {
            Some(i) => (&parte[..i], &parte[i..]),
            None => (parte, ""),
        };
        if nome.is_empty() {
            return Err(format!("caminho inválido: '{caminho}'"));
        }
        out.push(Passo::Chave(nome.into()));
        for idx in resto.split('[').filter(|s| !s.is_empty()) {
            let n = idx
                .trim_end_matches(']')
                .parse()
                .map_err(|_| format!("caminho inválido: '{caminho}'"))?;
            out.push(Passo::Indice(n));
        }
    }
    Ok(out)
}

fn alvo<'a>(raiz: &'a mut Value, passos: &[Passo], caminho: &str) -> Result<&'a mut Value, String> {
    let mut v = raiz;
    for p in passos {
        v = match p {
            Passo::Chave(k) => {
                if !v.is_object() {
                    *v = Value::Object(Default::default());
                }
                v.as_object_mut()
                    .expect("objeto")
                    .entry(k.clone())
                    .or_insert(Value::Null)
            }
            Passo::Indice(i) => v
                .as_array_mut()
                .and_then(|a| a.get_mut(*i))
                .ok_or_else(|| format!("'{caminho}': item {i} não existe"))?,
        };
    }
    Ok(v)
}

fn numero(texto: &str) -> Option<f64> {
    let t = texto.trim().replace(',', ".");
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// Default of a block switched on.
fn novo_bloco(nome: &str) -> Result<Value, String> {
    let v = match nome {
        "quality" => serde_json::to_value(utsim_model::quality::QualityDef::default()),
        "inspection" => serde_json::to_value(utsim_model::quality::InspectionDef::default()),
        "rework" => serde_json::to_value(utsim_model::quality::ReworkDef::default()),
        "safety" => serde_json::to_value(utsim_model::safety::SafetyDef::default()),
        "operation" => serde_json::to_value(utsim_model::OperationDef::default()),
        "downtime" => {
            let d: utsim_model::DowntimeDef =
                ron::from_str("(mtbf: Exp(mean: 3600.0), mttr: Exp(mean: 300.0))")
                    .map_err(|e| e.to_string())?;
            serde_json::to_value(d)
        }
        _ => return Err(format!("'{nome}' não é um bloco do nó")),
    };
    v.map_err(|e| e.to_string())
}

/// The command for the whole block `nome` with content `v` (`Null` removes it).
fn comando(objeto: &str, nome: &str, v: Value) -> Result<Command, String> {
    let name = objeto.to_string();
    let e = |e: serde_json::Error| format!("valor inválido para '{nome}': {e}");
    Ok(match nome {
        "quality" => Command::SetQuality {
            name,
            quality: serde_json::from_value(v).map_err(e)?,
        },
        "inspection" => Command::SetInspection {
            name,
            inspection: serde_json::from_value(v).map_err(e)?,
        },
        "rework" => Command::SetRework {
            name,
            rework: serde_json::from_value(v).map_err(e)?,
        },
        "safety" => Command::SetSafety {
            name,
            safety: serde_json::from_value(v).map_err(e)?,
        },
        "downtime" => Command::SetDowntime {
            name,
            downtime: serde_json::from_value(v).map_err(e)?,
        },
        "sink_role" => Command::SetSinkRole {
            name,
            role: serde_json::from_value(v).map_err(e)?,
        },
        "ppe" => Command::SetPpe {
            name,
            ppe: serde_json::from_value(v).map_err(e)?,
        },
        "operation" => Command::SetOperation {
            name,
            operation: serde_json::from_value(v).map_err(e)?,
        },
        _ => return Err(format!("'{nome}' não é um bloco do nó")),
    })
}

fn achar_campo(model: &Model, objeto: &str, caminho: &str) -> Result<Campo, String> {
    for aba in Aba::TODAS {
        if let Some(c) = campos(model, objeto, aba)?
            .into_iter()
            .find(|c| c.caminho == caminho)
        {
            return Ok(c);
        }
    }
    Err(format!("'{caminho}' não é um campo editável de '{objeto}'"))
}

fn obj<'a>(model: &'a Model, objeto: &str) -> Result<&'a ObjectDef, String> {
    model
        .objects
        .iter()
        .find(|o| o.name == objeto)
        .ok_or_else(|| format!("o objeto '{objeto}' não existe"))
}

/// The command that sets field `caminho` of `objeto` to `valor` (as typed in the tab).
pub fn editar(model: &Model, objeto: &str, caminho: &str, valor: &str) -> Result<Command, String> {
    let c = achar_campo(model, objeto, caminho)?;
    let o = obj(model, objeto)?;
    let passos = passos(caminho)?;
    let Some(Passo::Chave(nome)) = passos.first() else {
        return Err(format!("caminho inválido: '{caminho}'"));
    };
    let t = valor.trim();
    let sim_nao = |t: &str| match t.to_lowercase().as_str() {
        "sim" | "true" | "1" => Some(true),
        "não" | "nao" | "false" | "0" | "" => Some(false),
        _ => None,
    };
    let fora = |x: f64| c.min.is_some_and(|m| x < m) || c.max.is_some_and(|m| x > m);
    let faixa = || match (c.min, c.max) {
        (Some(a), Some(b)) if b < f64::MAX => format!(" (de {a} a {b})"),
        (Some(a), _) => format!(" (a partir de {a})"),
        _ => String::new(),
    };
    let novo: Value = match c.tipo {
        Tipo::Info | Tipo::Lista | Tipo::Item => {
            return Err(format!("'{}' não se edita por aqui", c.rotulo));
        }
        Tipo::Bloco => {
            let liga = sim_nao(t).ok_or_else(|| format!("'{}': use sim ou não", c.rotulo))?;
            let atual = bloco(o, nome);
            let v = match (liga, atual.is_null()) {
                (true, true) => novo_bloco(nome)?,
                (false, _) => Value::Null,
                (true, false) => atual,
            };
            return comando(objeto, nome, v);
        }
        Tipo::Booleano => {
            Value::Bool(sim_nao(t).ok_or_else(|| format!("'{}': use sim ou não", c.rotulo))?)
        }
        Tipo::Inteiro | Tipo::InteiroOpcional | Tipo::Real | Tipo::RealOpcional => {
            let opcional = matches!(c.tipo, Tipo::InteiroOpcional | Tipo::RealOpcional);
            if t.is_empty() && opcional {
                Value::Null
            } else {
                let x = numero(t).ok_or_else(|| format!("'{}' deve ser um número", c.rotulo))?;
                let inteiro = matches!(c.tipo, Tipo::Inteiro | Tipo::InteiroOpcional);
                if inteiro && x.fract() != 0.0 {
                    return Err(format!("'{}' deve ser um número inteiro", c.rotulo));
                }
                if fora(x) {
                    return Err(format!("'{}' fora da faixa{}", c.rotulo, faixa()));
                }
                if inteiro {
                    serde_json::json!(x as i64)
                } else {
                    serde_json::json!(x)
                }
            }
        }
        Tipo::Texto => Value::String(t.to_string()),
        Tipo::Escolha => {
            if !c.escolhas.is_empty() && !c.escolhas.iter().any(|e| e == t) {
                return Err(format!(
                    "'{}': escolha uma de {}",
                    c.rotulo,
                    c.escolhas.join(", ")
                ));
            }
            if c.caminho.ends_with(".msa_ok") {
                match t {
                    "" => Value::Null,
                    t => Value::Bool(t == "sim"),
                }
            } else if t.is_empty() && (c.caminho == "sink_role") {
                Value::Null
            } else {
                Value::String(t.to_string())
            }
        }
        Tipo::Conjunto => Value::Array(
            t.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Value::String(s.into()))
                .collect(),
        ),
        Tipo::Distribuicao if t.is_empty() && c.caminho.starts_with("operation.") => Value::Null,
        Tipo::Distribuicao => {
            let d: Dist = ron::from_str(t)
                .map_err(|e| format!("'{}': distribuição inválida ({e})", c.rotulo))?;
            serde_json::to_value(d).map_err(|e| e.to_string())?
        }
    };
    let mut raiz = serde_json::json!({ nome.clone(): bloco(o, nome) });
    *alvo(&mut raiz, &passos, caminho)? = novo;
    let v = raiz.get(nome.as_str()).cloned().unwrap_or(Value::Null);
    comando(objeto, nome, v)
}

/// Default item of a list of the tabs.
fn item_novo(lista: &str, n: usize) -> Result<Value, String> {
    use utsim_model::quality::{ControlLineDef, MeasureDef};
    use utsim_model::safety::{ErgonomicsMark, HazardDef};
    let v = match lista {
        "quality.defects" => {
            serde_json::json!({ "name": format!("defeito_{}", n + 1), "probability": 0.01 })
        }
        "quality.characteristics" => serde_json::json!({
            "name": format!("caracteristica_{}", n + 1),
            "value": serde_json::to_value(ron::from_str::<Dist>("Normal(mean: 10.0, sd: 0.1)").map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        }),
        "quality.control" => {
            serde_json::to_value(ControlLineDef::default()).map_err(|e| e.to_string())?
        }
        "inspection.measure" => serde_json::to_value(MeasureDef {
            characteristic: String::new(),
            sigma: 0.0,
        })
        .map_err(|e| e.to_string())?,
        "safety.hazards" => serde_json::to_value(HazardDef {
            id: format!("h{}", n + 1),
            description: "novo perigo".into(),
            ..HazardDef::default()
        })
        .map_err(|e| e.to_string())?,
        "safety.ergonomics" => {
            serde_json::to_value(ErgonomicsMark::default()).map_err(|e| e.to_string())?
        }
        _ => return Err(format!("'{lista}' não é uma lista das abas")),
    };
    Ok(v)
}

fn com_lista(
    model: &Model,
    objeto: &str,
    lista: &str,
    f: impl FnOnce(&mut Vec<Value>) -> Result<(), String>,
) -> Result<Command, String> {
    let o = obj(model, objeto)?;
    let (nome, campo_lista) = lista
        .split_once('.')
        .ok_or_else(|| format!("'{lista}' não é uma lista das abas"))?;
    let mut b = bloco(o, nome);
    if b.is_null() {
        return Err(format!("ligue o bloco '{nome}' antes"));
    }
    let arr = b
        .as_object_mut()
        .ok_or("bloco inválido")?
        .entry(campo_lista.to_string())
        .or_insert(Value::Array(Vec::new()));
    if !arr.is_array() {
        *arr = Value::Array(Vec::new());
    }
    f(arr.as_array_mut().expect("lista"))?;
    comando(objeto, nome, b)
}

/// The command that adds a new item to list `lista` (`safety.hazards`...).
pub fn adicionar(model: &Model, objeto: &str, lista: &str) -> Result<Command, String> {
    // Hazard ids stay unique: the next free `h<n>`.
    let o = obj(model, objeto)?;
    let usados: Vec<String> = o
        .safety
        .as_ref()
        .map(|s| s.hazards.iter().map(|h| h.id.clone()).collect())
        .unwrap_or_default();
    let primeira = o
        .quality
        .as_ref()
        .and_then(|q| q.characteristics.first())
        .map(|c| c.name.clone());
    if lista == "quality.control" && primeira.is_none() {
        return Err("cadastre uma característica medida antes do plano de controle".into());
    }
    com_lista(model, objeto, lista, |a| {
        let mut n = a.len();
        let mut item = item_novo(lista, n)?;
        if lista == "safety.hazards" {
            while usados.iter().any(|u| *u == format!("h{}", n + 1)) {
                n += 1;
            }
            item = item_novo(lista, n)?;
        }
        if let (Some(c), Some(obj)) = (&primeira, item.as_object_mut()) {
            if lista == "quality.control" {
                obj.insert("characteristic".into(), Value::String(c.clone()));
            }
        }
        a.push(item);
        Ok(())
    })
}

/// The command that removes item `indice` of list `lista`.
pub fn remover(model: &Model, objeto: &str, lista: &str, indice: usize) -> Result<Command, String> {
    com_lista(model, objeto, lista, |a| {
        if indice >= a.len() {
            return Err(format!("'{lista}': item {indice} não existe"));
        }
        a.remove(indice);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    const MODELO: &str = r#"
Model(
    name: "abas",
    run: (stop_time: 100.0, seed: 1, replications: 1),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0))),
        (name: "torno", kind: Processor(process: Const(0.5))),
        (name: "cliente", kind: Sink),
    ],
    connections: [("chegada", "torno"), ("torno", "cliente")],
)
"#;

    fn doc() -> Document {
        Document::from_model(Model::parse(MODELO).unwrap(), None, String::new())
    }

    fn aplica(d: &mut Document, f: impl FnOnce(&Model) -> Result<Command, String>) {
        let c = f(d.model()).unwrap();
        d.apply(c).unwrap();
    }

    #[test]
    fn switches_fields_and_lists_in_the_three_tabs() {
        let mut d = doc();
        let m = d.model().clone();
        assert!(
            campos(&m, "torno", Aba::Qualidade)
                .unwrap()
                .iter()
                .any(|c| c.caminho == "quality" && c.valor == "não")
        );
        aplica(&mut d, |m| editar(m, "torno", "inspection", "sim"));
        aplica(&mut d, |m| editar(m, "torno", "inspection.detect", "0,9"));
        aplica(&mut d, |m| editar(m, "torno", "inspection.every", "5"));
        let i = d.model().objects[1].inspection.as_ref().unwrap();
        assert_eq!((i.every, i.detect), (5, 0.9));
        // Ranges and types are checked before any command.
        assert!(editar(d.model(), "torno", "inspection.detect", "1.5").is_err());
        assert!(editar(d.model(), "torno", "inspection.every", "2.5").is_err());
        assert!(editar(d.model(), "torno", "inspection.on_reject", "Lixo").is_err());
        // Quality with a characteristic and its control line.
        aplica(&mut d, |m| editar(m, "torno", "quality", "sim"));
        aplica(&mut d, |m| adicionar(m, "torno", "quality.characteristics"));
        aplica(&mut d, |m| {
            editar(m, "torno", "quality.characteristics[0].name", "diametro")
        });
        aplica(&mut d, |m| {
            editar(
                m,
                "torno",
                "quality.characteristics[0].value",
                "Normal(mean: 25.0, sd: 0.01)",
            )
        });
        aplica(&mut d, |m| {
            editar(m, "torno", "quality.characteristics[0].usl", "25,05")
        });
        aplica(&mut d, |m| adicionar(m, "torno", "quality.control"));
        aplica(&mut d, |m| {
            editar(m, "torno", "quality.control[0].characteristic", "diametro")
        });
        aplica(&mut d, |m| {
            editar(m, "torno", "quality.control[0].msa_ok", "sim")
        });
        let q = d.model().objects[1].quality.as_ref().unwrap();
        assert_eq!(q.characteristics[0].usl, Some(25.05));
        assert_eq!(q.control[0].msa_ok, Some(true));
        // Safety: protective equipment, hazards with unique ids, ergonomics.
        aplica(&mut d, |m| {
            editar(m, "torno", "ppe", "oculos, luva, oculos")
        });
        assert_eq!(
            d.model().objects[1].ppe,
            vec!["oculos".to_string(), "luva".to_string()]
        );
        aplica(&mut d, |m| editar(m, "torno", "safety", "sim"));
        aplica(&mut d, |m| adicionar(m, "torno", "safety.hazards"));
        aplica(&mut d, |m| adicionar(m, "torno", "safety.hazards"));
        aplica(&mut d, |m| {
            editar(m, "torno", "safety.hazards[1].severity", "4")
        });
        assert!(editar(d.model(), "torno", "safety.hazards[1].severity", "5").is_err());
        aplica(&mut d, |m| remover(m, "torno", "safety.hazards", 0));
        aplica(&mut d, |m| adicionar(m, "torno", "safety.hazards"));
        let ids: Vec<String> = d.model().objects[1]
            .safety
            .as_ref()
            .unwrap()
            .hazards
            .iter()
            .map(|h| h.id.clone())
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        // Maintenance.
        aplica(&mut d, |m| editar(m, "torno", "downtime", "sim"));
        aplica(&mut d, |m| {
            editar(m, "torno", "downtime.mttr", "Exp(mean: 120.0)")
        });
        assert!(editar(d.model(), "torno", "downtime.mtbf", "Exp(").is_err());
        // Sink role.
        aplica(&mut d, |m| editar(m, "cliente", "sink_role", "Customer"));
        // Switching a block off removes it.
        aplica(&mut d, |m| editar(m, "torno", "inspection", "não"));
        assert!(d.model().objects[1].inspection.is_none());
        // Everything is valid, round-trips and each edit is one undo step.
        d.model().validate().unwrap();
        let texto =
            ron::ser::to_string_pretty(d.model(), ron::ser::PrettyConfig::default()).unwrap();
        assert_eq!(&Model::parse(&texto).unwrap(), d.model());
        d.undo();
        assert!(d.model().objects[1].inspection.is_some());
        assert!(d.model().objects[2].sink_role.is_some());
        // Fields of every tab carry their current value.
        let s = campos(d.model(), "torno", Aba::Seguranca).unwrap();
        assert!(s.iter().any(|c| c.caminho == "safety.hazards[0].severity"));
        let mt = campos(d.model(), "torno", Aba::Manutencao).unwrap();
        assert!(
            mt.iter()
                .any(|c| c.caminho == "downtime.mttr" && c.valor.contains("120"))
        );
    }

    #[test]
    fn operation_rule_in_the_safety_tab() {
        let mut d = doc();
        let s = campos(d.model(), "torno", Aba::Seguranca).unwrap();
        let regra = s.iter().find(|c| c.caminho == "operation").unwrap();
        assert_eq!(regra.valor, "não");
        assert!(!s.iter().any(|c| c.caminho == "operation.skills"));
        // Only a process takes the rule.
        let c = campos(d.model(), "cliente", Aba::Seguranca).unwrap();
        assert!(!c.iter().any(|c| c.caminho == "operation"));

        aplica(&mut d, |m| editar(m, "torno", "operation", "sim"));
        aplica(&mut d, |m| editar(m, "torno", "ppe", "oculos"));
        aplica(&mut d, |m| {
            editar(m, "torno", "operation.skills", "torno, cnc, torno")
        });
        aplica(&mut d, |m| {
            editar(m, "torno", "operation.require_ppe", "sim")
        });
        aplica(&mut d, |m| {
            editar(m, "torno", "operation.enter", "Const(15.0)")
        });
        let op = d.model().objects[1].operation.as_deref().unwrap();
        assert_eq!(op.skills, ["torno", "cnc"]);
        assert!(op.require_ppe);
        assert_eq!(op.enter, Some(Dist::Const(15.0)));
        assert_eq!(op.leave, None);
        let s = campos(d.model(), "torno", Aba::Seguranca).unwrap();
        let epi = s
            .iter()
            .find(|c| c.rotulo == "EPI exigido de quem opera")
            .unwrap();
        assert_eq!(epi.valor, "oculos");
        assert!(
            s.iter()
                .any(|c| c.caminho == "operation.enter" && c.valor.contains("15"))
        );

        // Empty time removes it, and the switch removes the rule.
        aplica(&mut d, |m| editar(m, "torno", "operation.enter", ""));
        assert_eq!(
            d.model().objects[1].operation.as_deref().unwrap().enter,
            None
        );
        aplica(&mut d, |m| editar(m, "torno", "operation", "não"));
        assert!(d.model().objects[1].operation.is_none());
        d.undo();
        d.model().validate().unwrap();
    }
}
