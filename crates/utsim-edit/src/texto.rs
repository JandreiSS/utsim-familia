//! Os textos das abas Qualidade, Segurança e Manutenção dos nós nas cinco línguas da
//! família: as chaves de `idiomas/<código>/abas.ftl` (`docs/traducao.md`). Saem traduzidos
//! os rótulos, as ajudas, os nomes das abas e os erros; ficam como estão os caminhos, as
//! escolhas (os valores do arquivo do modelo, como `Customer` e `Scrap`) e o `sim`/`não` dos
//! interruptores, que é o valor trocado com [`crate::abas::editar`].

use std::fmt::Display;

use utsim_idiomas::{Catalogo, Valor};

/// Os textos das abas nas cinco línguas, embutidos de `idiomas/`.
pub static TEXTOS: Catalogo = utsim_idiomas::catalogo!("abas");

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

/// O nome de um estado de bloco na língua de agora (`utsim_scene::style::state_label` fica em
/// português: é o rótulo 3D que os roteiros conferem).
pub fn estado(state: Option<utsim_core::State>) -> &'static str {
    use utsim_core::State;
    tx(match state {
        None => "estado-sem-estado",
        Some(State::Idle) => "estado-ocioso",
        Some(State::Empty) => "estado-vazio",
        Some(State::Generating) => "estado-gerando",
        Some(State::Releasing) => "estado-liberando",
        Some(State::Collecting) => "estado-coletando",
        Some(State::Setup) => "estado-preparacao",
        Some(State::Processing) => "estado-processando",
        Some(State::Blocked) => "estado-bloqueado",
        Some(State::WaitingOperator) => "estado-aguardando-operador",
        Some(State::WaitingTransport) => "estado-aguardando-transporte",
        Some(State::Down) => "estado-em-falha",
        Some(State::OffShift) => "estado-fora-do-turno",
        Some(State::TravelEmpty) => "estado-viajando-vazio",
        Some(State::TravelLoaded) => "estado-viajando-carregado",
        Some(State::Loading) => "estado-carregando",
        Some(State::Unloading) => "estado-descarregando",
        Some(State::Utilize) => "estado-trabalhando-no-posto",
        Some(State::Stopped) => "estado-parado",
        Some(State::WaitingRepair) => "estado-aguardando-tecnico",
        Some(State::Maintenance) => "estado-em-manutencao",
        Some(State::Resting) => "estado-em-pausa",
        Some(State::Entering) => "estado-operador-entrando",
        Some(State::Leaving) => "estado-operador-saindo",
    })
}

/// O rótulo de uma entrada da paleta de objetos na língua de agora (o `label` da entrada
/// fica em português: é o que os roteiros e os modelos antigos conhecem).
pub fn rotulo_paleta(e: &crate::PaletteEntry) -> &'static str {
    rotulo_de(e.id, e.label)
}

/// O mesmo, a partir do id da entrada e do rótulo em português que ela traz.
pub fn rotulo_de<'a>(id: &str, rotulo: &'a str) -> &'a str {
    match id {
        "Source" => tx("paleta-source"),
        "Queue" => tx("paleta-queue"),
        "Processor" => tx("paleta-processor"),
        "Sink" => tx("paleta-sink"),
        "Combiner" => tx("paleta-combiner"),
        "Separator" => tx("paleta-separator"),
        "Conveyor" => tx("paleta-conveyor"),
        "Rack" => tx("paleta-rack"),
        "Transporter" => tx("paleta-transporter"),
        "Operator" => tx("paleta-operator"),
        "Forklift" => tx("paleta-forklift"),
        "Agv" => tx("paleta-agv"),
        "Dispatcher" => tx("paleta-dispatcher"),
        // The drawing tools of the Godot editor (`utsim-godot` `path_edit`).
        "EsteiraDesenhada" => tx("paleta-esteira-desenhada"),
        "EsteiraCanaletas" => tx("paleta-esteira-canaletas"),
        "CaminhoPiso" => tx("paleta-caminho-piso"),
        _ => rotulo,
    }
}

/// O nome de uma categoria da paleta ([`crate::CATEGORIES`]) na língua de agora.
pub fn rotulo_categoria(categoria: &str) -> &str {
    match categoria {
        "Fluxo de itens" => tx("categoria-fluxo-de-itens"),
        "Montagem" => tx("categoria-montagem"),
        "Manuseio de materiais" => tx("categoria-manuseio-de-materiais"),
        "Recursos móveis" => tx("categoria-recursos-moveis"),
        outra => outra,
    }
}

/// O rótulo de um tipo de nó da ferramenta Fluidos na língua de agora.
pub fn rotulo_fluido(e: &crate::fluids::FluidEntry) -> &'static str {
    match e.id {
        "Source" => tx("fluido-source"),
        "Tank" => tx("fluido-tank"),
        "Pipe" => tx("fluido-pipe"),
        "Sink" => tx("fluido-sink"),
        "FromItems" => tx("fluido-fromitems"),
        "ToItems" => tx("fluido-toitems"),
        _ => e.label,
    }
}

#[cfg(test)]
mod tests_paleta {
    use super::*;

    /// Cada entrada da paleta, cada categoria e cada tipo de fluido tem texto nas cinco
    /// línguas, e em português é o rótulo de sempre.
    #[test]
    fn a_paleta_fala_as_cinco_linguas() {
        assert!(TEXTOS.faltas().is_empty(), "{:?}", TEXTOS.faltas());
        utsim_idiomas::definir_nesta_thread(Some(utsim_idiomas::Idioma::PtBr));
        for e in crate::PALETTE {
            assert_eq!(rotulo_paleta(&e), e.label, "{}", e.id);
        }
        for c in crate::CATEGORIES {
            assert_eq!(rotulo_categoria(c), c);
        }
        for e in crate::fluids::FLUID_PALETTE {
            assert_eq!(rotulo_fluido(&e), e.label, "{}", e.id);
        }
        utsim_idiomas::definir_nesta_thread(Some(utsim_idiomas::Idioma::En));
        assert_eq!(
            rotulo_paleta(crate::palette_entry("Queue").unwrap()),
            "Queue"
        );
        assert_eq!(rotulo_categoria("Montagem"), "Assembly");
        utsim_idiomas::definir_nesta_thread(None);
    }
}
