# Requisitos do UTSim

Lista do que o UTSim **faz** (requisitos funcionais, RF) e de **como** ele precisa ser
(requisitos não funcionais, RNF). Cada item tem um código fixo, a fase em que entra,
a situação atual e onde está a prova de que funciona.

**Situação:** ✅ pronto e testado · 🔄 em andamento · ⏳ planejado

> Regra: toda funcionalidade nova atualiza esta página no mesmo commit (ver
> [processo.md](processo.md)). Um código nunca é reaproveitado. Se um requisito for
> abandonado, ele fica marcado como ~~riscado~~ com o motivo.

## Requisitos funcionais (RF)

### Itens e fluxo

| Código | O sistema deve... | Fase | Situação | Verificação |
|---|---|---|---|---|
| RF-01 | criar itens em intervalos dados por uma distribuição (Source) | 0 | ✅ | `validation.rs`, M/M/1 |
| RF-02 | permitir a primeira chegada em t=0 e um limite de chegadas | 0 | ✅ | `deterministic_line_is_exact` |
| RF-03 | guardar rótulos nos itens (ex.: `tipo`), sorteados na criação | 0 | ✅ | `routing_by_label_and_round_robin` |
| RF-04 | ter filas com capacidade e disciplina FIFO ou LIFO | 0 | ✅ | `lifo_has_same_mean_queue_length` |
| RF-05 | processar itens com tempo de preparação (setup) e de processo, um ou vários em paralelo | 0 | ✅ | M/M/c (Erlang C) |
| RF-06 | retirar itens do modelo (Sink) e medir o tempo de cada um no sistema | 0 | ✅ | M/M/1, Jackson |
| RF-07 | ligar objetos por portas e escolher a saída por regra: primeira livre, rodízio, sorteio, pesos ou rótulo | 0–1 | ✅ | `weighted_routing_splits_flow` |
| RF-08 | dar prioridade, ao puxar itens, a quem foi conectado primeiro | 0 | ✅ | `docs/arquitetura.md` §3 |
| RF-09 | bloquear um objeto quando o próximo está cheio | 0 | ✅ | `blocking_without_buffer` |
| RF-10 | montar itens: empacotar em contêiner, juntar ou formar lote (Combiner) | 1 | ✅ | `objects.rs` |
| RF-11 | desmontar itens: desempacotar ou dividir em cópias (Separator) | 1 | ✅ | `objects.rs` |
| RF-12 | acumular itens na fila e liberar em lotes, com tempo máximo de espera | 1 | ✅ | `objects.rs` |
| RF-13 | descartar chegadas quando não há lugar, contando as perdas | 1 | ✅ | `finite_capacity.rs` (M/M/1/K, M/M/1/1) |

### Disponibilidade dos recursos

| Código | O sistema deve... | Fase | Situação | Verificação |
|---|---|---|---|---|
| RF-20 | simular quebras aleatórias (MTBF/MTTR), pausando o trabalho em andamento | 1 | ✅ | `breakdowns_follow_availability` |
| RF-21 | simular turnos: pausas, noites e folgas que se repetem | 1 | ✅ | `shifts_close_the_block_on_schedule` (testes completos 🔄) |
| RF-22 | combinar quebra e turno no mesmo objeto, mostrando qual dos dois está ativo | 1 | ✅ | testes de turnos 🔄 |

### Resultados e análise

| Código | O sistema deve... | Fase | Situação | Verificação |
|---|---|---|---|---|
| RF-30 | medir, por objeto, entradas, saídas, conteúdo médio/máximo, permanência e tempo em cada estado | 0 | ✅ | `validation.rs` |
| RF-31 | descartar o período de aquecimento das estatísticas | 0 | ✅ | M/M/1 com warm-up |
| RF-32 | rodar várias replicações e mostrar média com intervalo de confiança de 95% | 0 | ✅ | CLI `utsim run -n` |
| RF-33 | comparar cenários (variações do modelo) numa tabela | 1 | ✅ | `tests/experiment.rs` |
| RF-34 | registrar séries temporais (conteúdo e estado × tempo) | 1 | ✅ | `output_data.rs` |
| RF-35 | gerar histogramas de permanência e de tempo no sistema | 1 | ✅ | `output_data.rs` |
| RF-36 | exportar resultados em CSV e JSON | 1 | ✅ | `csv.rs`, `output_data.rs` |
| RF-37 | validar o modelo antes de rodar, com mensagens claras em português | 0 | ✅ | `invalid_models_are_reported` |

### Dados de entrada (fase 1b)

| Código | O sistema deve... | Fase | Situação | Verificação |
|---|---|---|---|---|
| RF-40 | ler dados reais (CSV) de chegadas, processos ou reparos | 1b | ⏳ | |
| RF-41 | diagnosticar os dados: independência, tendência, valores atípicos | 1b | ⏳ | |
| RF-42 | ajustar **todas** as distribuições candidatas e mostrar o resultado de cada uma | 1b | ⏳ | |
| RF-43 | testar a aderência (KS, Anderson-Darling, qui-quadrado) e comparar por AIC/BIC | 1b | ⏳ | |
| RF-44 | indicar a melhor distribuição e **explicar por que** ela venceu e as outras perderam | 1b | ⏳ | |
| RF-45 | mostrar gráficos (histograma com as curvas, Q-Q, P-P) e gerar a linha pronta para o modelo | 1b | ⏳ | |

### Fases seguintes

| Código | O sistema deve... | Fase | Situação |
|---|---|---|---|
| RF-50 | mostrar o modelo em 3D e animar os itens a partir da simulação | 2 | 🔄 |
| RF-51 | controlar a animação: velocidade, pausa, passo a passo | 2 | 🔄 |
| RF-60 | montar o modelo arrastando objetos e ligando portas com o mouse | 3 | ⏳ |
| RF-61 | editar as propriedades dos objetos num painel e salvar o `.ron` | 3 | ⏳ |
| RF-70 | ter operadores e veículos que se deslocam e executam tarefas | 4 | ⏳ |
| RF-71 | distribuir tarefas entre recursos (dispatcher) e calcular rotas | 4 | ⏳ |
| RF-80 | ter esteiras com acúmulo e armazéns endereçáveis (racks) | 5 | ⏳ |
| RF-90 | descrever a lógica com fluxos de tokens e scripts | 6 | ⏳ |
| RF-100 | buscar automaticamente os melhores parâmetros (otimização) | 7 | ⏳ |
| RF-101 | rodar modelos no navegador | 7 | ⏳ |

## Requisitos não funcionais (RNF)

| Código | O sistema precisa... | Situação | Como garantimos |
|---|---|---|---|
| RNF-01 | ser software livre (GPL-3.0-or-later) | ✅ | `LICENSE`, campo `license` nos crates |
| RNF-02 | ser feito do zero (clean room), sem copiar código ou analisar binários proprietários | ✅ | comportamento especificado em `arquitetura.md` a partir de documentação pública |
| RNF-03 | ser **reprodutível**: mesma semente → mesmo resultado, sempre | ✅ | `same_seed_same_result_and_replications_differ`, snapshots golden |
| RNF-04 | ter números aleatórios independentes por objeto, para comparar cenários com justiça | ✅ | `rng.rs`; cenário base = rodada direta (`tests/experiment.rs`) |
| RNF-05 | dar resultados **estatisticamente corretos** | ✅ | testes contra a teoria das filas (M/M/1, M/M/c, Jackson, M/M/1/K) |
| RNF-06 | ser rápido: 10 replicações de 100 mil chegadas M/M/1 em menos de 1 s (release) | ✅ | hoje cerca de 45 ms; medido pela CLI |
| RNF-07 | rodar sem tela (headless) e usar todos os núcleos nas replicações | ✅ | `utsim-cli`, rayon |
| RNF-08 | guardar modelos em texto legível e versionável (RON) | ✅ | `examples/*.ron` |
| RNF-09 | manter compatibilidade: modelos antigos continuam abrindo e dando o mesmo resultado | ✅ | campos novos com valor padrão; snapshots golden |
| RNF-10 | funcionar em Windows e Linux | ✅ | CI (`windows-latest` e `ubuntu-latest`) |
| RNF-11 | manter a qualidade: formatação, zero avisos do clippy, todos os testes passando | ✅ | `scripts/verificar.sh` / `.ps1`, CI |
| RNF-12 | ser extensível: blocos novos sem mexer no kernel | ✅ | trait `Behavior`; Combiner/Separator feitos sem tocar no núcleo |
| RNF-13 | separar simulação e visualização (o 3D só lê o estado) | ✅ | `utsim-view` só lê um `Replay`; `timeline.rs` bate com `RunReport.series` |
| RNF-14 | falar português com o usuário (mensagens, relatórios, docs) e usar inglês no código | ✅ | revisão nos merges |
| RNF-15 | ter documentação sempre atualizada: requisitos, arquitetura, roteiro e processo | ✅ | regra do `processo.md` |
