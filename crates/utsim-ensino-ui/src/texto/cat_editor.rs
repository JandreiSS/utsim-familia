//! Catalogue of the mission editor (editor.rs). Entries start with `ed_`.

textos! {
    aba_editor:
        "Editor de missões",
        "Mission editor",
        "Editor de misiones";
    ed_titulo:
        "Editor de missões",
        "Mission editor",
        "Editor de misiones";
    ed_texto:
        "Monte uma missão a partir de um modelo do UTSim, sem programar. Marque as alavancas e as métricas, ajuste faixas, custos e metas e calibre: o app simula todas as combinações, acha a referência e sugere as metas. Grave a missão no seu pacote e publique o .utse na turma.",
        "Build a mission from a UTSim model, without programming. Tick the levers and the measures, adjust ranges, costs and goals and calibrate: the app simulates every combination, finds the reference and suggests the goals. Write the mission to your package and publish the .utse to the class.",
        "Arme una misión a partir de un modelo de UTSim, sin programar. Marque las palancas y las métricas, ajuste rangos, costos y metas y calibre: la app simula todas las combinaciones, encuentra la referencia y sugiere las metas. Grabe la misión en su paquete y publique el .utse en el grupo.";
    ed_pacote:
        "Seu pacote",
        "Your package",
        "Su paquete";
    ed_pasta_pacote:
        "Pasta do pacote",
        "Package folder",
        "Carpeta del paquete";
    ed_abrir_criar:
        "Abrir ou criar",
        "Open or create",
        "Abrir o crear";
    ed_escolha_pasta:
        "escolha a pasta do pacote",
        "choose the package folder",
        "elija la carpeta del paquete";
    ed_pacote_aberto:
        "pacote \"{}\" aberto, com {} missão(ões)",
        "package \"{}\" open, with {} mission(s)",
        "paquete \"{}\" abierto, con {} misión(es)";
    ed_pacote_novo:
        "pacote novo em {}: ele é gravado com a primeira missão",
        "new package in {}: it is written with the first mission",
        "paquete nuevo en {}: se graba con la primera misión";
    ed_meu_pacote:
        "Meu pacote",
        "My package",
        "Mi paquete";
    ed_titulo_pacote:
        "Título",
        "Title",
        "Título";
    ed_autor:
        "Autor",
        "Author",
        "Autor";
    ed_pacote_vazio:
        "O pacote ainda não tem missões.",
        "The package has no missions yet.",
        "El paquete todavía no tiene misiones.";
    ed_ato_n:
        "Ato {}",
        "Act {}",
        "Acto {}";
    ed_ato_novo:
        "Ato novo",
        "New act",
        "Acto nuevo";
    ed_editar:
        "Editar",
        "Edit",
        "Editar";
    ed_remover:
        "Remover",
        "Remove",
        "Quitar";
    ed_conferir_pacote:
        "Conferir o pacote",
        "Check the package",
        "Verificar el paquete";
    ed_gravar_utse:
        "Gravar o .utse",
        "Write the .utse",
        "Grabar el .utse";
    ed_gravar_pasta:
        "Gravar a pasta",
        "Write the folder",
        "Grabar la carpeta";
    ed_gravado:
        "pacote gravado em {}",
        "package written to {}",
        "paquete grabado en {}";
    ed_pacote_confere:
        "o pacote confere: {} missão(ões) em {} ato(s)",
        "the package checks out: {} mission(s) in {} act(s)",
        "el paquete está correcto: {} misión(es) en {} acto(s)";
    ed_nova:
        "Missão nova a partir de um modelo",
        "New mission from a model",
        "Misión nueva a partir de un modelo";
    ed_filtro_modelo:
        "Modelo do UTSim",
        "UTSim model",
        "Modelo de UTSim";
    ed_criar_rascunho:
        "Criar o rascunho",
        "Create the draft",
        "Crear el borrador";
    ed_sem_alavancas:
        "o modelo não tem capacidades, filas limitadas nem equipes para virar alavancas",
        "the model has no capacities, limited queues or teams to become levers",
        "el modelo no tiene capacidades, colas limitadas ni equipos para volverse palancas";
    ed_missao:
        "Missão",
        "Mission",
        "Misión";
    ed_id:
        "Identificador",
        "Identifier",
        "Identificador";
    ed_ato:
        "Ato",
        "Act",
        "Acto";
    ed_orcamento:
        "Orçamento",
        "Budget",
        "Presupuesto";
    ed_alavancas:
        "Alavancas",
        "Levers",
        "Palancas";
    ed_rotulo:
        "Rótulo",
        "Label",
        "Etiqueta";
    ed_de:
        "De",
        "From",
        "Desde";
    ed_ate:
        "Até",
        "To",
        "Hasta";
    ed_passo:
        "Passo",
        "Step",
        "Paso";
    ed_custo:
        "Custo por unidade",
        "Cost per unit",
        "Costo por unidad";
    ed_opcoes:
        "{} opções",
        "{} options",
        "{} opciones";
    ed_combinacoes:
        "{} combinação(ões) para simular na calibragem",
        "{} combination(s) to simulate in the calibration",
        "{} combinación(es) para simular en la calibración";
    ed_metricas:
        "Métricas",
        "Measures",
        "Métricas";
    ed_melhor:
        "Melhor quando",
        "Better when",
        "Mejor cuando";
    ed_menor:
        "menor",
        "lower",
        "menor";
    ed_maior:
        "maior",
        "higher",
        "mayor";
    ed_meta:
        "Meta",
        "Goal",
        "Meta";
    ed_tolerancia:
        "Tolerância",
        "Tolerance",
        "Tolerancia";
    ed_peso:
        "Peso na nota",
        "Weight in the grade",
        "Peso en la nota";
    ed_enunciado:
        "Enunciado (Markdown)",
        "Statement (Markdown)",
        "Enunciado (Markdown)";
    ed_calibrar:
        "Calibrar",
        "Calibrate",
        "Calibrar";
    ed_calibrar_dica:
        "Simula todas as combinações das alavancas marcadas e acha a melhor dentro do orçamento.",
        "Simulates every combination of the ticked levers and finds the best one within the budget.",
        "Simula todas las combinaciones de las palancas marcadas y encuentra la mejor dentro del presupuesto.";
    ed_calibrando:
        "calibrando...",
        "calibrating...",
        "calibrando...";
    ed_gravar_no_pacote:
        "Gravar no pacote",
        "Write to the package",
        "Grabar en el paquete";
    ed_gravar_dica:
        "Grava a missão na pasta do pacote, no lugar dela se o identificador já existe.",
        "Writes the mission to the package folder, in its place when the identifier already exists.",
        "Graba la misión en la carpeta del paquete, en su lugar si el identificador ya existe.";
    ed_fechar:
        "Fechar a missão",
        "Close the mission",
        "Cerrar la misión";
    ed_aplicar:
        "Usar a referência e as metas sugeridas",
        "Use the reference and the suggested goals",
        "Usar la referencia y las metas sugeridas";
    ed_aplicado:
        "referência e metas aplicadas: grave a missão para guardar",
        "reference and goals applied: write the mission to keep them",
        "referencia y metas aplicadas: grabe la misión para guardarlas";
    ed_missao_gravada:
        "missão gravada em {} ({})",
        "mission written to {} ({})",
        "misión grabada en {} ({})";
    ed_resultado:
        "{} combinações, {} dentro do orçamento e das restrições, {} batem todas as metas ({} %)",
        "{} combinations, {} within the budget and the limits, {} reach every goal ({} %)",
        "{} combinaciones, {} dentro del presupuesto y las restricciones, {} alcanzan todas las metas ({} %)";
    ed_metrica:
        "Métrica",
        "Measure",
        "Métrica";
    ed_base:
        "Situação inicial",
        "Starting point",
        "Situación inicial";
    ed_referencia:
        "Referência",
        "Reference",
        "Referencia";
    ed_meta_sugerida:
        "Meta sugerida",
        "Suggested goal",
        "Meta sugerida";
    ed_melhor_combinacao:
        "melhor combinação: {}",
        "best combination: {}",
        "mejor combinación: {}";
}
