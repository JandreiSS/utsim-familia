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
- [x] Tabelas de turnos (parada programada), combináveis com quebras
- [x] Roteamento ponderado (`Weighted`) e eventos canceláveis
- [x] **Validação:** M/M/c (Erlang C), redes de Jackson, LIFO, disponibilidade com quebras
- [x] Perda de chegadas na Source (`on_blocked: Drop`)
- [x] **Validação:** M/M/1/K e M/M/1/1 (perda de Erlang)
- [x] Rede de segurança: snapshots golden, `scripts/verificar`, CI

## Fase 1b: modelagem de dados de entrada (ajuste de distribuições)
A partir de dados reais (intervalos entre chegadas, tempos de processo, tempos de reparo...),
o UTSim testa **todas** as distribuições candidatas, mostra o resultado de cada uma e
justifica a escolha com métricas estatísticas.

**Entrada e diagnóstico dos dados**
- [x] Leitura de CSV/TXT (uma coluna, ou coluna escolhida), com timestamps opcionais para gerar intervalos
- [x] Estatística descritiva: n, média, mediana, desvio, CV, assimetria, curtose, mín/máx, quartis
- [x] Verificação das hipóteses, com aviso quando falharem:
  - independência (autocorrelação por lag e gráfico de dispersão x[i] × x[i+1], teste de corridas)
  - estacionariedade (tendência ao longo do tempo, teste de Mann-Kendall)
  - valores atípicos (IQR) e empates/arredondamento (dados discretizados)
- [x] Recomendação quando os dados não forem i.i.d. (separar por turno/período, usar série empírica)

**Ajuste**
- [x] Candidatas contínuas: Exponencial, Erlang, Gama, Weibull, Lognormal, Normal (truncada),
  Triangular, Uniforme, Beta (com min/máx), Log-logística, Pearson V e VI; com e sem
  deslocamento (*location*); discretas: Poisson, Geométrica, Binomial, Binomial Negativa
- [x] Estimação por máxima verossimilhança (MLE), com método dos momentos como ponto de partida e alternativa
- [x] Distribuição empírica (contínua por interpolação, ou discreta) como alternativa quando nenhuma paramétrica passar, com a linha `.ron` gerada

**Testes e métricas (para todas as candidatas, numa tabela única)**
- [x] Kolmogorov-Smirnov, com p-valor por *bootstrap* paramétrico, porque os parâmetros são estimados dos próprios dados
- [x] Anderson-Darling, mais sensível às caudas, também com bootstrap
- [x] Qui-quadrado com classes equiprováveis e graus de liberdade corrigidos pelos parâmetros estimados
- [x] Log-verossimilhança, AIC, BIC e pesos de Akaike
- [x] Erro quadrático entre histograma e densidade, e erro máximo da P-P

**Escolha e justificativa**
- [x] Ranking transparente: descarta quem é rejeitado nos testes (α configurável), ordena por AIC, desempata por A-D e aplica parcimônia entre equivalentes (ΔAIC < 2). A regra aparece no relatório.
- [x] Texto explicando por que o modelo vencedor ganhou e por que os outros perderam
  (ex.: "Weibull rejeitada no A-D, p=0,01, cauda direita subestimada")
- [x] Aviso quando dois modelos são praticamente equivalentes (ΔAIC < 2)
- [x] Gráficos: histograma com as densidades sobrepostas, Q-Q, P-P e ECDF × CDF teórica
- [x] Saída pronta para o modelo: a linha `Dist` do `.ron` (ex.: `Weibull(shape: 1.8, scale: 4.2)`)
- [x] CLI `utsim fit dados.csv [--coluna ...] [--alpha 0.05] [--html relatorio.html]`
- [ ] Ajuste dentro do editor (fase 3)

**Validação**
- [x] Gerar amostras de distribuições conhecidas e verificar que o ajuste recupera a família e os parâmetros (`tests/recovery.rs`)
- [x] Taxa de erro tipo I dos testes ≈ α (com bootstrap) em amostras simuladas
- [ ] Comparar estimativas MLE com valores de referência da literatura (Law, *Simulation Modeling and Analysis*, cap. 6)

## Fase 2: visualização 3D (só leitura)
- [x] Rastro de eventos emitido pelo motor (`Observer`, `utsim replay`)
- [x] Visualizador Bevy (`utsim-view`): carrega o `.ron` ou o replay `.json`, desenha objetos em `position` (ou layout automático), anima itens
- [x] Controle de velocidade, pausa, passo a passo e barra de tempo
- [x] **Validação:** o índice da linha do tempo reproduz as séries amostradas (`timeline.rs`); a conferência visual em janela ainda é manual

## Fase 3: editor
- [x] Documento editável sem interface (`utsim-edit`): comandos, desfazer/refazer, validação ao vivo, salvamento estável
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
- [x] Esteiras cinemáticas com e sem acúmulo (`Conveyor`)
- [ ] Fotocélulas e transferências dedicadas (junções usam filas e regras de saída)
- [x] Porta-paletes endereçável (`Rack`)
- [x] Esteiras e porta-paletes no 3D (posições reconstruídas do rastro, conferidas contra o kernel)
- [x] **Validação:** centro de distribuição (`examples/centro_de_distribuicao.ron`): conservação de itens, vazão, lei de Little no porta-paletes, determinismo

## 3D realista e realidade virtual (plano em `docs/plano_3d_realista.md`)
- [x] A3D-1: cena independente de motor (`utsim-scene`), `utsim cena`, visualizador usando a cena
- [x] A3D-2: modelos articulados gerados por código (empilhadeira, AGV, porta-paletes...) e carga de glTF
- [ ] A3D-3: pessoas animadas, itens presos às mãos e aos garfos, garfos até o nível, giro suave
- [x] A3D-4: câmeras de seguir, de primeira pessoa e do recurso; níveis de detalhe e limite de rótulos para modelos grandes
- [ ] A3D-5: cliente Unreal com realidade virtual (OpenXR), lendo `utsim-scene/1`
- [x] A3D-6: cena ao vivo por socket a partir da emulação (`--cena-porta`, `utsim cena --servir`)

## Melhorias anotadas (dos relatórios das frentes)
- [ ] Relatório com contagem de itens criados e destruídos por bloco (hoje só no rastro)
- [ ] Estado próprio para armazenagem ("armazenando") no porta-paletes, em vez de `releasing`
- [ ] Prioridade de tarefas configurável no modelo (o kernel hoje pede sempre prioridade 0)
- [ ] Painel do visualizador: tirar o "(0)" redundante ao lado de "recebidos: N" na saída
- [ ] Roteiros de QA mais robustos: passos condicionais (ex.: pausar só se estiver reproduzindo), `Digitar` com espaços
- [ ] Diagrama de fluxo: arrastar caixas, zoom; arestas longas desviando das caixas
- [ ] Linha de status do Simular não deve contar os blocos de fluxo
- [ ] Scripts: evitar a conversão de rótulos quando o script não usa `item` (custo por chamada)
- [ ] Página web: o visualizador 3D tem 36 MB (testar `wasm-opt`, compressão); rodar a simulação num Web Worker; as métricas próprias (níveis de tanques) viram colunas largas na tabela
- [ ] Emulação OPC UA: segurança (certificados, usuário) e publicação mais barata a cada tick
- [ ] `utsim emular --ate T`: o cabeçalho do relatório ainda mostra o `stop_time` do modelo
- [ ] `utsim fit --timestamps`: escolher a coluna de tempos automaticamente (hoje usa a primeira coluna numérica sem `--coluna`)
- [ ] Resultados: legenda do gráfico de estados cobre o fim das barras; estados de filas e chegadas com o mesmo cinza; relatório HTML sem séries, histogramas e otimização; seletor de arquivos
- [ ] CI: os envios (push) não disparam a CI automaticamente no repositório recriado; hoje é disparada à mão
- [ ] Fonte "sempre pronta" (saturada) sem precisar de intervalo mínimo artificial (achado da validação do piloto)
- [ ] Mostrar os avisos de validação (distâncias) também no visualizador e na página web
- [ ] Página web: incluir os modelos dos tutoriais na lista de exemplos
- [ ] Métricas: expor a contagem dos registros extras (`<bloco>.lost.count`, hoje só a média) e a utilização por servidor de processadores com capacidade > 1 (achados da validação do otimizador)

## Fase 6: lógica do usuário
- [x] Fluxo de tokens (gerar, agenda, ao evento, esperar, decidir, atribuir, adquirir/liberar recurso, aguardar evento, criar item, lote, fim)
- [x] Scripts em Rhai: expressões, gatilho de entrada (`on_entry`) e regra de saída por script, com limites de segurança
- [x] Diagrama dos fluxos no editor, edição de atividades e contagem de tokens ao vivo
- [x] **Validação:** M/M/1, M/M/3 e Jackson só com fluxos; lei de Little; roteamento por script reprodutível; docas de recebimento (`examples/fluxo_docas.ron`)

## MVP
- [x] Fase 8: consolidação (formato versionado, publicação, GitHub privado)
- [x] Fase 9: editor completo (biblioteca, multisseleção, copiar e colar, aparência, rede de caminhos, fluidos, avisos)
- [x] Fase 10: resultados dentro do app (tabela com IC, gráficos, cenários, otimização, exportação)
- [x] Fase 11: robustez e desempenho (parte 1)
- [x] Fase 12: instalador (Windows NSIS, .deb, .tar.gz) e publicação por tag
- [x] Fase 13: manual e 6 tutoriais (parte 1)
- [ ] Fase 14: piloto com casos reais

## Fase 7+: avançado
- [x] Otimização de parâmetros: grade, recozimento simulado, genético, refinamento, comparação pareada (`utsim otimizar`)
- [x] Aprendizado por reforço: decisão por agente no fluxo, ambiente estilo Gym (`utsim-env`), Q-learning tabular, ligações Python (`crates/utsim-py`)
- [x] Fluidos: tanques, tubos, fontes, sumidouros, conversores item↔volume (`utsim-fluid`, `examples/tanques.ron`)
- [x] AGVs: capacidade por trecho, espera FIFO, detecção de impasse (`examples/agvs.ron`)
- [x] Emulação via OPC UA em tempo real, com gravação e reprodução de comandos (`utsim emular`)
- [x] Visualizador 3D no navegador (WebGL2, aba 3D da página)
- [x] Fluidos e AGVs desenhados no 3D (níveis dos tanques, trechos controlados; roteiros `fluidos_tanques.ron` e `agvs.ron`)
- [x] Build WASM: rodar modelos no navegador (`utsim-web`, página `web/`, `scripts/web.sh`)
