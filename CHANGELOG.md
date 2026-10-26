# Changelog

Todas as mudanças relevantes do UTSim ficam registradas aqui.
O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto usa
[versionamento semântico](https://semver.org/lang/pt-BR/) (veja `docs/versionamento.md`).

## [Não lançado]

### Adicionado
- Fase 10 (resultados no editor): janela "Resultados" com replicações em segundo plano (progresso e "Cancelar"), tabela por objeto com intervalos de 95%, ordenação, filtro por tipo e o gargalo destacado, gráficos de ocupação, estados, série temporal e histograma, aba "Cenários" (montar, abrir, salvar e comparar com a diferença pareada) e aba "Otimização" (grade, recozimento ou genético, convergência e "Aplicar ao modelo"), exportação de CSV e relatório HTML. `utsim experiment` passou a imprimir a diferença pareada para o primeiro cenário. Condições de roteiro `PainelResultados`, `AbaResultados`, `AnaliseRodando`, `ReplicacoesNoResultado`, `Gargalo`, `CenariosComparados`, `Exportado` e `MelhorPontoContem`.
- Fase 12 (pacotes): ícone do aplicativo (`assets/icone`, gerado por `tools/icone`) na janela e nos `.exe`; instalador do Windows por usuário (NSIS), `.deb` e `.tar.gz` com `instalar.sh` para o Linux; fluxo `release.yml` com simulação (`workflow_dispatch`) e notas tiradas deste arquivo.
- Campo opcional `format` nos modelos `.ron` (ausente vale 1), com erro claro para formatos mais novos e gancho de migração.
- Biblioteca de modelos 3D articulados gerados por código (`utsim-assets`, binário `utsim-assets`): empilhadeira, paleteira, AGV, palete, caixa, porta-paletes, esteira de rolos, tanque, máquina, bancada, operador e área de piso, versionados em `assets/modelos/*.glb`.
- Visualizador com "Visual: simples / realista": modelos glTF por papel e tipo ou por `visual.asset`, garfos subindo até a célula, rodas girando, luz de estado do AGV, operadores andando sem deslizar os pés e cargas nos garfos e nas mãos.
- Passos de roteiro `IrPara(t)` e `Focar(...)`.
- Fase 11: `utsim gerar-modelo` (modelos sintéticos de qualquer tamanho), benchmark `cargo bench -p utsim-scene`, metas de desempenho em testes `#[ignore]` e `docs/desempenho.md`.
- Testes de robustez (proptest) do leitor de modelos e do leitor de dados do ajuste.
- Página web: o motor roda em um Web Worker, com botão "Cancelar".
- Fase 9 (editor): biblioteca de objetos por categoria (com operador, empilhadeira e AGV prontos), colocar clicando ou arrastando até o chão com encaixe na grade configurável, seleção múltipla (Shift + clique e caixa com Shift + arrastar), mover, excluir, alinhar e distribuir a seleção, editar uma propriedade de vários objetos do mesmo tipo, copiar, recortar, colar e duplicar (Ctrl+C/X/V/D, nomes únicos e ligações internas preservadas), setas para mover e janela "Atalhos" (F1). Cada operação é um passo de desfazer.
- Seção "Aparência" nas propriedades: ativo da biblioteca ou do catálogo de terceiros (autor e licença na dica), cor, escala, giro e "Restaurar padrão"; o modo realista desenha os `.glb` de terceiros no tamanho real virados para +x. Campo "Transferência pela distância" nas propriedades.
- Passos de roteiro `Segurar` e `LargarTecla` e condições `SelecionadosIgual`, `SelecaoContem`, `AtivoDe`, `ObjetosComPrefixo` e `Ligado`.

### Corrigido
- Modelos que travavam ou esgotavam a memória agora são recusados na validação (fontes com intervalo quase nulo, porta-paletes ou `Split` gigantes, fluxos com intervalo zero) ou terminam com erro (muitos eventos no mesmo instante).

### Alterado
- Núcleo e índice da cena mais rápidos (10 % a 45 % no núcleo), sem mudar resultados.

## [0.1.0]

Primeira versão pública, cobrindo as fases 0 a 7 do roteiro.

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
