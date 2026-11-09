//! Catalogue of the mission screen (missao.rs). Entries start with `missao_`.

textos! {
    // Messages after a run.
    missao_erro_salvar_progresso:
        "não foi possível salvar o progresso: {}",
        "could not save the progress: {}",
        "no fue posible guardar el progreso: {}";
    missao_competencias_erros:
        "competências: {}",
        "competencies: {}",
        "competencias: {}";
    missao_competencias_nao_registradas:
        "competências não registradas no programa: {}",
        "competencies not recorded in the program: {}",
        "competencias no registradas en el programa: {}";
    missao_tentativa_nao_registrada:
        "tentativa não registrada no diário: {}",
        "attempt not recorded in the journal: {}",
        "intento no registrado en el diario: {}";

    // Situation of an indicator against the goal.
    missao_meta_atingida:
        "meta atingida",
        "goal met",
        "meta alcanzada";
    missao_dentro_tolerancia:
        "dentro da tolerância",
        "within tolerance",
        "dentro de la tolerancia";
    missao_meta_nao_atingida:
        "meta não atingida",
        "goal not met",
        "meta no alcanzada";
    missao_sem_meta:
        "sem meta",
        "no goal",
        "sin meta";

    // Levers.
    missao_cada_unidade:
        "{} cada unidade a mais",
        "{} per extra unit",
        "{} por cada unidad adicional";

    // Header and statement.
    missao_campanha:
        "← Campanha",
        "← Campaign",
        "← Campaña";
    missao_numero:
        "Missão {}",
        "Mission {}",
        "Misión {}";
    missao_conceitos:
        "Conceitos: {}",
        "Concepts: {}",
        "Conceptos: {}";
    missao_medicoes:
        "Medições do estado atual",
        "Measurements of the current state",
        "Mediciones del estado actual";

    // Data of the student's factory.
    missao_dados_titulo:
        "Dados da sua fábrica",
        "Data from your factory",
        "Datos de su fábrica";
    missao_dados_texto:
        "Os dados foram gerados para você: outro aluno recebe outros números. Analise com as ferramentas do jogo ou leve para o UTSim Seis Sigma, e responda abaixo.",
        "The data were generated for you: another student gets other numbers. Analyze them with the game's tools or take them to UTSim Seis Sigma, and answer below.",
        "Los datos se generaron para usted: otro alumno recibe otros números. Analícelos con las herramientas del juego o llévelos a UTSim Seis Sigma, y responda abajo.";
    missao_dados_linhas:
        "{} ({} linhas)",
        "{} ({} rows)",
        "{} ({} filas)";
    missao_mais_linhas:
        "... e mais {} linhas no arquivo",
        "... and {} more rows in the file",
        "... y {} filas más en el archivo";
    missao_ferramentas:
        "Ferramentas do jogo",
        "Game tools",
        "Herramientas del juego";
    missao_pasta:
        "Pasta",
        "Folder",
        "Carpeta";
    missao_gravar_csv:
        "Gravar os dados (.csv)",
        "Save the data (.csv)",
        "Guardar los datos (.csv)";
    missao_analisar_estatistica:
        "Analisar no UTSim Estatística",
        "Analyse in UTSim Estatística",
        "Analizar en UTSim Estatística";
    missao_analisar_qualidade:
        "Analisar no UTSim Qualidade",
        "Analyse in UTSim Qualidade",
        "Analizar en UTSim Qualidade";
    missao_analisar_dica:
        "Grava os seus dados desta missão (.csv) e abre no app. Analise lá e digite as respostas aqui: o jogo corrige pelos mesmos dados.",
        "Saves your data of this mission (.csv) and opens them in the app. Analyse there and type the answers here: the game grades them from the same data.",
        "Guarda sus datos de esta misión (.csv) y los abre en la aplicación. Analice allí y escriba las respuestas aquí: el juego las corrige con los mismos datos.";
    missao_dados_abertos:
        "{} arquivo(s) gravado(s) em {} e aberto(s) no app",
        "{} file(s) saved to {} and opened in the app",
        "{} archivo(s) guardado(s) en {} y abierto(s) en la aplicación";
    missao_levar_seis_sigma:
        "Levar para o UTSim Seis Sigma",
        "Take to UTSim Seis Sigma",
        "Llevar a UTSim Seis Sigma";
    missao_arquivos_gravados:
        "{} arquivo(s) gravado(s) em {}",
        "{} file(s) saved in {}",
        "{} archivo(s) guardado(s) en {}";
    missao_erro_gravar:
        "não foi possível gravar: {}",
        "could not save: {}",
        "no fue posible guardar: {}";
    missao_projeto_criado:
        "projeto de treino criado em {}: abra no UTSim Seis Sigma, analise e responda aqui",
        "training project created in {}: open it in UTSim Seis Sigma, analyze it and answer here",
        "proyecto de práctica creado en {}: ábralo en UTSim Seis Sigma, analícelo y responda aquí";
    missao_erro_projeto:
        "não foi possível criar o projeto: {}",
        "could not create the project: {}",
        "no fue posible crear el proyecto: {}";

    // Prediction, decisions and pieces.
    missao_previsao:
        "Previsão",
        "Prediction",
        "Predicción";
    missao_decisoes:
        "Decisões",
        "Decisions",
        "Decisiones";
    missao_custo_orcamento:
        "Custo das decisões: {} de {} do orçamento",
        "Cost of the decisions: {} of a {} budget",
        "Costo de las decisiones: {} de {} del presupuesto";
    missao_custo:
        "Custo das decisões: {}",
        "Cost of the decisions: {}",
        "Costo de las decisiones: {}";
    missao_restricao:
        "Restrição: {}",
        "Constraint: {}",
        "Restricción: {}";
    missao_balanceamento:
        "Balanceamento da linha",
        "Line balancing",
        "Balanceo de línea";
    missao_arranjo:
        "Arranjo físico",
        "Facility layout",
        "Distribución en planta";

    // Modelling mission.
    missao_seu_modelo:
        "Seu modelo",
        "Your model",
        "Su modelo";
    missao_modelo_texto:
        "Monte o modelo no UTSim com os nomes de objeto pedidos no enunciado, grave o arquivo .ron e indique-o aqui. O botão \"Gravar modelo inicial\" grava um ponto de partida.",
        "Build the model in UTSim with the object names the statement asks for, save the .ron file and point to it here. The \"Save initial model\" button saves a starting point.",
        "Arme el modelo en UTSim con los nombres de objeto que pide el enunciado, guarde el archivo .ron e indíquelo aquí. El botón \"Guardar modelo inicial\" guarda un punto de partida.";
    missao_arquivo_ron:
        "Arquivo .ron",
        ".ron file",
        "Archivo .ron";
    missao_gravar_modelo_inicial:
        "Gravar modelo inicial",
        "Save initial model",
        "Guardar modelo inicial";
    missao_ler_modelo:
        "Ler o modelo",
        "Load the model",
        "Leer el modelo";
    missao_modelo_inicial_gravado:
        "modelo inicial gravado em {}",
        "initial model saved in {}",
        "modelo inicial guardado en {}";
    missao_modelo_lido:
        "modelo lido ({} linhas)",
        "model loaded ({} lines)",
        "modelo leído ({} líneas)";
    missao_erro_ler:
        "não foi possível ler: {}",
        "could not read: {}",
        "no fue posible leer: {}";
    missao_nenhum_modelo:
        "Nenhum modelo lido ainda.",
        "No model loaded yet.",
        "Todavía no se leyó ningún modelo.";

    // Justification and hints.
    missao_justificativa:
        "Justificativa",
        "Justification",
        "Justificación";
    missao_dicas:
        "Dicas",
        "Hints",
        "Pistas";
    missao_ver_dica_desconto:
        "Ver dica {} (desconta {} pontos)",
        "Show hint {} (costs {} points)",
        "Ver pista {} (descuenta {} puntos)";
    missao_ver_dica:
        "Ver dica {} (sem desconto)",
        "Show hint {} (no penalty)",
        "Ver pista {} (sin descuento)";

    // Run.
    missao_rodadas_usadas:
        "as {} rodada(s) desta missão já foram usadas",
        "the {} run(s) of this mission have already been used",
        "ya se usaron las {} corrida(s) de esta misión";
    missao_simular_bloqueado:
        "Simular está bloqueado: {}.",
        "Simulate is blocked: {}.",
        "Simular está bloqueado: {}.";
    missao_responda_previsoes:
        "Responda às previsões para simular.",
        "Answer the predictions to simulate.",
        "Responda las predicciones para simular.";
    missao_rodadas_de:
        "Rodadas: {} de {}",
        "Runs: {} of {}",
        "Corridas: {} de {}";
    missao_rodadas_sem_desconto:
        "Rodadas: {} de {} sem desconto",
        "Runs: {} of {} without penalty",
        "Corridas: {} de {} sin descuento";
    missao_rodadas_feitas:
        "Rodadas feitas: {}",
        "Runs done: {}",
        "Corridas hechas: {}";
    missao_sua_fabrica:
        "A sua fábrica (uma replicação)",
        "Your factory (one replication)",
        "Su fábrica (una réplica)";
    missao_ver_3d:
        "Ver em 3D",
        "View in 3D",
        "Ver en 3D";
    missao_levar_utsim:
        "Levar para o UTSim",
        "Take to UTSim",
        "Llevar a UTSim";
    missao_levar_utsim_dica:
        "Grava o modelo desta tentativa, com as suas decisões, e abre no UTSim para você continuar no simulador completo. A missão não muda.",
        "Saves the model of this attempt, with your decisions, and opens it in UTSim so you can go on in the full simulator. The mission does not change.",
        "Guarda el modelo de este intento, con sus decisiones, y lo abre en UTSim para que usted continúe en el simulador completo. La misión no cambia.";
    missao_levado_utsim:
        "modelo gravado em {} e aberto no UTSim",
        "model saved to {} and opened in UTSim",
        "modelo guardado en {} y abierto en UTSim";
    missao_erro_3d:
        "não foi possível abrir em 3D: {}",
        "could not open in 3D: {}",
        "no fue posible abrir en 3D: {}";
    missao_proxima:
        "Próxima missão →",
        "Next mission →",
        "Siguiente misión →";
    missao_caixa_areia:
        "Caixa de areia",
        "Sandbox",
        "Caja de arena";

    // Result.
    missao_resultado_rodada:
        "Resultado da rodada",
        "Result of the run",
        "Resultado de la corrida";
    missao_nota_professor:
        "A nota sai da correção do professor, que simula de novo as suas decisões.",
        "The grade comes from the teacher's correction, which simulates your decisions again.",
        "La nota sale de la corrección del profesor, que simula de nuevo sus decisiones.";
    missao_nota:
        "Nota {}",
        "Grade {}",
        "Nota {}";
    missao_resolvida:
        "resolvida",
        "solved",
        "resuelta";
    missao_nao_resolvida:
        "não resolvida",
        "not solved",
        "no resuelta";
    missao_selo:
        "selo: melhor que a referência",
        "badge: better than the reference",
        "sello: mejor que la referencia";
    missao_restricao_violada:
        "Restrição violada: {}",
        "Constraint violated: {}",
        "Restricción violada: {}";
    missao_pontos:
        "Pontos: desempenho {}, previsão {}, justificativa {} (de {} possíveis) · descontos {} (dicas) e {} (rodadas)",
        "Points: performance {}, prediction {}, justification {} (out of {}) · penalties {} (hints) and {} (runs)",
        "Puntos: desempeño {}, predicción {}, justificación {} (de {} posibles) · descuentos {} (pistas) y {} (corridas)";
    missao_indicador:
        "Indicador",
        "Indicator",
        "Indicador";
    missao_estado_inicial:
        "Estado inicial",
        "Initial state",
        "Estado inicial";
    missao_sua_tentativa:
        "Sua tentativa",
        "Your attempt",
        "Su intento";
    missao_meta:
        "Meta",
        "Goal",
        "Meta";
    missao_situacao:
        "Situação",
        "Status",
        "Situación";
    missao_previsao_certa:
        "Previsão certa: {} {}",
        "Correct prediction: {} {}",
        "Predicción correcta: {} {}";
    missao_previsao_errada:
        "Previsão errada: {} A resposta da simulação é {}.",
        "Wrong prediction: {} The simulation's answer is {}.",
        "Predicción equivocada: {} La respuesta de la simulación es {}.";
    missao_justificativa_certas:
        "Justificativa: {} de {} certas",
        "Justification: {} of {} correct",
        "Justificación: {} de {} correctas";
    missao_criterios:
        "Critérios do modelo",
        "Model criteria",
        "Criterios del modelo";
    missao_criterio_ok:
        "OK",
        "OK",
        "OK";
    missao_criterio_falta:
        "Falta",
        "Missing",
        "Falta";
    missao_o_que_aconteceu:
        "O que aconteceu",
        "What happened",
        "Qué pasó";

    // Sandbox.
    missao_areia_titulo:
        "Caixa de areia: {}",
        "Sandbox: {}",
        "Caja de arena: {}";
    missao_cenario_a:
        "Cenário A",
        "Scenario A",
        "Escenario A";
    missao_cenario_b:
        "Cenário B",
        "Scenario B",
        "Escenario B";
    missao_custo_curto:
        "Custo: {}",
        "Cost: {}",
        "Costo: {}";
    missao_comparar:
        "Comparar A e B",
        "Compare A and B",
        "Comparar A y B";
    missao_coluna_a:
        "A",
        "A",
        "A";
    missao_coluna_b:
        "B",
        "B",
        "B";
    missao_diferenca_pareada:
        "B menos A (pareada)",
        "B minus A (paired)",
        "B menos A (pareada)";
    missao_diferenca_significativa:
        "diferença significativa",
        "significant difference",
        "diferencia significativa";
    missao_empate:
        "empate estatístico",
        "statistical tie",
        "empate estadístico";
    missao_levar_cenario:
        "Levar o cenário A para o UTSim (grava o modelo .ron com as decisões aplicadas):",
        "Take scenario A to UTSim (saves the .ron model with the decisions applied):",
        "Llevar el escenario A a UTSim (guarda el modelo .ron con las decisiones aplicadas):";
    missao_gravar_ron:
        "Gravar .ron",
        "Save .ron",
        "Guardar .ron";
    missao_modelo_gravado:
        "modelo gravado em {}",
        "model saved in {}",
        "modelo guardado en {}";
}
