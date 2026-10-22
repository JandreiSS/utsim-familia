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
- [x] Séries temporais (conteúdo × tempo) e histogramas de permanência
- [x] Cenários (variação de parâmetros) e tabela comparativa (`utsim experiment`)
- [x] Exportação CSV
- [x] Combiner, Separator e Queue com lote
- [x] Quebras (MTBF/MTTR) com pausa dos tempos em andamento
- [ ] Tabelas de turnos (parada programada)
- [x] Roteamento ponderado (`Weighted`) e eventos canceláveis
- [x] **Validação:** M/M/c (Erlang C), redes de Jackson, LIFO, disponibilidade com quebras
- [ ] **Validação:** M/M/1/K (precisa de perda de chegadas na Source)
- [ ] Rede de segurança: snapshots golden, `scripts/verificar`, CI

## Fase 1b: modelagem de dados de entrada (ajuste de distribuições)
A partir de dados reais (intervalos entre chegadas, tempos de processo, tempos de reparo...),
o UTSim testa **todas** as distribuições candidatas, mostra o resultado de cada uma e
justifica a escolha com métricas estatísticas. A ideia vem de ferramentas como o ferramentas acadêmicas e
o ferramentas de ajuste, mas o desenho é nosso.

**Entrada e diagnóstico dos dados**
- [ ] Leitura de CSV/TXT (uma coluna, ou coluna escolhida), com timestamps opcionais para gerar intervalos
- [ ] Estatística descritiva: n, média, mediana, desvio, CV, assimetria, curtose, mín/máx, quartis
- [ ] Verificação das hipóteses, com aviso quando falharem:
  - independência (autocorrelação por lag e gráfico de dispersão x[i] × x[i+1], teste de corridas)
  - estacionariedade (tendência ao longo do tempo, teste de Mann-Kendall)
  - valores atípicos (IQR) e empates/arredondamento (dados discretizados)
- [ ] Recomendação quando os dados não forem i.i.d. (separar por turno/período, usar série empírica)

**Ajuste**
- [ ] Candidatas contínuas: Exponencial, Erlang, Gama, Weibull, Lognormal, Normal (truncada),
  Triangular, Uniforme, Beta (com min/máx), Log-logística, Pearson V e VI; com e sem
  deslocamento (*location*); discretas: Poisson, Geométrica, Binomial, Binomial Negativa
- [ ] Estimação por máxima verossimilhança (MLE), com método dos momentos como ponto de partida e alternativa
- [ ] Distribuição empírica (contínua por interpolação, ou discreta) como alternativa quando nenhuma paramétrica passar

**Testes e métricas (para todas as candidatas, numa tabela única)**
- [ ] Kolmogorov-Smirnov, com p-valor por *bootstrap* paramétrico, porque os parâmetros são estimados dos próprios dados
- [ ] Anderson-Darling, mais sensível às caudas, também com bootstrap
- [ ] Qui-quadrado com classes equiprováveis e graus de liberdade corrigidos pelos parâmetros estimados
- [ ] Log-verossimilhança, AIC, BIC e pesos de Akaike
- [ ] Erro quadrático entre histograma e densidade, e erro máximo da P-P

**Escolha e justificativa**
- [ ] Ranking transparente: primeiro descarta quem é rejeitado nos testes (α configurável), depois ordena por AIC/BIC e estatística A-D. A regra aparece no relatório.
- [ ] Texto explicando por que o modelo vencedor ganhou e por que os outros perderam
  (ex.: "Weibull rejeitada no A-D, p=0,01, cauda direita subestimada")
- [ ] Aviso quando dois modelos são praticamente equivalentes (ΔAIC < 2)
- [ ] Gráficos: histograma com as densidades sobrepostas, Q-Q, P-P e ECDF × CDF teórica
- [ ] Saída pronta para o modelo: a linha `Dist` do `.ron` (ex.: `Weibull(shape: 1.8, scale: 4.2)`)
- [ ] CLI `utsim fit dados.csv [--coluna ...] [--alpha 0.05] [--html relatorio.html]`; depois no editor (fase 3)

**Validação**
- [ ] Gerar amostras de distribuições conhecidas e verificar que o ajuste recupera a família e os parâmetros
- [ ] Taxa de erro tipo I dos testes ≈ α (com bootstrap) em amostras simuladas
- [ ] Comparar estimativas MLE com valores de referência da literatura (Law, *Simulation Modeling and Analysis*, cap. 6)

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
- [ ] Fluidos, AGVs, emulação via OPC UA, interface para aprendizado por reforço
- [ ] Build WASM: rodar modelos no navegador
