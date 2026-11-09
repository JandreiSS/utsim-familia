//! Catalogue of the class screens, student and teacher (turma.rs). Entries start with `turma_`.

textos! {
    // The student's class screen.
    turma_titulo:
        "Turma",
        "Class",
        "Curso";
    turma_voce_esta:
        "Você está na turma {} {} ({})",
        "You are in the class {} {} ({})",
        "Usted está en el curso {} {} ({})";
    turma_pasta_da_turma:
        "Pasta da turma: {}",
        "Class folder: {}",
        "Carpeta del curso: {}";
    turma_gravada_na_pasta:
        "Cada tentativa é gravada nessa pasta assim que termina. O Google Drive para computador sincroniza.",
        "Each attempt is saved in this folder as soon as it ends. Google Drive for desktop keeps it in sync.",
        "Cada intento se guarda en esa carpeta en cuanto termina. Google Drive para computadora lo sincroniza.";
    turma_copiar_de_novo:
        "Copiar de novo todas as tentativas para a pasta",
        "Copy all attempts to the folder again",
        "Copiar de nuevo todos los intentos a la carpeta";
    turma_tentativas_na_pasta:
        "{} tentativa(s) na pasta da turma",
        "{} attempt(s) in the class folder",
        "{} intento(s) en la carpeta del curso";
    turma_erro:
        "erro: {}",
        "error: {}",
        "error: {}";
    turma_por_arquivo:
        "Esta turma recebe as entregas por arquivo.",
        "This class receives submissions by file.",
        "Este curso recibe las entregas por archivo.";
    turma_sair:
        "Sair da turma",
        "Leave the class",
        "Salir del curso";
    turma_como_entrar:
        "Para entrar numa turma, abra o convite (.utsc) que o professor enviou. Se a turma usa uma pasta compartilhada no Drive, indique também a pasta sincronizada.",
        "To join a class, open the invitation (.utsc) your teacher sent. If the class uses a shared Drive folder, also point to the synced folder.",
        "Para entrar en un curso, abra la invitación (.utsc) que le envió el profesor. Si el curso usa una carpeta compartida en Drive, indique también la carpeta sincronizada.";
    turma_convite:
        "Convite (.utsc)",
        "Invitation (.utsc)",
        "Invitación (.utsc)";
    turma_pasta_opcional:
        "Pasta da turma (opcional)",
        "Class folder (optional)",
        "Carpeta del curso (opcional)";
    turma_entrar:
        "Entrar na turma",
        "Join the class",
        "Entrar en el curso";
    turma_nao_gravou_pasta:
        "não foi possível gravar na pasta: {}",
        "could not write to the folder: {}",
        "no fue posible guardar en la carpeta: {}";
    turma_convite_invalido:
        "convite inválido: {}",
        "invalid invitation: {}",
        "invitación no válida: {}";
    turma_entrega_titulo:
        "Entrega por arquivo",
        "Submission by file",
        "Entrega por archivo";
    turma_entrega_texto:
        "Grava todas as suas tentativas assinadas num arquivo .utsr para enviar ao professor (ambiente da disciplina, e-mail ou pen drive).",
        "Saves all your signed attempts in a .utsr file to send to the teacher (course site, e-mail or USB drive).",
        "Guarda todos sus intentos firmados en un archivo .utsr para enviar al profesor (aula virtual, correo electrónico o memoria USB).";
    turma_arquivo:
        "Arquivo",
        "File",
        "Archivo";
    turma_gravar_entrega:
        "Gravar entrega",
        "Save submission",
        "Guardar entrega";
    turma_entrega_gravada:
        "entrega com {} tentativa(s) gravada em {}",
        "submission with {} attempt(s) saved in {}",
        "entrega con {} intento(s) guardada en {}";
    turma_entrega_falhou:
        "não foi possível gravar a entrega: {}",
        "could not save the submission: {}",
        "no fue posible guardar la entrega: {}";
    turma_placares:
        "Placares da turma",
        "Class scoreboards",
        "Tablas de posiciones del curso";
    turma_desafio_de:
        "Desafio: {}",
        "Challenge: {}",
        "Desafío: {}";
    turma_posicao:
        "Posição",
        "Rank",
        "Posición";

    // The teacher's panel.
    turma_professor:
        "Professor",
        "Teacher",
        "Profesor";
    turma_construtor:
        "Construtor de casos",
        "Case builder",
        "Constructor de casos";
    turma_construtor_texto:
        "Gera uma missão de rascunho a partir de um modelo do UTSim: alavancas sugeridas (capacidades, filas limitadas, equipes), medidas de cada saída, referência pela busca exaustiva e metas a 5 % dela. Revise o texto, as faixas, os custos e as metas na pasta e confira de novo.",
        "Generates a draft mission from a UTSim model: suggested levers (capacities, bounded queues, crews), measures of each output, a reference from exhaustive search and targets within 5 % of it. Review the text, ranges, costs and targets in the folder and check again.",
        "Genera una misión borrador a partir de un modelo de UTSim: palancas sugeridas (capacidades, colas limitadas, cuadrillas), medidas de cada salida, referencia por búsqueda exhaustiva y metas a 5 % de ella. Revise el texto, los rangos, los costos y las metas en la carpeta y verifique de nuevo.";
    turma_modelo:
        "Modelo (.ron)",
        "Model (.ron)",
        "Modelo (.ron)";
    turma_titulo_missao:
        "Título da missão",
        "Mission title",
        "Título de la misión";
    turma_pasta_pacote:
        "Pasta do pacote",
        "Package folder",
        "Carpeta del paquete";
    turma_criar_calibrar:
        "Criar e calibrar",
        "Create and calibrate",
        "Crear y calibrar";
    turma_conferir_de_novo:
        "Conferir de novo",
        "Check again",
        "Verificar de nuevo";
    turma_gravar_utse:
        "Gravar .utse",
        "Save .utse",
        "Guardar .utse";
    turma_pacote_gravado:
        "pacote gravado em {}",
        "package saved in {}",
        "paquete guardado en {}";
    turma_missao_nova:
        "Missão nova",
        "New mission",
        "Misión nueva";
    turma_nova:
        "Nova turma",
        "New class",
        "Curso nuevo";
    turma_pasta_local_drive:
        "Pasta da turma (local ou no Drive)",
        "Class folder (local or on Drive)",
        "Carpeta del curso (local o en Drive)";
    turma_nome:
        "Nome",
        "Name",
        "Nombre";
    turma_periodo:
        "Período",
        "Term",
        "Período";
    turma_finalidade:
        "Finalidade dos dados (LGPD)",
        "Purpose of the data (LGPD)",
        "Finalidad de los datos (LGPD)";
    turma_lista_csv:
        "Lista de alunos (.csv: nome;registro;apelido)",
        "Student list (.csv with columns nome, registro, apelido)",
        "Lista de estudiantes (.csv con columnas nome, registro, apelido)";
    turma_criar:
        "Criar turma",
        "Create class",
        "Crear curso";
    turma_informe_pasta_nome:
        "informe a pasta e o nome",
        "enter the folder and the name",
        "indique la carpeta y el nombre";
    turma_criada:
        "turma criada em {}; envie o convite.utsc aos alunos",
        "class created in {}. Send convite.utsc to the students",
        "curso creado en {}. Envíe convite.utsc a los estudiantes";
    turma_nao_criou:
        "não foi possível criar a turma: {}",
        "could not create the class: {}",
        "no fue posible crear el curso: {}";
    turma_abrir_turma:
        "Abrir turma:",
        "Open class:",
        "Abrir curso:";
    turma_abrir:
        "Abrir",
        "Open",
        "Abrir";
    turma_nao_abriu:
        "não foi possível abrir: {}",
        "could not open: {}",
        "no fue posible abrir: {}";
    turma_recentes:
        "Recentes:",
        "Recent:",
        "Recientes:";
    turma_cabecalho:
        "{} {} · {} aluno(s)",
        "{} {} · {} student(s)",
        "{} {} · {} estudiante(s)";
    turma_pasta_convite:
        "Pasta: {} · convite: {}",
        "Folder: {} · invitation: {}",
        "Carpeta: {} · invitación: {}";
    turma_importar_lista:
        "Importar lista de alunos (.csv):",
        "Import student list (.csv):",
        "Importar lista de estudiantes (.csv):";
    turma_importar:
        "Importar",
        "Import",
        "Importar";
    turma_importados:
        "{} aluno(s) importado(s)",
        "{} student(s) imported",
        "{} estudiante(s) importado(s)";
    turma_lista_recusada:
        "lista recusada: {}",
        "list rejected: {}",
        "lista rechazada: {}";
    turma_registrar_ultima:
        "Registrar a última tentativa (desmarcado: a melhor)",
        "Record the last attempt (unchecked: the best one)",
        "Registrar el último intento (sin marcar: el mejor)";
    turma_entregas_utsr:
        "Entregas por arquivo (pasta com .utsr, opcional):",
        "Submissions by file (folder with .utsr, optional):",
        "Entregas por archivo (carpeta con .utsr, opcional):";
    turma_corrigir_todas:
        "Corrigir todas as entregas",
        "Grade all submissions",
        "Calificar todas las entregas";
    turma_prova_nao_encontrada:
        "prova {} não encontrada",
        "exam {} not found",
        "examen {} no encontrado";
    turma_nao_leu_pacote:
        "não foi possível ler o pacote aberto",
        "could not read the open package",
        "no fue posible leer el paquete abierto";
    turma_nao_corrigiu:
        "não foi possível corrigir: {}",
        "could not grade: {}",
        "no fue posible calificar: {}";
    turma_matriz:
        "Matriz aluno por missão",
        "Student by mission matrix",
        "Matriz estudiante por misión";
    turma_aluno:
        "Aluno",
        "Student",
        "Estudiante";
    turma_media:
        "Média",
        "Mean",
        "Media";
    turma_avisos:
        "Avisos",
        "Warnings",
        "Avisos";
    turma_celula_dica:
        "{} tentativa(s), dica até o nível {}",
        "{} attempt(s), hints up to level {}",
        "{} intento(s), pistas hasta el nivel {}";
    turma_pares:
        "Pares para olhar de perto",
        "Pairs to look at closely",
        "Pares para revisar de cerca";
    turma_semelhanca:
        "Semelhança não é prova de cópia: a decisão é sua.",
        "Similarity is not proof of copying. The decision is yours.",
        "La semejanza no es prueba de copia. La decisión es suya.";
    turma_par:
        "• {} e {} em {}: {}",
        "• {} and {} in {}: {}",
        "• {} y {} en {}: {}";
    turma_erros_comuns:
        "Erros mais comuns",
        "Most common mistakes",
        "Errores más comunes";
    turma_nenhum_erro:
        "Nenhum erro frequente nas entregas corrigidas.",
        "No frequent mistakes in the graded submissions.",
        "Ningún error frecuente en las entregas calificadas.";
    turma_exportar:
        "Exportar notas (CSV e .xlsx em correcao/)",
        "Export grades (CSV and .xlsx in correcao/)",
        "Exportar notas (CSV y .xlsx en correcao/)";
    turma_planilha_notas:
        "Notas",
        "Grades",
        "Notas";
    turma_notas_exportadas:
        "notas exportadas em {}",
        "grades exported to {}",
        "notas exportadas en {}";
    turma_nao_exportou:
        "não foi possível exportar: {}",
        "could not export: {}",
        "no fue posible exportar: {}";

    // Challenge and teams.
    turma_desafio_equipes:
        "Desafio da turma e equipes",
        "Class challenge and teams",
        "Desafío del curso y equipos";
    turma_missao:
        "Missão",
        "Mission",
        "Misión";
    turma_prazo:
        "Prazo (AAAA-MM-DD HH:MM, Brasília, opcional)",
        "Deadline (YYYY-MM-DD HH:MM, Brasília time, optional)",
        "Plazo (AAAA-MM-DD HH:MM, hora de Brasilia, opcional)";
    turma_montar_placar:
        "Montar o placar",
        "Build the scoreboard",
        "Armar la tabla de posiciones";
    turma_prazo_formato:
        "prazo no formato AAAA-MM-DD HH:MM",
        "deadline in the format YYYY-MM-DD HH:MM",
        "plazo en el formato AAAA-MM-DD HH:MM";
    turma_publicar_placar:
        "Publicar o placar na pasta da turma",
        "Publish the scoreboard in the class folder",
        "Publicar la tabla de posiciones en la carpeta del curso";
    turma_placar_publicado:
        "placar publicado em {} (só apelidos)",
        "scoreboard published in {} (nicknames only)",
        "tabla de posiciones publicada en {} (solo apodos)";
    turma_nao_publicou:
        "não foi possível publicar: {}",
        "could not publish: {}",
        "no fue posible publicar: {}";
    turma_equipes_texto:
        "Equipes, uma por linha (Equipe: registro, registro):",
        "Teams, one per line (Team: student ID, student ID):",
        "Equipos, uno por línea (Equipo: matrícula, matrícula):";
    turma_comparar_equipes:
        "Comparar as equipes nesta missão",
        "Compare the teams in this mission",
        "Comparar los equipos en esta misión";
    turma_duas_equipes:
        "é preciso ao menos duas equipes com tentativas corrigidas",
        "at least two teams with graded attempts are needed",
        "se necesitan al menos dos equipos con intentos calificados";
    turma_sem_tentativa:
        "{}: sem tentativa corrigida nesta missão",
        "{}: no graded attempt in this mission",
        "{}: sin intento calificado en esta misión";
    turma_apelido:
        "Apelido",
        "Nickname",
        "Apodo";
    turma_nota:
        "Nota",
        "Grade",
        "Nota";
    turma_custo:
        "Custo",
        "Cost",
        "Costo";
    turma_empate:
        "= empate estatístico: os intervalos se sobrepõem, a diferença pode ser ruído.",
        "= statistical tie: the intervals overlap, so the difference may be noise.",
        "= empate estadístico: los intervalos se superponen, la diferencia puede ser ruido.";
    turma_comparacao_titulo:
        "{} ({}), mesma fábrica e mesmos números aleatórios para todas as equipes",
        "{} ({}), same plant and same random numbers for every team",
        "{} ({}), misma planta y mismos números aleatorios para todos los equipos";
    turma_equipe:
        "Equipe",
        "Team",
        "Equipo";
    turma_menos:
        "menos {}",
        "minus {}",
        "menos {}";
    turma_a_frente:
        "Uma equipe só fica à frente de outra quando o intervalo da diferença pareada não contém o zero.",
        "A team ranks ahead of another only when the interval of the paired difference does not contain zero.",
        "Un equipo queda por delante de otro solo cuando el intervalo de la diferencia pareada no contiene el cero.";

    // The exam.
    turma_prova:
        "Prova",
        "Exam",
        "Examen";
    turma_id_prova:
        "Id da prova",
        "Exam ID",
        "Id del examen";
    turma_titulo_campo:
        "Título",
        "Title",
        "Título";
    turma_inicio:
        "Início (AAAA-MM-DD HH:MM, Brasília)",
        "Start (YYYY-MM-DD HH:MM, Brasília time)",
        "Inicio (AAAA-MM-DD HH:MM, hora de Brasilia)";
    turma_fim:
        "Fim",
        "End",
        "Fin";
    turma_tempo_maximo:
        "Tempo máximo (min, 0 para só a janela)",
        "Time limit (min, 0 for the window only)",
        "Tiempo máximo (min, 0 para solo la ventana)";
    turma_rodadas:
        "Rodadas por missão",
        "Runs per mission",
        "Corridas por misión";
    turma_semente:
        "Semente (vazia: sorteada agora)",
        "Seed (empty: drawn now)",
        "Semilla (vacía: sorteada ahora)";
    turma_por_ato:
        "Missões sorteadas por ato (as de entregar modelo ficam de fora):",
        "Missions drawn per act (the ones that ask for a model are left out):",
        "Misiones sorteadas por acto (las de entregar modelo quedan fuera):";
    turma_ato:
        "Ato {}",
        "Act {}",
        "Acto {}";
    turma_sortear:
        "Sortear e publicar na pasta da turma",
        "Draw and publish in the class folder",
        "Sortear y publicar en la carpeta del curso";
    turma_prova_publicada:
        "prova {} publicada em {} (semente {}). A montagem fica só no seu perfil.",
        "exam {} published in {} (seed {}). The assembly stays only in your profile.",
        "examen {} publicado en {} (semilla {}). El armado queda solo en su perfil.";
    turma_prova_nao_publicada:
        "prova não publicada: {}",
        "exam not published: {}",
        "examen no publicado: {}";
    turma_correcao_usa:
        "A correção usa:",
        "Grading uses:",
        "La calificación usa:";
    turma_pacote_aberto:
        "o pacote aberto",
        "the open package",
        "el paquete abierto";
    turma_a_prova:
        "a prova {}",
        "the exam {}",
        "el examen {}";
    turma_inicio_formato:
        "início no formato AAAA-MM-DD HH:MM",
        "start in the format YYYY-MM-DD HH:MM",
        "inicio en el formato AAAA-MM-DD HH:MM";
    turma_fim_formato:
        "fim no formato AAAA-MM-DD HH:MM",
        "end in the format YYYY-MM-DD HH:MM",
        "fin en el formato AAAA-MM-DD HH:MM";
    turma_semente_inteiro:
        "a semente é um número inteiro",
        "the seed is a whole number",
        "la semilla es un número entero";

    // The company's Lean Six Sigma program.
    turma_programa_titulo:
        "Programa Lean Seis Sigma da empresa",
        "The company's Lean Six Sigma program",
        "Programa Lean Seis Sigma de la empresa";
    turma_pasta_programa:
        "Pasta do programa",
        "Program folder",
        "Carpeta del programa";
    turma_ligar:
        "Ligar",
        "Connect",
        "Conectar";
    turma_programa_ligado:
        "programa ligado para {}",
        "program connected for {}",
        "programa conectado para {}";
    turma_nao_ligou:
        "não foi possível ligar: {}",
        "could not connect: {}",
        "no fue posible conectar: {}";
    turma_ligado_a:
        "Ligado a {}",
        "Connected to {}",
        "Conectado a {}";
    turma_registrar_missoes:
        "Registrar as missões já resolvidas",
        "Record the missions already solved",
        "Registrar las misiones ya resueltas";
    turma_nenhum_pacote:
        "nenhum pacote aberto",
        "no package open",
        "ningún paquete abierto";
    turma_competencias:
        "{} competência(s) enviadas ao programa (as que já estavam lá não se repetem)",
        "{} competence(s) sent to the program (the ones already there are not repeated)",
        "{} competencia(s) enviadas al programa (las que ya estaban no se repiten)";
    turma_algumas_nao:
        "algumas não foram: {}",
        "some were not sent: {}",
        "algunas no se enviaron: {}";
    turma_nao_registrou:
        "não foi possível registrar: {}",
        "could not record: {}",
        "no fue posible registrar: {}";
    turma_desligar:
        "Desligar",
        "Disconnect",
        "Desconectar";
}
