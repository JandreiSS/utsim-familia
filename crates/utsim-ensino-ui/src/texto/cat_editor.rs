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
    ed_ato_fixo:
        "o mesmo da missão gravada (use as setas da lista para mudar)",
        "the same as the written mission (use the arrows of the list to change it)",
        "el mismo de la misión grabada (use las flechas de la lista para cambiarlo)";
    ed_pasta_ocupada:
        "a pasta já tem arquivos e não tem pacote.ron: escolha uma pasta vazia para um pacote novo",
        "the folder already has files and no pacote.ron: choose an empty folder for a new package",
        "la carpeta ya tiene archivos y no tiene pacote.ron: elija una carpeta vacía para un paquete nuevo";
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
    ed_previa:
        "Prévia",
        "Preview",
        "Vista previa";
    ed_conceitos:
        "Conceitos do glossário ({} marcados)",
        "Glossary concepts ({} ticked)",
        "Conceptos del glosario ({} marcados)";
    ed_conceito_fora:
        "{} (fora do glossário)",
        "{} (not in the glossary)",
        "{} (fuera del glosario)";
    ed_medicoes:
        "Medições antes da previsão",
        "Measurements before the prediction",
        "Mediciones antes de la predicción";
    ed_add_medicao:
        "Adicionar medição",
        "Add measurement",
        "Agregar medición";
    ed_chave:
        "Chave",
        "Key",
        "Clave";
    ed_unidade:
        "Unidade",
        "Unit",
        "Unidad";
    ed_casas:
        "Casas",
        "Decimals",
        "Decimales";
    ed_formula_n:
        "fórmula de {} termos (mantida)",
        "formula of {} terms (kept)",
        "fórmula de {} términos (mantenida)";
    ed_chave_mais:
        "mais {} chaves: digite para filtrar",
        "{} more keys: type to filter",
        "{} claves más: escriba para filtrar";
    ed_previsoes:
        "Perguntas de previsão",
        "Prediction questions",
        "Preguntas de predicción";
    ed_add_escolha:
        "Adicionar escolha",
        "Add choice",
        "Agregar elección";
    ed_add_valor:
        "Adicionar valor",
        "Add value",
        "Agregar valor";
    ed_add_calculo:
        "Adicionar cálculo",
        "Add calculation",
        "Agregar cálculo";
    ed_pv_escolha:
        "Escolha",
        "Choice",
        "Elección";
    ed_pv_valor:
        "Valor",
        "Value",
        "Valor";
    ed_pv_calculo:
        "Cálculo",
        "Calculation",
        "Cálculo";
    ed_pergunta:
        "Pergunta",
        "Question",
        "Pregunta";
    ed_opcao:
        "Opção",
        "Option",
        "Opción";
    ed_add_opcao:
        "Adicionar opção",
        "Add option",
        "Agregar opción";
    ed_resposta:
        "Resposta",
        "Answer",
        "Respuesta";
    ed_resp_fixa:
        "fixa: opção {}",
        "fixed: option {}",
        "fija: opción {}";
    ed_resp_maior:
        "a de maior valor no estado inicial",
        "the one with the highest value in the initial state",
        "la de mayor valor en el estado inicial";
    ed_resp_menor:
        "a de menor valor no estado inicial",
        "the one with the lowest value in the initial state",
        "la de menor valor en el estado inicial";
    ed_tol_relativa:
        "relativa (%)",
        "relative (%)",
        "relativa (%)";
    ed_tol_absoluta:
        "absoluta",
        "absolute",
        "absoluta";
    ed_calc_abt:
        "Máquinas por operador",
        "Machines per operator",
        "Máquinas por operario";
    ed_calc_normal:
        "Tempo normal",
        "Normal time",
        "Tiempo normal";
    ed_calc_padrao:
        "Tempo padrão",
        "Standard time",
        "Tiempo estándar";
    ed_calc_ciclos:
        "Número de ciclos",
        "Number of cycles",
        "Número de ciclos";
    ed_tempos:
        "Tempos (separados por espaço ou ponto e vírgula)",
        "Times (separated by space or semicolon)",
        "Tiempos (separados por espacio o punto y coma)";
    ed_ritmo:
        "Ritmo",
        "Rating",
        "Ritmo";
    ed_calc_a:
        "a (operador com a máquina parada)",
        "a (operator with the machine stopped)",
        "a (operario con la máquina parada)";
    ed_calc_b:
        "b (operador a caminho da máquina)",
        "b (operator walking to the machine)",
        "b (operario yendo a la máquina)";
    ed_calc_t:
        "t (tempo automático)",
        "t (automatic time)",
        "t (tiempo automático)";
    ed_folgas:
        "Folgas",
        "Allowances",
        "Holguras";
    ed_erro_rel:
        "Erro relativo",
        "Relative error",
        "Error relativo";
    ed_calc_resposta:
        "resposta: {}",
        "answer: {}",
        "respuesta: {}";
    ed_pv_dados:
        "Pergunta sobre dados",
        "Question about data",
        "Pregunta sobre datos";
    ed_justificativa:
        "Perguntas de justificativa",
        "Justification questions",
        "Preguntas de justificación";
    ed_add_justificativa:
        "Adicionar pergunta",
        "Add question",
        "Agregar pregunta";
    ed_certa:
        "Opção certa",
        "Right option",
        "Opción correcta";
    ed_dicas:
        "Dicas",
        "Hints",
        "Pistas";
    ed_dica_1:
        "1. Pergunta que guia",
        "1. A guiding question",
        "1. Una pregunta que guía";
    ed_dica_2:
        "2. A ferramenta",
        "2. The tool",
        "2. La herramienta";
    ed_dica_3:
        "3. O caminho",
        "3. The path",
        "3. El camino";
    ed_add_dica:
        "Adicionar dica",
        "Add hint",
        "Agregar pista";
    ed_calibrar_sem_alavanca:
        "Uma missão só de perguntas não precisa de calibração: marque ao menos uma alavanca para calibrar.",
        "A mission of questions only needs no calibration: tick at least one lever to calibrate.",
        "Una misión solo de preguntas no necesita calibración: marque al menos una palanca para calibrar.";
    ed_dados:
        "Dados da missão",
        "Mission data",
        "Datos de la misión";
    ed_dados_dica:
        "Cada aluno recebe os próprios dados, gerados pela receita a partir da semente dele, e a resposta certa é calculada sobre esses dados.",
        "Each student gets their own data, made by the recipe from their seed, and the right answer is computed from those data.",
        "Cada alumno recibe sus propios datos, generados por la receta a partir de su semilla, y la respuesta correcta se calcula sobre esos datos.";
    ed_novo_conjunto:
        "Novo conjunto",
        "New set",
        "Nuevo conjunto";
    ed_tipo:
        "Tipo",
        "Kind",
        "Tipo";
    ed_nome_conjunto:
        "Nome",
        "Name",
        "Nombre";
    ed_ferramenta:
        "Ferramenta",
        "Tool",
        "Herramienta";
    ed_sem_ferramenta:
        "nenhuma",
        "none",
        "ninguna";
    ed_lie:
        "Limite inferior",
        "Lower limit",
        "Límite inferior";
    ed_lse:
        "Limite superior",
        "Upper limit",
        "Límite superior";
    ed_add_pergunta_dados:
        "+ pergunta",
        "+ question",
        "+ pregunta";
    ed_add_pergunta_sobre_dados:
        "Adicionar pergunta sobre dados",
        "Add question about data",
        "Agregar pregunta sobre datos";
    ed_amostra:
        "Amostra dos dados",
        "Data sample",
        "Muestra de los datos";
    ed_amostra_efeito:
        "Efeito plantado: {}",
        "Planted effect: {}",
        "Efecto plantado: {}";
    ed_amostra_mais:
        "… {} linhas",
        "… {} rows",
        "… {} filas";
    ed_amostra_invalida:
        "Corrija os problemas do conjunto para ver a amostra.",
        "Fix the problems of the set to see the sample.",
        "Corrija los problemas del conjunto para ver la muestra.";
    ed_analisar_em:
        "Analisar em",
        "Analyse in",
        "Analizar en";
    ed_app_estatistica:
        "UTSim Estatística",
        "UTSim Estatística",
        "UTSim Estatística";
    ed_app_qualidade:
        "UTSim Qualidade",
        "UTSim Qualidade",
        "UTSim Qualidade";
    ed_tg_tempos:
        "Tempos de processo",
        "Process times",
        "Tiempos de proceso";
    ed_tg_msa:
        "Estudo de R&R",
        "Gauge R&R study",
        "Estudio de R&R";
    ed_tg_media:
        "Grupos com uma média deslocada",
        "Groups with one shifted mean",
        "Grupos con una media desplazada";
    ed_tg_serie:
        "Série com um padrão",
        "Series with a pattern",
        "Serie con un patrón";
    ed_tg_fatorial:
        "Experimento fatorial",
        "Factorial experiment",
        "Experimento factorial";
    ed_tg_pareado:
        "Comparação pareada",
        "Paired comparison",
        "Comparación pareada";
    ed_tg_subgrupos:
        "Subgrupos de medidas",
        "Subgroups of measures",
        "Subgrupos de medidas";
    ed_dist:
        "Distribuição",
        "Distribution",
        "Distribución";
    ed_dist_normal:
        "Normal",
        "Normal",
        "Normal";
    ed_dist_exp:
        "Exponencial",
        "Exponential",
        "Exponencial";
    ed_dist_uniforme:
        "Uniforme",
        "Uniform",
        "Uniforme";
    ed_dist_triangular:
        "Triangular",
        "Triangular",
        "Triangular";
    ed_dist_lognormal:
        "Log-normal",
        "Log-normal",
        "Log-normal";
    ed_g_valores:
        "Valores",
        "Values",
        "Valores";
    ed_g_media:
        "Média",
        "Mean",
        "Media";
    ed_g_sd:
        "Desvio padrão",
        "Standard deviation",
        "Desviación estándar";
    ed_g_min:
        "Mínimo",
        "Minimum",
        "Mínimo";
    ed_g_moda:
        "Moda",
        "Mode",
        "Moda";
    ed_g_max:
        "Máximo",
        "Maximum",
        "Máximo";
    ed_g_pecas:
        "Peças",
        "Parts",
        "Piezas";
    ed_g_operadores:
        "Operadores",
        "Operators",
        "Operadores";
    ed_g_repeticoes:
        "Repetições",
        "Repeats",
        "Repeticiones";
    ed_g_sd_peca:
        "Desvio das peças",
        "Part deviation",
        "Desviación de las piezas";
    ed_g_sd_operador:
        "Desvio dos operadores",
        "Operator deviation",
        "Desviación de los operadores";
    ed_g_sd_repet:
        "Desvio da repetibilidade",
        "Repeatability deviation",
        "Desviación de la repetibilidad";
    ed_g_grupos:
        "Grupos",
        "Groups",
        "Grupos";
    ed_g_add_grupo:
        "Adicionar grupo",
        "Add group",
        "Agregar grupo";
    ed_g_afetado:
        "Grupo deslocado",
        "Shifted group",
        "Grupo desplazado";
    ed_g_sortear:
        "Sortear o grupo por aluno",
        "Draw the group by student",
        "Sortear el grupo por alumno";
    ed_g_por_grupo:
        "Valores por grupo",
        "Values per group",
        "Valores por grupo";
    ed_g_deslocamento:
        "Deslocamento (em desvios padrão)",
        "Shift (in standard deviations)",
        "Desplazamiento (en desviaciones estándar)";
    ed_g_fatores:
        "Fatores",
        "Factors",
        "Factores";
    ed_g_add_fator:
        "Adicionar fator",
        "Add factor",
        "Agregar factor";
    ed_g_efeito:
        "Efeito",
        "Effect",
        "Efecto";
    ed_g_replicas:
        "Réplicas",
        "Replicates",
        "Réplicas";
    ed_g_interacao:
        "Interação dos dois primeiros fatores",
        "Interaction of the first two factors",
        "Interacción de los dos primeros factores";
    ed_g_diferenca:
        "Diferença (B menos A)",
        "Difference (B minus A)",
        "Diferencia (B menos A)";
    ed_g_sd_diferenca:
        "Desvio da diferença",
        "Deviation of the difference",
        "Desviación de la diferencia";
    ed_g_subgrupos:
        "Subgrupos",
        "Subgroups",
        "Subgrupos";
    ed_g_tamanho:
        "Medidas por subgrupo",
        "Measures per subgroup",
        "Medidas por subgrupo";
    ed_g_com_padrao:
        "Com um padrão",
        "With a pattern",
        "Con un patrón";
    ed_pad:
        "Padrão",
        "Pattern",
        "Patrón";
    ed_pad_deslocamento:
        "Deslocamento",
        "Shift",
        "Desplazamiento";
    ed_pad_tendencia:
        "Tendência",
        "Trend",
        "Tendencia";
    ed_pad_inicio_ponto:
        "Começa no ponto",
        "Starts at point",
        "Empieza en el punto";
    ed_pad_inicio_subgrupo:
        "Começa no subgrupo",
        "Starts at subgroup",
        "Empieza en el subgrupo";
    ed_pad_tamanho:
        "Tamanho (em desvios padrão)",
        "Size (in standard deviations)",
        "Tamaño (en desviaciones estándar)";
    ed_pad_inclinacao:
        "Inclinação (desvios padrão por ponto)",
        "Slope (standard deviations per point)",
        "Pendiente (desviaciones estándar por punto)";
    ed_conjunto:
        "Conjunto de dados",
        "Data set",
        "Conjunto de datos";
    ed_questao:
        "Questão",
        "Question",
        "Cuestión";
    ed_q_rr_pct:
        "%R&R do estudo",
        "%R&R of the study",
        "%R&R del estudio";
    ed_q_rr_veredito:
        "Veredito do R&R",
        "Verdict of the R&R",
        "Veredicto del R&R";
    ed_q_ppk:
        "Capacidade Ppk",
        "Capability Ppk",
        "Capacidad Ppk";
    ed_q_grupo:
        "Grupo deslocado",
        "Shifted group",
        "Grupo desplazado";
    ed_q_primeiro_sinal:
        "Primeiro sinal na carta de individuais",
        "First signal on the individuals chart",
        "Primera señal en la carta de individuales";
    ed_q_efeito:
        "Efeito de um fator",
        "Effect of a factor",
        "Efecto de un factor";
    ed_q_meia_largura:
        "Meia largura do intervalo da média",
        "Half-width of the interval of the mean",
        "Semiancho del intervalo de la media";
    ed_q_replicacoes:
        "Replicações necessárias",
        "Replications needed",
        "Replicaciones necesarias";
    ed_q_dif_media:
        "Diferença média entre B e A",
        "Mean difference between B and A",
        "Diferencia media entre B y A";
    ed_q_dif_signif:
        "A diferença é significativa",
        "The difference is significant",
        "La diferencia es significativa";
    ed_q_primeiro_sub:
        "Primeiro subgrupo fora dos limites",
        "First subgroup beyond the limits",
        "Primer subgrupo fuera de los límites";
    ed_q_cpk:
        "Capacidade Cpk",
        "Capability Cpk",
        "Capacidad Cpk";
    ed_q_fonte:
        "Fonte dominante da variação",
        "Dominant source of variation",
        "Fuente dominante de la variación";
    ed_qd_rr_pct:
        "Pede o %R&R do estudo, pelo método da análise de variância.",
        "Asks for the %R&R of the study, by the analysis of variance method.",
        "Pide el %R&R del estudio, por el método del análisis de varianza.";
    ed_qd_rr_veredito:
        "Pede a classe do sistema de medição: aceitável, marginal ou inaceitável.",
        "Asks for the class of the measurement system: acceptable, marginal or unacceptable.",
        "Pide la clase del sistema de medición: aceptable, marginal o inaceptable.";
    ed_qd_ppk:
        "Pede o Ppk dos valores individuais frente aos limites de especificação.",
        "Asks for the Ppk of the individual values against the specification limits.",
        "Pide el Ppk de los valores individuales frente a los límites de especificación.";
    ed_qd_grupo:
        "O aluno aponta o grupo cuja média foi deslocada.",
        "The student points out the group whose mean was shifted.",
        "El alumno señala el grupo cuya media fue desplazada.";
    ed_qd_primeiro_sinal:
        "Pede o primeiro ponto, depois da base, que quebra uma regra de Nelson, com limites tirados dos primeiros pontos.",
        "Asks for the first point, after the base, that breaks a Nelson rule, with limits taken from the first points.",
        "Pide el primer punto, después de la base, que rompe una regla de Nelson, con límites tomados de los primeros puntos.";
    ed_qd_efeito:
        "Pede o efeito do fator: a média no nível alto menos a média no nível baixo.",
        "Asks for the effect of the factor: the mean at the high level minus the mean at the low level.",
        "Pide el efecto del factor: la media en el nivel alto menos la media en el nivel bajo.";
    ed_qd_meia_largura:
        "Pede a meia largura do intervalo de confiança da média dos valores.",
        "Asks for the half-width of the confidence interval of the mean of the values.",
        "Pide el semiancho del intervalo de confianza de la media de los valores.";
    ed_qd_replicacoes:
        "Pede quantas replicações dão a meia largura desejada, usando estes valores como piloto.",
        "Asks how many replications give the wanted half-width, using these values as the pilot.",
        "Pide cuántas replicaciones dan el semiancho deseado, usando estos valores como piloto.";
    ed_qd_dif_media:
        "Pede a média das diferenças B menos A dos pares.",
        "Asks for the mean of the B minus A differences of the pairs.",
        "Pide la media de las diferencias B menos A de los pares.";
    ed_qd_dif_signif:
        "O aluno diz se B difere de A pelo teste t pareado a 5 %.",
        "The student says whether B differs from A by the paired t test at 5 %.",
        "El alumno dice si B difiere de A por la prueba t pareada al 5 %.";
    ed_qd_primeiro_sub:
        "Pede o primeiro subgrupo, depois da base, com um ponto fora dos limites da carta de médias.",
        "Asks for the first subgroup, after the base, with a point beyond the limits of the averages chart.",
        "Pide el primer subgrupo, después de la base, con un punto fuera de los límites de la carta de medias.";
    ed_qd_cpk:
        "Pede o Cpk dos subgrupos frente aos limites de especificação.",
        "Asks for the Cpk of the subgroups against the specification limits.",
        "Pide el Cpk de los subgrupos frente a los límites de especificación.";
    ed_qd_fonte:
        "O aluno aponta a maior fonte de variação da medição: o instrumento ou os operadores.",
        "The student points out the larger source of the measurement variation: the instrument or the operators.",
        "El alumno señala la mayor fuente de variación de la medición: el instrumento o los operadores.";
    ed_qp_base_pontos:
        "Pontos da base dos limites",
        "Points of the limits base",
        "Puntos de la base de los límites";
    ed_qp_base_subgrupos:
        "Subgrupos da base dos limites",
        "Subgroups of the limits base",
        "Subgrupos de la base de los límites";
    ed_qp_fator:
        "Fator",
        "Factor",
        "Factor";
    ed_qp_confianca:
        "Confiança (%)",
        "Confidence (%)",
        "Confianza (%)";
    ed_qp_meia_largura:
        "Meia largura desejada",
        "Wanted half-width",
        "Semiancho deseado";
    ed_conferir_questao:
        "Conferir com os dados",
        "Check with the data",
        "Comprobar con los datos";
    ed_resposta_exemplo:
        "Resposta com os dados de exemplo: {}",
        "Answer with the sample data: {}",
        "Respuesta con los datos de ejemplo: {}";
    ed_sem_resposta_exemplo:
        "Os dados de exemplo não têm resposta: {}",
        "The sample data have no answer: {}",
        "Los datos de ejemplo no tienen respuesta: {}";
    ed_sem_resposta_n:
        "{} de {} alunos de teste recebem dados sem resposta",
        "{} of {} test students get data with no answer",
        "{} de {} alumnos de prueba reciben datos sin respuesta";
    ed_questao_invalida:
        "Corrija os problemas da pergunta e do conjunto para conferir.",
        "Fix the problems of the question and of the set to check.",
        "Corrija los problemas de la pregunta y del conjunto para comprobar.";
    ed_conferir_dados:
        "Conferir os dados",
        "Check the data",
        "Comprobar los datos";
    ed_conferir_dados_dica:
        "Gera dados para 30 alunos de teste e avisa onde falta resposta ou pergunta.",
        "Generates data for 30 test students and warns where an answer or a question is missing.",
        "Genera datos para 30 alumnos de prueba y avisa dónde falta respuesta o pregunta.";
    ed_g_grupo_novo:
        "Grupo {}",
        "Group {}",
        "Grupo {}";
    ed_g_fator_novo:
        "Fator {}",
        "Factor {}",
        "Factor {}";
    ed_dados_sem_avisos:
        "Os dados não têm avisos.",
        "The data have no warnings.",
        "Los datos no tienen avisos.";
}
