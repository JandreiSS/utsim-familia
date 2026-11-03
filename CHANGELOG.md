# Changelog

Todas as mudanças relevantes do UTSim ficam registradas aqui.
O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto usa
[versionamento semântico](https://semver.org/lang/pt-BR/) (veja `docs/versionamento.md`).

## [Não lançado]

### Adicionado
- PERT/CPM de uma parte do processo (crate `utsim-pert`, comando `utsim pert`): rede de atividades de uma seleção de objetos (`--objetos`) ou de um trecho (`--de A --ate B`), com divisão E no `Separator`, junção E no `Combiner`, saídas OU, ciclos de retrabalho (fator `1/(1-p)`), transportes e transferências como atividades e a lista de hipóteses e avisos; CPM com início e fim mais cedo e mais tarde, folgas total e livre e todos os caminhos críticos; três modos: caminho crítico observado na simulação (padrão, com filas, transportes, operadores, quebras e turnos), PERT clássico (`(a, m, b)` das distribuições, momentos exatos ou beta, probabilidade de prazo) e Monte Carlo determinístico (criticidade por atividade e por caminho, percentis e sensibilidade); relatório em texto, CSV e HTML com a rede, o Gantt, o histograma e o tornado, e as coordenadas do diagrama para a interface. Capítulo "PERT/CPM" no manual.

### Em desenvolvimento
- Interface Godot, fase G1 (`godot/`, ponte `crates/utsim-godot` com o godot-rust gdext): abre e valida um modelo, roda as replicações no Rust e reproduz a replicação 0 com os modelos da biblioteca (empilhadeira com garfos, AGV com luz, rodas, porta-paletes e esteira do tamanho do modelo), pessoas animadas por esqueleto e cargas nos garfos e nas mãos. `scripts/godot.sh`, teste sem janela `scripts/godot_teste.sh`, capturas roteirizadas e passo opcional `UTSIM_GODOT=1` no `verificar.sh`. Plano das fases G2 a G5 em `docs/plano_godot.md`.

## [0.1.0] - 2026-11-02

Primeira versão para testes: simulação, editor 3D, resultados, cenários, otimização, ajuste de distribuições, manual com tutoriais e estudos de caso.

### Adicionado
- Núcleo de simulação a eventos discretos determinístico, com fluxos aleatórios por bloco e 10 distribuições (fase 0).
- Blocos Source, Queue, Processor, Sink, Combiner, Separator, esteiras (`Conveyor`) e porta-paletes (`Rack`).
- Quebras (MTBF/MTTR), turnos, roteamento ponderado e perda de chegadas.
- Formato de modelo `.ron` com validação em português e CLI `utsim run | check | experiment`.
- Replicações em paralelo com intervalo de confiança de 95%, séries temporais, histogramas e exportação CSV (fase 1).
- Ajuste de distribuições a dados (`utsim fit`): MLE, KS, Anderson-Darling, qui-quadrado, AIC/BIC e relatório HTML (fase 1b).
- Visualizador 3D em Bevy (`utsim-view`) com replay, controle de velocidade e linha do tempo (fase 2).
- Editor sem interface (`utsim-edit`) com desfazer/refazer e salvamento estável (fase 3).
- Recursos móveis: operadores, transportadores, despachantes, rede de caminhos com A* (fase 4).
- Manuseio de materiais: esteiras com e sem acúmulo e porta-paletes endereçável (fase 5).
- Lógica do usuário: fluxos de tokens e scripts em Rhai com limites de segurança (fase 6).
- Otimização de parâmetros (grade, recozimento, genético), aprendizado por reforço (`utsim-env`, ligações Python), fluidos, AGVs, emulação OPC UA e versão no navegador via WebAssembly (fase 7).
- Validação contra a teoria: M/M/1, M/M/c, M/M/1/K, redes de Jackson e lei de Little.
- Verificação completa em `scripts/verificar` e integração contínua.
- Estudos de caso (`docs/estudos_de_caso/`): linha de montagem com inspeção e retrabalho, centro de distribuição com corredor estreito e pronto atendimento com turnos e prioridade. Cada um traz enunciado, dados brutos em CSV, modelo de referência, experimento ou otimização e resolução comentada; o índice lista as lacunas do programa encontradas ao modelá-los. Capítulo "Estudos de caso" no manual e teste `crates/utsim-model/tests/estudos_de_caso.rs`.
- Fase 10 (resultados no editor): janela "Resultados" com replicações em segundo plano (progresso e "Cancelar"), tabela por objeto com intervalos de 95%, ordenação, filtro por tipo e o gargalo destacado, gráficos de ocupação, estados, série temporal e histograma, aba "Cenários" (montar, abrir, salvar e comparar com a diferença pareada) e aba "Otimização" (grade, recozimento ou genético, convergência e "Aplicar ao modelo"), exportação de CSV e relatório HTML. `utsim experiment` passou a imprimir a diferença pareada para o primeiro cenário. Condições de roteiro `PainelResultados`, `AbaResultados`, `AnaliseRodando`, `ReplicacoesNoResultado`, `Gargalo`, `CenariosComparados`, `Exportado` e `MelhorPontoContem`.
- Fase 12 (pacotes): ícone do aplicativo (`assets/icone`, gerado por `tools/icone`) na janela e nos `.exe`; instalador do Windows por usuário (NSIS), `.deb` e `.tar.gz` com `instalar.sh` para o Linux; fluxo `release.yml` com simulação (`workflow_dispatch`) e notas tiradas deste arquivo.
- Campo opcional `format` nos modelos `.ron` (ausente vale 1), com erro claro para formatos mais novos e gancho de migração.
- Biblioteca de modelos 3D articulados gerados por código (`utsim-assets`, binário `utsim-assets`): empilhadeira, paleteira, AGV, palete, caixa, porta-paletes, esteira de rolos, tanque, máquina, bancada, operador e área de piso, versionados em `assets/modelos/*.glb`.
- Visualizador com "Visual: simples / realista": modelos glTF por papel e tipo ou por `visual.asset`, garfos subindo até a célula, rodas girando, luz de estado do AGV, operadores andando sem deslizar os pés e cargas nos garfos e nas mãos.
- Passos de roteiro `IrPara(t)` e `Focar(...)`.
- A3D-3 (movimento realista): pessoas animadas por esqueleto (boneco CC0, clipes de parar, andar, correr, andar carregando, pegar e trabalhar, passada ligada à distância andada, roupa de trabalho, capacete e colete, item entre as mãos); a carga troca de mãos no meio da janela de carga ou descarga, subindo nos garfos até a célula; curvas suaves nas quinas do caminho (`utsim-scene`, também no fluxo `utsim-scene/1`); sombras do sol com a caixa "Sombras"; exemplo `pessoas_carregando.ron`; passo de roteiro `MedirQuadros(segundos)`.
- Fase 11: `utsim gerar-modelo` (modelos sintéticos de qualquer tamanho), benchmark `cargo bench -p utsim-scene`, metas de desempenho em testes `#[ignore]` e `docs/desempenho.md`.
- Testes de robustez (proptest) do leitor de modelos e do leitor de dados do ajuste.
- Página web: o motor roda em um Web Worker, com botão "Cancelar".
- Fase 9 (editor): biblioteca de objetos por categoria (com operador, empilhadeira e AGV prontos), colocar clicando ou arrastando até o chão com encaixe na grade configurável, seleção múltipla (Shift + clique e caixa com Shift + arrastar), mover, excluir, alinhar e distribuir a seleção, editar uma propriedade de vários objetos do mesmo tipo, copiar, recortar, colar e duplicar (Ctrl+C/X/V/D, nomes únicos e ligações internas preservadas), setas para mover e janela "Atalhos" (F1). Cada operação é um passo de desfazer.
- Seção "Aparência" nas propriedades: ativo da biblioteca ou do catálogo de terceiros (autor e licença na dica), cor, escala, giro e "Restaurar padrão"; o modo realista desenha os `.glb` de terceiros no tamanho real virados para +x. Campo "Transferência pela distância" nas propriedades.
- Passos de roteiro `Segurar` e `LargarTecla` e condições `SelecionadosIgual`, `SelecaoContem`, `AtivoDe`, `ObjetosComPrefixo` e `Ligado`.
- A3D-4 (câmeras e modelos grandes): menu "Câmera" na barra e teclas 1 a 4 com os modos Órbita, Seguir (um recurso móvel por trás e por cima, com "Seguir este recurso" no painel), Primeira pessoa (a 1,6 m do chão com `W`, `A`, `S`, `D`, `Shift`, botão direito ou mouse travado com `L`, colisão simples com os blocos) e Do recurso (no assento do motorista), com transição suave e estado guardado por modo. Níveis de detalhe: modelo realista vira caixa, item vira cubo e some, e só os 150 rótulos mais próximos são escritos (só em modelos com 100 blocos ou 400 itens); `F3` mostra tempos de quadro e contagens, `F4` liga e desliga. Passos de roteiro `Camera`, `Lod`, `Tempos` e `Medir` e condições `CameraModo`, `CameraAlvo`, `CameraAlturaEntre`, `CameraPertoDe` e `LodAtivo`. Medições em `docs/desempenho.md`.
- Acabamento do editor (piloto simulado): campos de posição (x, y, z) e giro no painel de cada objeto; aba "Objetos" ao lado da Biblioteca, com busca por nome, filtro por tipo, seleção com Shift e duplo clique que enquadra; clique repetido no mesmo ponto alterna entre objetos sobrepostos, com uma dica; ordem das ligações (portas de saída e prioridade das entradas) com botões Subir e Descer desfazíveis (comando `SwapConnections`); "Salvar como..." (`Ctrl+Shift+S`); zoom da roda em direção ao cursor; busca nas listas longas e famílias de distribuição agrupadas. Passo `Focar` também em Editar; condições de roteiro `GiroDe`, `PortaDe`, `EntradaDe`, `PilhaSobCursor`, `ListaObjetos`, `AbaEsquerda` e `CaminhoTermina`. Roteiro `acabamento_interface.ron`.
- Chegadas por período num ciclo (dia ou semana): cada período igual ao padrão, com distribuição própria, com taxa por hora (Poisson exato nas fronteiras) ou sem chegadas, com rótulos por período; tabela no editor com horários e chegadas esperadas.
- Fim de turno: pausar ou terminar o trabalho em andamento (`on_end: Pause | Finish`), também para transportadores e equipes; calendários de turno com nome (`calendars`, `Calendar("S1")`) usados por vários objetos.
- Gargalo geral e gargalo dentro do turno no painel de resultados, no relatório HTML e no `utsim run`.

### Corrigido
- Editor: os sinais de viável e inviável da otimização eram caixas vazias (a fonte padrão não tem os glifos); agora são "sim"/"não" e "atende"/"não atende" em cores, e um teste confere que todo símbolo escrito nos painéis existe na fonte. A tabela de cenários rola na horizontal em vez de cortar depois de 4 cenários. As listas suspensas mostram todas as opções (antes paravam em 400 pontos). O painel de propriedades volta ao topo ao trocar de objeto. Texto inválido em "editar como texto" continua na caixa, em vermelho e com o motivo, sem alterar o modelo.
- Modo roteirizado: `Digitar` agora mantém maiúsculas e digita espaços; `Arrastar` mira o ponto do chão do bloco e solta depois de parar no destino (antes errava cerca de 1 m, de forma variável).
- Polimento da v0.1 após o piloto: ocupação nas quatro formas (`utilization`, `utilization_incl_blocked`, `utilization_in_shift`, `utilization_excl_blocked`), perdas de fontes com `Drop` (`lost.count`, `entered.count`, `loss_fraction` e totais `model.*`), contagem de todas as medidas (`<bloco>.<medida>.count`) e medição do tempo entre dois pontos do fluxo (`spans: [(name, from, to)]`, métricas `<nome>.value.mean` e `.count`). Tabela e gráfico de ocupação, CSV, HTML e `utsim run` mostram as quatro ocupações.
- Gargalo errado e ocupação acima de 100%: a ocupação de montagens, processos e desmontagens era o conteúdo médio dividido pela capacidade, e o conteúdo de uma montagem inclui os componentes que esperam o resto do conjunto (347% no estudo de caso 1, com a montagem apontada como gargalo). Agora é a fração do tempo trabalhando (`processing` e `setup`), com a média de servidores ocupados sobre a capacidade nos processos com várias máquinas, e nunca passa de 100% por servidor.
- `utsim fit` ajustava a coluna de numeração (1, 2, 3...) quando ela vinha primeiro e devolvia, por exemplo, `Uniform(1, 250)` sem aviso. Agora pula as colunas de numeração, diz qual coluna usou e por quê, escolhe a coluna de instantes com `--timestamps` e pede `--coluna` quando restam várias plausíveis.
- Visualizador: o editor revalidava o modelo inteiro a cada quadro, mesmo em Simular (cerca de 165 ms com 500 objetos); agora só refaz quando o documento muda (`Document::revision`).
- Modelos que travavam ou esgotavam a memória agora são recusados na validação (fontes com intervalo quase nulo, porta-paletes ou `Split` gigantes, fluxos com intervalo zero) ou terminam com erro (muitos eventos no mesmo instante).

### Alterado
- Núcleo e índice da cena mais rápidos (10 % a 45 % no núcleo), sem mudar resultados.
