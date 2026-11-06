# Linha com inspeção e retrabalho

A Vale Verde Eletrônica monta controladores de irrigação e precisa saber se a linha entrega 850 controladores aprovados por dia. O modelo inicial já tem a montagem do kit, a solda e a calibração, com os turnos da fábrica. Falta o resto, que você vai modelar no UTSim.

O que falta:

- **Quebras da solda.** A solda trabalha, em média, 7.220,7 segundos entre quebras (exponencial) e cada conserto leva, em média, 1.612 segundos (exponencial).
- **Inspeção.** Depois da calibração, uma fila de 2 lugares e a inspeção com 3 bancadas em paralelo. Cada inspeção leva um tempo Gama de forma 4,117 e escala 24,787 segundos e ocupa um inspetor durante todo o tempo. A equipe tem 3 inspetores, que andam a 1,3 m/s e seguem os turnos da fábrica.
- **Retrabalho.** 12 % das peças inspecionadas são reprovadas e vão para uma fila sem limite antes do posto de retrabalho (triangular de 150, 240 e 420 segundos). Depois voltam para uma fila sem limite antes da calibração, que tem prioridade sobre as peças novas.

Use estes nomes de objeto, que a correção procura: **solda**, **inspecao**, **retrabalho** e **expedicao** (as aprovadas).

## O que fazer

1. Diga se a linha atual chega a 850 aprovados por dia.
2. Grave o modelo inicial, abra no UTSim e complete o modelo.
3. Indique o arquivo .ron e simule. O app confere a estrutura e compara as medidas do seu modelo com as do modelo de referência.

Este é o estudo de caso 1 do manual. Os dados brutos e a análise completa estão lá.
