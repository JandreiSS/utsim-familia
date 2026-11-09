//! Validation of a mission against its model: every key exists, every range is possible,
//! the budget allows a solution and every measure is produced by the model.

use std::collections::{BTreeMap, BTreeSet};

use utsim_model::Model;
use utsim_model::experiment::apply_patches;

use crate::missao::{Alavanca, Missao, Previsao, Resposta, Termo};
use crate::variante;

/// Every problem of `missao` over `modelo` (empty when valid).
pub fn validar_missao(missao: &Missao, modelo: &Model) -> Result<(), Vec<String>> {
    let mut erros = Vec::new();
    if missao.id.trim().is_empty() {
        erros.push("a missão não tem id".into());
    }
    let so_perguntas =
        missao.alavancas.is_empty() && !missao.tem_pecas() && !missao.previsao.is_empty();
    if missao.metricas.is_empty() && !so_perguntas && missao.modelagem.is_none() {
        erros.push(
            "a missão não tem métricas (só uma missão de perguntas ou de modelagem pode não ter)"
                .into(),
        );
    }
    if missao.dicas.len() > 3 {
        erros.push("no máximo três níveis de dica".into());
    }
    if missao.rodadas.replicacoes < 2 {
        erros.push("são precisas pelo menos 2 replicações para o intervalo".into());
    }

    let mut ids = BTreeSet::new();
    let mut chaves = BTreeSet::new();
    for a in &missao.alavancas {
        if !ids.insert(a.id().to_string()) {
            erros.push(format!("alavanca '{}' repetida", a.id()));
        }
        match a {
            Alavanca::Quantidade { de, ate, .. } if de > ate => {
                erros.push(format!("alavanca '{}': 'de' maior que 'ate'", a.id()));
            }
            Alavanca::Valor { de, ate, passo, .. } if de > ate || *passo <= 0.0 => {
                erros.push(format!("alavanca '{}': faixa ou passo impossível", a.id()));
            }
            Alavanca::Escolha { opcoes, .. } if opcoes.is_empty() => {
                erros.push(format!("alavanca '{}': sem opções", a.id()));
            }
            _ => {}
        }
        if !a.aceita(a.inicial()) {
            erros.push(format!(
                "alavanca '{}': valor inicial fora da faixa",
                a.id()
            ));
        }
        // Every value of every lever must patch the model.
        let mut minhas = BTreeSet::new();
        for v in a.valores() {
            let aj = a.ajustes(v);
            minhas.extend(aj.keys().cloned());
            if let Err(e) = apply_patches(modelo, &aj) {
                erros.push(format!("alavanca '{}' = {}: {e}", a.id(), a.descrever(v)));
                break;
            }
        }
        for k in minhas {
            if !chaves.insert(k.clone()) {
                erros.push(format!("a chave '{k}' é usada por mais de uma alavanca"));
            }
        }
    }

    // The variant patches the model at both ends of each range.
    for f in &missao.variante {
        if f.de > f.ate {
            erros.push(format!("variante '{}': 'de' maior que 'ate'", f.chave));
            continue;
        }
        for v in [f.de, f.ate] {
            let aj = BTreeMap::from([(f.chave.clone(), variante::texto_faixa(f, v))]);
            if let Err(e) = apply_patches(modelo, &aj) {
                erros.push(format!("variante '{}': {e}", f.chave));
                break;
            }
        }
    }

    // The pieces: their own definition, and the compiled keys patch the model.
    let objetos: Vec<String> = modelo.objects.iter().map(|o| o.name.clone()).collect();
    if let Some(b) = &missao.balanceamento {
        erros.extend(b.validar());
        if erros.is_empty()
            && let Err(e) = apply_patches(modelo, &b.ajustes(&b.inicial))
        {
            erros.push(format!(
                "balanceamento: as estações do modelo não aceitam os tempos: {e}"
            ));
        }
    }
    if let Some(a) = &missao.arranjo {
        erros.extend(a.validar());
        if erros.is_empty()
            && let Err(e) = apply_patches(modelo, &a.ajustes(&a.inicial, &objetos))
        {
            erros.push(format!("arranjo: {e}"));
        }
    }
    if missao.tem_pecas() && !missao.alavancas.is_empty() {
        erros.push(
            "uma missão com tarefas ou máquinas para posicionar não pode ter outras alavancas"
                .into(),
        );
    }

    // Reference: valid decisions inside the budget and the limits.
    let iniciais = missao.decisoes_iniciais();
    for (k, v) in &missao.referencia {
        if (k.starts_with("tarefa.") || k.starts_with("pos.")) && iniciais.contains_key(k) {
            continue;
        }
        match missao.alavanca(k) {
            None => erros.push(format!("referência: alavanca '{k}' não existe")),
            Some(a) if !a.aceita(*v) => {
                erros.push(format!("referência: valor {v} fora da faixa de '{k}'"));
            }
            _ => {}
        }
    }
    // An exam mission has no reference ([`crate::prova::sem_gabarito`]).
    if !missao.referencia.is_empty()
        && let Some(motivo) = crate::nota::portao(missao, &missao.referencia)
    {
        erros.push(format!("a referência não passa nas restrições: {motivo}"));
    }
    if !missao.alavancas.is_empty()
        && missao.orcamento.is_some()
        && !missao
            .combinacoes()
            .iter()
            .any(|d| crate::nota::portao(missao, d).is_none())
    {
        erros.push("o orçamento não permite nenhuma solução".into());
    }

    let mut previsoes = BTreeSet::new();
    for p in &missao.previsao {
        if !previsoes.insert(p.id().to_string()) {
            erros.push(format!("previsão '{}' repetida", p.id()));
        }
        if let Previsao::Escolha {
            opcoes, resposta, ..
        } = p
        {
            if opcoes.len() < 2 {
                erros.push(format!("previsão '{}': pelo menos duas opções", p.id()));
            }
            match resposta {
                Resposta::Fixa(i) if *i >= opcoes.len() => {
                    erros.push(format!("previsão '{}': resposta fora das opções", p.id()));
                }
                Resposta::Maior | Resposta::Menor if opcoes.iter().any(|o| o.valor.is_empty()) => {
                    erros.push(format!("previsão '{}': opção sem valor", p.id()));
                }
                _ => {}
            }
        }
    }
    for (i, q) in missao.justificativa.iter().enumerate() {
        if q.certa >= q.opcoes.len() {
            erros.push(format!("justificativa {}: resposta fora das opções", i + 1));
        }
    }

    // Terms: decision terms name levers; the others must be metrics of the model.
    let mut termos: Vec<(&str, &Termo)> = Vec::new();
    for m in &missao.metricas {
        termos.extend(m.valor.iter().map(|t| (m.nome.as_str(), t)));
        termos.extend(m.divisor.iter().map(|t| (m.nome.as_str(), t)));
    }
    for m in &missao.medicoes {
        termos.extend(m.valor.iter().map(|t| (m.rotulo.as_str(), t)));
    }
    for p in &missao.previsao {
        match p {
            Previsao::Valor { id, valor, .. } => {
                termos.extend(valor.iter().map(|t| (id.as_str(), t)))
            }
            Previsao::Escolha { id, opcoes, .. } => {
                for o in opcoes {
                    termos.extend(o.valor.iter().map(|t| (id.as_str(), t)));
                }
            }
            Previsao::Calculo { id, calculo, .. } => {
                if !calculo.valor().is_finite() {
                    erros.push(format!("previsão '{id}': o cálculo não dá um número"));
                }
            }
            Previsao::Dados {
                id,
                conjunto,
                questao,
                ..
            } => match missao.conjunto(conjunto) {
                None => erros.push(format!("previsão '{id}': conjunto '{conjunto}' não existe")),
                Some(cj) => {
                    if let Err(e) = questao.resposta(&cj.gerar(&missao.id, "")) {
                        erros.push(format!("previsão '{id}': {e}"));
                    }
                    if questao.e_escolha() && questao.opcoes(cj).len() < 2 {
                        erros.push(format!(
                            "previsão '{id}': a pergunta precisa de grupos no conjunto"
                        ));
                    }
                }
            },
        }
    }
    for a in &missao.analisar_em {
        if !crate::missao::APPS_DE_ANALISE.contains(&a.as_str()) {
            erros.push(format!("analisar_em: app '{a}' desconhecido"));
        }
        if missao.dados.is_empty() {
            erros.push(format!("analisar_em: '{a}' sem conjunto de dados"));
        }
    }
    for r in &missao.restricoes {
        for t in &r.valor {
            if !termo_de_decisao(t) {
                erros.push(format!(
                    "restrição '{}': só usa custo, decisao.<id> e const",
                    r.rotulo
                ));
            }
        }
        termos.extend(r.valor.iter().map(|t| (r.rotulo.as_str(), t)));
    }
    let mut do_modelo = BTreeSet::new();
    for (onde, t) in &termos {
        if let Some(id) = t.chave.strip_prefix("decisao.") {
            if missao.alavanca(id).is_none() {
                erros.push(format!("'{onde}': alavanca '{id}' não existe"));
            }
        } else if !termo_de_decisao(t) {
            do_modelo.insert((onde.to_string(), t.chave.clone()));
        }
    }
    if erros.is_empty() && !do_modelo.is_empty() {
        let mut m = modelo.clone();
        m.run.replications = 1;
        match m.run_replications() {
            Ok(reports) => {
                let conhecidas: BTreeSet<String> = reports
                    .iter()
                    .flat_map(|r| r.metrics().into_keys())
                    .collect();
                for (onde, k) in do_modelo {
                    if !conhecidas.contains(&k) {
                        erros.push(format!("'{onde}': a métrica '{k}' não sai do modelo"));
                    }
                }
            }
            Err(e) => erros.push(format!("o modelo não roda: {e}")),
        }
    }

    if erros.is_empty() { Ok(()) } else { Err(erros) }
}

fn termo_de_decisao(t: &Termo) -> bool {
    t.chave == "custo"
        || t.chave == "const"
        || t.chave.starts_with("decisao.")
        || crate::pecas::e_termo(&t.chave)
}
