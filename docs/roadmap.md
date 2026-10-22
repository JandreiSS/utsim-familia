# Roteiro por fases

Cada fase termina com algo **executável e validado**. A próxima fase só começa quando a
validação da anterior passa.

## Fase 0: núcleo DES ✅
- [x] Relógio, lista de eventos determinística, itens com rótulos
- [x] Streams aleatórios por bloco, 10 distribuições
- [x] Protocolo de fluxo (liberar → ambos abertos → transferir), regras de saída
- [x] Source, Queue (FIFO/LIFO, capacidade), Processor (setup + processo, capacidade n), Sink
- [x] Estatísticas: estados, conteúdo ponderado no tempo, permanência, warm-up
- [x] Formato `.ron`, validação com mensagens claras, CLI `utsim run | check`
- [x] Replicações em paralelo com IC de 95%
- [x] **Validação:** M/M/1 contra a teoria, linha determinística exata, bloqueio, determinismo

## Fase 1: análise estatística mais rica
- [ ] Séries temporais (conteúdo × tempo) e histogramas de permanência
- [ ] Cenários (variação de parâmetros) e tabela comparativa
- [ ] Exportação CSV
- [ ] Combiner, Separator e Queue com lote
- [ ] Quebras (MTBF/MTTR) e tabelas de turnos (estado `down`)
- [ ] **Validação:** M/M/c, M/M/1/K e redes de Jackson contra a teoria

## Fase 2: visualização 3D (só leitura)
- [ ] Registro de trajetórias (`t0, t1, origem, destino`) emitido pelo motor
- [ ] Visualizador Bevy: carrega o `.ron`, desenha objetos em `position`, anima itens
- [ ] Controle de velocidade, pausa e passo a passo
- [ ] **Validação:** a animação bate com o log de eventos

## Fase 3: editor
- [ ] Arrastar objetos da biblioteca, mover/rotacionar (gizmos), grade
- [ ] Conectar portas com o mouse; painel de propriedades (egui)
- [ ] Salvar e abrir `.ron`
- [ ] **Validação:** montar uma linha de 5 postos sem editar texto

## Fase 4: recursos móveis
- [ ] TaskExecuter, TaskSequence (travel, load, unload, delay, utilize, break)
- [ ] Dispatcher (mais próximo, menos ocupado, rodízio)
- [ ] Operadores em setup/processo (`waiting_operator`), transporte entre blocos
- [ ] Rede de caminhos com A*, aceleração e desaceleração
- [ ] **Validação:** empilhadeira atendendo 3 postos, conferida à mão

## Fase 5: manuseio de materiais
- [ ] Esteiras cinemáticas com acúmulo, junções, fotocélulas
- [ ] Rack / armazenagem endereçável
- [ ] **Validação:** centro de distribuição simples

## Fase 6: lógica do usuário
- [ ] Fluxo de tokens no estilo fluxo de processo (create, delay, decide, acquire/release, wait)
- [ ] Script embutido (Rhai ou Lua) para gatilhos
- [ ] **Validação:** WMS simples implementado fora dos objetos físicos

## Fase 7+: avançado
- [ ] Otimização de parâmetros (metaheurísticas)
- [ ] Ajuste de distribuições a partir de dados
- [ ] Fluidos, AGVs, emulação via OPC UA, interface para aprendizado por reforço
- [ ] Build WASM: rodar modelos no navegador
