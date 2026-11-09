//! Catalogue of the 2D view and the parts screens (vista2d.rs, pecas.rs). Entries start with `vista_`.

textos! {
    // The 2D view (vista2d.rs).
    vista_continuar:
        "▶ Continuar",
        "▶ Resume",
        "▶ Continuar";
    vista_pausar:
        "⏸ Pausar",
        "⏸ Pause",
        "⏸ Pausar";
    vista_do_inicio:
        "⟲ Do início",
        "⟲ From the start",
        "⟲ Desde el inicio";
    vista_velocidade:
        "Velocidade:",
        "Speed:",
        "Velocidad:";
    vista_tempo:
        "t = {} {} de {}",
        "t = {} {} of {}",
        "t = {} {} de {}";
    vista_trabalhando:
        "trabalhando",
        "working",
        "trabajando";
    vista_ociosa:
        "ociosa",
        "idle",
        "ociosa";
    vista_bloqueada:
        "bloqueada",
        "blocked",
        "bloqueada";
    vista_parada_ou_quebrada:
        "parada ou quebrada",
        "stopped or broken down",
        "parada o averiada";
    vista_em_setup:
        "em setup",
        "in setup",
        "en preparación";
    vista_setup:
        "setup",
        "setup",
        "preparación";
    vista_parada:
        "parada",
        "stopped",
        "parada";
    vista_fora_do_turno:
        "fora do turno",
        "off shift",
        "fuera de turno";
    vista_sem_operador:
        "sem operador",
        "no operator",
        "sin operario";
    vista_vazia:
        "vazia",
        "empty",
        "vacía";
    vista_fila:
        "fila",
        "queue",
        "cola";
    vista_chegadas:
        "chegadas",
        "arrivals",
        "llegadas";
    vista_saida:
        "saída",
        "exit",
        "salida";

    // Line balancing (pecas.rs).
    vista_balanc_resumo:
        "Takt {} s · soma dos tempos {} s · estações usadas {} · eficiência {}",
        "Takt {} s · sum of task times {} s · workstations used {} · efficiency {}",
        "Takt {} s · suma de los tiempos {} s · estaciones usadas {} · eficiencia {}";
    vista_balanc_ajuda:
        "Arraste cada tarefa para uma estação, ou clique na tarefa e depois na estação.",
        "Drag each task to a workstation, or click the task and then the workstation.",
        "Arrastre cada tarea a una estación, o haga clic en la tarea y luego en la estación.";
    vista_estacao_n:
        "Estação {}",
        "Workstation {}",
        "Estación {}";
    vista_carga_de_takt:
        "{} de {} s",
        "{} of {} s",
        "{} de {} s";
    vista_tarefa_tempo:
        "{} · {} s",
        "{} · {} s",
        "{} · {} s";
    vista_depois_de_lista:
        " (depois de {})",
        " (after {})",
        " (después de {})";
    vista_tarefas_precedencias:
        "Tarefas e precedências",
        "Tasks and precedences",
        "Tareas y precedencias";
    vista_col_tarefa:
        "Tarefa",
        "Task",
        "Tarea";
    vista_col_descricao:
        "Descrição",
        "Description",
        "Descripción";
    vista_col_tempo:
        "Tempo (s)",
        "Time (s)",
        "Tiempo (s)";
    vista_col_depois_de:
        "Depois de",
        "After",
        "Después de";
    vista_col_estacao:
        "Estação",
        "Workstation",
        "Estación";

    // Layout grid (pecas.rs).
    vista_arranjo_resumo:
        "Momento de transporte: {} (viagens por dia × metros) · células de {} m",
        "Transport moment: {} (trips per day × metres) · cells of {} m",
        "Momento de transporte: {} (viajes por día × metros) · celdas de {} m";
    vista_arranjo_ajuda:
        "Arraste cada máquina para uma célula, ou clique na máquina e depois na célula. A doca é fixa.",
        "Drag each machine to a cell, or click the machine and then the cell. The dock is fixed.",
        "Arrastre cada máquina a una celda, o haga clic en la máquina y luego en la celda. El muelle es fijo.";
    vista_celula_n:
        "célula {}",
        "cell {}",
        "celda {}";
    vista_matriz_de_para:
        "Matriz de-para (viagens por dia e momento)",
        "From-to matrix (trips per day and moment)",
        "Matriz desde-hacia (viajes por día y momento)";
    vista_col_de:
        "De",
        "From",
        "Desde";
    vista_col_para:
        "Para",
        "To",
        "Hacia";
    vista_col_viagens:
        "Viagens",
        "Trips",
        "Viajes";
    vista_col_distancia:
        "Distância (m)",
        "Distance (m)",
        "Distancia (m)";
    vista_col_momento:
        "Momento",
        "Moment",
        "Momento";
}
