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
        "El marcador usa la última corrección: el intento que cuenta de cada alumno hasta el plazo, por la nota y después por el costo. Cuando los intervalos de los indicadores se superponen, los alumnos aparecen empatados. El grupo ve solo el apodo. Cada equipo entrega una solución (el intento que cuenta entre los de sus miembros) y corren en la misma fábrica, con los mismos números aleatorios.";
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
}
