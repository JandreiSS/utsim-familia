# Montagem do ventilador

A Vale Azul vai montar ventiladores de mesa numa linha nova. O cliente pede 480 ventiladores por turno de 8 horas, ou seja, um a cada 60 segundos. Esse é o **takt** da linha.

A montagem tem 14 tarefas. Cada uma tem um tempo e algumas só podem ser feitas depois de outras (a hélice só entra depois da grade traseira, a embalagem é a última de todas). A lista com os tempos e as precedências está no quadro de tarefas.

Hoje as tarefas estão espalhadas por 10 estações, quase todas com folga.

## O que fazer

1. Preveja o número mínimo teórico de estações.
2. Arraste cada tarefa para uma estação. Nenhuma estação pode passar de 60 s, e nenhuma tarefa pode ficar antes de uma tarefa que precisa vir primeiro.
3. Simule. A meta é uma eficiência de balanceamento de pelo menos 0,77, com a linha entregando um ventilador por minuto.

Nem sempre o mínimo teórico é possível: a precedência e o tamanho das tarefas podem obrigar a usar uma estação a mais.
