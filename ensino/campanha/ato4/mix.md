# Mudou o mix

A fábrica lançou o ventilador de coluna, e a mesma linha agora monta os dois modelos: metade da demanda é de mesa e metade de coluna, um ventilador a cada 60 segundos no total.

As tarefas são as mesmas, mas os tempos mudam com o modelo. O de coluna leva mais tempo na haste, no cabo, no teste e na embalagem. O de mesa leva mais nos botões e na carcaça do motor, que tem o mecanismo de oscilação. O tempo de cada tarefa no quadro é a média dos dois modelos, e é com ela que se calcula a eficiência do balanceamento.

Hoje a linha usa o balanceamento antigo, de um modelo só, e monta em lotes de 40 de cada modelo. Entre as estações cabem duas peças. Cada troca de modelo obriga a logística a trocar o kit de peças ao lado da linha, por isso ela prefere lotes grandes.

## O que fazer

1. Preveja se montar em lotes entrega tanto quanto alternar os modelos.
2. Atribua as tarefas às estações, olhando a carga de cada modelo em cada estação.
3. Escolha a sequência de entrada na linha.
4. Simule. A meta é entregar pelo menos 29,5 ventiladores por hora de cada modelo. A nota premia o menor número de estações, a menor carga de um modelo numa estação e o maior lote.

Uma estação pode passar do takt num modelo se ficar abaixo dele no outro. Isso só funciona se os modelos se revezarem com frequência: num lote longo do modelo pesado, a estação passa do takt várias vezes seguidas e o pulmão de duas peças não dá conta.
