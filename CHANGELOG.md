# Changelog

Todas as mudanças relevantes do UTSim ficam registradas aqui.
O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto usa
[versionamento semântico](https://semver.org/lang/pt-BR/) (veja `docs/versionamento.md`).

## [Não lançado]

### Adicionado
- Campo opcional `format` nos modelos `.ron` (ausente vale 1), com erro claro para formatos mais novos e gancho de migração.
- Biblioteca de modelos 3D articulados gerados por código (`utsim-assets`, binário `utsim-assets`): empilhadeira, paleteira, AGV, palete, caixa, porta-paletes, esteira de rolos, tanque, máquina, bancada, operador e área de piso, versionados em `assets/modelos/*.glb`.
- Visualizador com "Visual: simples / realista": modelos glTF por papel e tipo ou por `visual.asset`, garfos subindo até a célula, rodas girando, luz de estado do AGV, operadores andando sem deslizar os pés e cargas nos garfos e nas mãos.
- Passos de roteiro `IrPara(t)` e `Focar(...)`.

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
