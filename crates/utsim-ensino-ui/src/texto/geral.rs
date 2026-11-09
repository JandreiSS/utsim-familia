//! Catalogue of the frame of the app: identity, top bar, map, packages, glossary, settings,
//! the window and the command line.

/// The name of the app (the same in every language).
pub const NOME_APP: &str = "UTSim Ensino";

textos! {
    subtitulo:
        "Ensino pela simulação",
        "Learning through simulation",
        "Enseñanza por la simulación";
    identidade_titulo:
        "Quem está jogando?",
        "Who is playing?",
        "¿Quién está jugando?";
    identidade_texto:
        "O registro acadêmico identifica as suas entregas e gera a variante dos seus dados. Use o mesmo registro da lista do professor.",
        "Your student ID identifies your submissions and generates the variant of your data. Use the same ID as on the teacher's list.",
        "El número de matrícula identifica sus entregas y genera la variante de sus datos. Use la misma matrícula de la lista del profesor.";
    prever_antes:
        "Responda às previsões antes de simular. Elas valem pontos e ficam travadas depois da primeira rodada.",
        "Answer the predictions before you simulate. They are worth points and are locked after the first run.",
        "Responda las predicciones antes de simular. Valen puntos y quedan bloqueadas después de la primera corrida.";
    sem_identidade:
        "Preencha nome e registro (letras, números, - e _).",
        "Fill in name and ID (letters, digits, - and _).",
        "Complete nombre y matrícula (letras, números, - y _).";
    medir:
        "Medir o estado atual",
        "Measure the current state",
        "Medir el estado actual";
    simular:
        "Simular",
        "Simulate",
        "Simular";
    cancelar:
        "Cancelar",
        "Cancel",
        "Cancelar";
    areia_texto:
        "Sem meta e sem orçamento. Compare dois cenários com os mesmos números aleatórios. Nada aqui conta nota.",
        "No target and no budget. Compare two scenarios with the same random numbers. Nothing here counts toward the grade.",
        "Sin meta y sin presupuesto. Compare dos escenarios con los mismos números aleatorios. Nada aquí cuenta para la nota.";
    limite_cola:
        "Sem servidor, nada impede a consulta. A entrega leva as decisões e as tentativas assinadas, e o professor simula de novo: editar o arquivo não muda a nota e deixa a alteração visível.",
        "Without a server, nothing prevents looking things up. The submission carries the decisions and the signed attempts, and the teacher simulates again: editing the file does not change the grade and leaves the change visible.",
        "Sin servidor, nada impide la consulta. La entrega lleva las decisiones y los intentos firmados, y el profesor simula de nuevo: editar el archivo no cambia la nota y deja la alteración visible.";
    prova_texto:
        "O sorteio usa a semente registrada, entre as missões do pacote aberto. O aluno recebe a prova sem referência, sem dicas e sem gabarito, com janela de horário, tempo máximo e rodadas contadas. A correção acontece aqui, com a prova reconstruída pela semente.",
        "The draw uses the recorded seed, among the missions of the open package. The student gets the exam with no reference, no hints and no answer key, with a time window, a time limit and counted runs. Grading happens here, with the exam rebuilt from the seed.",
        "El sorteo usa la semilla registrada, entre las misiones del paquete abierto. El alumno recibe la prueba sin referencia, sin pistas y sin respuestas, con ventana de horario, tiempo máximo y corridas contadas. La corrección ocurre aquí, con la prueba reconstruida a partir de la semilla.";
    desafio_texto:
        "O placar usa a última correção: a tentativa que conta de cada aluno até o prazo, pela nota e depois pelo custo. Quando os intervalos dos indicadores se sobrepõem, os alunos aparecem empatados. A turma vê só o apelido. As equipes entregam uma solução cada (a tentativa que conta entre as dos membros) e rodam na mesma fábrica, com os mesmos números aleatórios.",
        "The scoreboard uses the last grading: the attempt that counts for each student up to the deadline, by grade and then by cost. When the intervals of the indicators overlap, the students appear tied. The class sees only the nickname. Each team submits one solution (the attempt that counts among the members' attempts) and they run in the same factory, with the same random numbers.",
        "La tabla de posiciones usa la última corrección: el intento que cuenta de cada alumno hasta el plazo, por la nota y después por el costo. Cuando los intervalos de los indicadores se superponen, los alumnos aparecen empatados. El curso ve solo el apodo. Cada equipo entrega una solución (el intento que cuenta entre los de sus miembros) y corren en la misma fábrica, con los mismos números aleatorios.";
    programa_texto:
        "Se você está num programa Lean Seis Sigma da empresa, indique a pasta do programa. Cada missão resolvida passa a registrar \"praticou\" no seu mapa de competências, nas ferramentas ligadas a ela. O registro usa a sua identidade do UTSim Seis Sigma neste computador.",
        "If you are in your company's Lean Six Sigma program, point to the program folder. Each solved mission then records \"practiced\" in your competence map, in the tools tied to it. The record uses your UTSim Seis Sigma identity on this computer.",
        "Si usted está en un programa Lean Seis Sigma de la empresa, indique la carpeta del programa. Cada misión resuelta pasa a registrar \"practicó\" en su mapa de competencias, en las herramientas ligadas a ella. El registro usa su identidad de UTSim Seis Sigma en esta computadora.";
    requisitos_aviso:
        "Este pacote pede o que esta versão não tem: {}. As missões abrem, mas alguma pode não funcionar. Atualize o UTSim Ensino pelo launcher.",
        "This package asks for what this version does not have: {}. The missions open, but some may not work. Update UTSim Ensino through the launcher.",
        "Este paquete pide lo que esta versión no tiene: {}. Las misiones abren, pero alguna puede no funcionar. Actualice UTSim Ensino por el launcher.";
    outros_pacotes:
        "Outros pacotes de missões",
        "Other mission packages",
        "Otros paquetes de misiones";
    outros_pacotes_texto:
        "Também dá para abrir uma pasta ou um arquivo .utse que o professor passou. O progresso fica guardado por pacote, e a campanha continua onde parou.",
        "You can also open a folder or a .utse file the teacher gave you. Progress is kept per package, and the campaign continues where it stopped.",
        "También se puede abrir una carpeta o un archivo .utse que el profesor entregó. El progreso queda guardado por paquete, y la campaña continúa donde se detuvo.";
    pacote_nao_abriu_voltei:
        "o pacote {} não abriu ({}), voltei à campanha",
        "package {} did not open ({}), back to the campaign",
        "el paquete {} no abrió ({}), volví a la campaña";
    config_nao_salva:
        "não foi possível salvar a configuração: {}",
        "could not save the settings: {}",
        "no fue posible guardar la configuración: {}";
    ok:
        "ok",
        "ok",
        "ok";
    aba_campanha:
        "Campanha",
        "Campaign",
        "Campaña";
    aba_glossario:
        "Glossário",
        "Glossary",
        "Glosario";
    aba_turma:
        "Turma",
        "Class",
        "Curso";
    aba_professor:
        "Professor",
        "Teacher",
        "Profesor";
    aba_ajustes:
        "Ajustes",
        "Settings",
        "Ajustes";
    nome:
        "Nome",
        "Name",
        "Nombre";
    registro_academico:
        "Registro acadêmico",
        "Student ID",
        "Matrícula";
    apelido_opcional:
        "Apelido no placar (opcional)",
        "Nickname on the scoreboard (optional)",
        "Apodo en la tabla de posiciones (opcional)";
    comecar:
        "Começar",
        "Start",
        "Comenzar";
    campanha_nao_carregou:
        "A campanha não carregou:",
        "The campaign did not load:",
        "La campaña no cargó:";
    ato_titulo:
        "Ato {}. {}",
        "Act {}. {}",
        "Acto {}. {}";
    bloqueada_ato:
        "bloqueada: resolva o ato anterior",
        "locked: solve the previous act",
        "bloqueada: resuelva el acto anterior";
    bloqueada_anterior:
        "bloqueada: resolva a anterior",
        "locked: solve the previous one",
        "bloqueada: resuelva la anterior";
    melhor_nota:
        "melhor nota {} em {} tentativa(s)",
        "best grade {} in {} attempt(s)",
        "mejor nota {} en {} intento(s)";
    caixa_de_areia:
        "Caixa de areia",
        "Sandbox",
        "Caja de arena";
    pacotes:
        "Pacotes:",
        "Packages:",
        "Paquetes:";
    pacote_nao_abriu:
        "o pacote não abriu: {}",
        "the package did not open: {}",
        "el paquete no abrió: {}";
    abrir_pacote:
        "Abrir pacote",
        "Open package",
        "Abrir paquete";
    voltar_campanha:
        "Voltar à campanha",
        "Back to the campaign",
        "Volver a la campaña";
    glossario_texto:
        "Cada conceito aparece quando uma missão o usa. O texto completo está no manual.",
        "Each concept appears when a mission uses it. The full text is in the manual.",
        "Cada concepto aparece cuando una misión lo usa. El texto completo está en el manual.";
    glossario_missoes:
        "Missões: {}. ",
        "Missions: {}. ",
        "Misiones: {}. ";
    glossario_manual:
        "Manual: {}",
        "Manual: {}",
        "Manual: {}";
    tamanho_letra:
        "Tamanho da letra",
        "Text size",
        "Tamaño de la letra";
    idioma:
        "Idioma",
        "Language",
        "Idioma";
    idioma_texto:
        "Muda os textos do programa. As missões aparecem na língua do pacote.",
        "Changes the texts of the program. Missions appear in the language of the package.",
        "Cambia los textos del programa. Las misiones aparecen en el idioma del paquete.";
    pacote_em_outra_lingua:
        "As missões deste pacote estão em {}.",
        "The missions of this package are in {}.",
        "Las misiones de este paquete están en {}.";
    modo_professor:
        "Modo professor (turmas, correção em lote, matriz da turma)",
        "Teacher mode (classes, batch grading, class matrix)",
        "Modo profesor (cursos, corrección en lote, matriz del curso)";
    chave_professor_falhou:
        "não foi possível criar a chave do professor: {}",
        "could not create the teacher's key: {}",
        "no fue posible crear la clave del profesor: {}";
    pasta_perfil:
        "Pasta do perfil: {}",
        "Profile folder: {}",
        "Carpeta del perfil: {}";
    trocar_aluno:
        "Trocar de aluno",
        "Change student",
        "Cambiar de alumno";
    nucleo_versao:
        "Núcleo {}",
        "Core {}",
        "Núcleo {}";
    pacote_versao:
        "Pacote {} {} ({})",
        "Package {} {} ({})",
        "Paquete {} {} ({})";
    prova_linha:
        "Prova de {} a {}, {} rodada(s) por missão",
        "Exam from {} to {}, {} run(s) per mission",
        "Prueba de {} a {}, {} corrida(s) por misión";
    prova_tempo:
        ", {} min a partir da primeira missão aberta",
        ", {} min from the first mission opened",
        ", {} min a partir de la primera misión abierta";
    prova_restam:
        ". Restam {} min.",
        ". {} min left.",
        ". Quedan {} min.";
    prova_nao_da:
        ". Agora não dá para simular: {}.",
        ". You cannot simulate now: {}.",
        ". Ahora no se puede simular: {}.";
    prova_sem_dicas:
        " Sem dicas, sem nota na tela e sem caixa de areia: o professor corrige.",
        " No hints, no grade on screen and no sandbox: the teacher grades.",
        " Sin pistas, sin nota en pantalla y sin caja de arena: el profesor corrige.";
    erro:
        "erro: {}",
        "error: {}",
        "error: {}";
    sem_pasta_usuario:
        "não encontrei a pasta do usuário",
        "could not find the user folder",
        "no encontré la carpeta del usuario";
    sem_perfil:
        "sem perfil",
        "no profile",
        "sin perfil";
    voltar_3d:
        "← Voltar ao UTSim 3D",
        "← Back to UTSim 3D",
        "← Volver a UTSim 3D";
    erro_janela:
        "erro na janela: {}",
        "window error: {}",
        "error en la ventana: {}";
    faltam_argumentos:
        "faltam argumentos, veja utsim-ensino --ajuda",
        "missing arguments, see utsim-ensino --help",
        "faltan argumentos, vea utsim-ensino --ajuda";
    missao_nova:
        "Missão nova",
        "New mission",
        "Misión nueva";
    pacote_gravado:
        "pacote gravado em {}",
        "package written to {}",
        "paquete guardado en {}";
    ajuda_cli:
        "utsim-ensino [--pacote <pasta|pacote.utse>] [--missao <id>] [--idioma pt-BR|en|es]\n  abre a janela do UTSim Ensino\n  --pacote: abre esse pacote de missões no lugar da campanha\n  --missao ato4/ventilador: abre essa missão direto (demonstração em aula)\n  --idioma: língua dos textos do programa (também em Ajustes)\n  UTSIM_ENSINO_PERFIL=<pasta>: guarda o perfil nessa pasta (computadores de laboratório)\n\nConstrutor de casos (o resultado também fica em <pasta>/conferencia.txt):\n  utsim-ensino caso <modelo.ron> <pasta> [título]: missão de rascunho a partir de um modelo\n  utsim-ensino conferir <pasta|pacote.utse>: confere as missões de um pacote\n  utsim-ensino pacote <pasta> <pacote.utse>: empacota a pasta",
        "utsim-ensino [--pacote <folder|package.utse>] [--missao <id>] [--idioma pt-BR|en|es]\n  opens the UTSim Ensino window\n  --pacote: opens this mission package instead of the campaign\n  --missao ato4/ventilador: opens this mission directly (classroom demonstration)\n  --idioma: language of the program texts (also in Settings)\n  UTSIM_ENSINO_PERFIL=<folder>: keeps the profile in this folder (lab computers)\n\nCase builder (the result also goes to <folder>/conferencia.txt):\n  utsim-ensino caso <model.ron> <folder> [title]: draft mission from a model\n  utsim-ensino conferir <folder|package.utse>: checks the missions of a package\n  utsim-ensino pacote <folder> <package.utse>: zips the folder",
        "utsim-ensino [--pacote <carpeta|paquete.utse>] [--missao <id>] [--idioma pt-BR|en|es]\n  abre la ventana de UTSim Ensino\n  --pacote: abre ese paquete de misiones en lugar de la campaña\n  --missao ato4/ventilador: abre esa misión directamente (demostración en clase)\n  --idioma: idioma de los textos del programa (también en Ajustes)\n  UTSIM_ENSINO_PERFIL=<carpeta>: guarda el perfil en esa carpeta (computadoras de laboratorio)\n\nConstructor de casos (el resultado también queda en <carpeta>/conferencia.txt):\n  utsim-ensino caso <modelo.ron> <carpeta> [título]: misión de borrador a partir de un modelo\n  utsim-ensino conferir <carpeta|paquete.utse>: verifica las misiones de un paquete\n  utsim-ensino pacote <carpeta> <paquete.utse>: empaqueta la carpeta";
}
