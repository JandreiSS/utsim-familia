# Testes e verificação

O UTSim tem quatro camadas de teste. Cada uma pega um tipo diferente de erro.

## As quatro camadas

1. **Testes unitários (núcleo).** Ficam dentro de `crates/*/src` e testam peças isoladas:
   relógio, lista de eventos, RNG, distribuições, estatísticas e o protocolo de fluxo.
   São os primeiros a quebrar quando uma função básica muda.

2. **Validação contra a teoria.** `crates/utsim-model/tests/validation.rs` e
   `crates/utsim-model/tests/queueing_theory.rs` rodam modelos cujo resultado é conhecido
   em forma fechada: M/M/1, M/M/c (Erlang C), redes de Jackson, LIFO, bloqueio e quebras.
   Comparam com tolerância estatística. Pegam erros de lógica de simulação, como uma
   fila que atende na ordem errada. O piloto de validação
   (`crates/utsim-model/tests/validacao_piloto.rs`, resultados em `docs/validacao.md`) amplia
   isso com cadeias de Markov, M/G/1, retrabalho, quebras, turnos, transporte, esteira,
   transferência, combinador e fluidos, com intervalos de confiança.

3. **Snapshots golden.** `crates/utsim-model/tests/golden.rs` roda cada modelo de
   `examples/` com duas replicações (tempo de parada limitado a 5000) e compara todas as
   métricas com `tests/golden/<modelo>.json`, com tolerância relativa de 1e-9.
   Pegam **qualquer** mudança de comportamento, mesmo a que não viola a teoria: uma
   troca na ordem de sorteio do RNG, um arredondamento novo ou um evento a mais.

4. **Verificação completa e CI.** Os scripts `scripts/verificar.sh` e
   `scripts/verificar.ps1` rodam tudo em sequência, e o workflow
   `.github/workflows/ci.yml` faz o mesmo a cada push e pull request.

### Quando os snapshots golden quebram

Um teste golden falhando **não** significa que o código está errado. Significa que o
resultado mudou. Antes de qualquer coisa:

- Confirme que a mudança era esperada e que ela não quebra os testes de validação.
- Se o resultado mudou sem motivo, é uma regressão: corrija o código.
- Se a mudança é intencional (por exemplo, uma nova ordem de sorteio ou uma correção
  de estatística), regenere os snapshots:

```bash
UTSIM_BLESS=1 cargo test -p utsim-model --test golden
```

Depois disso, **revise o diff de `tests/golden/`** e explique a mudança na mensagem
do commit, dizendo quais métricas mudaram, de quanto, e por quê. Um commit que
regenera snapshots sem explicação não deve ser aprovado.

Os snapshots são gerados e comparados no Windows. Como libm pode dar resultados de
ponto flutuante diferentes em outros sistemas, o job Linux do CI roda com
`--skip golden`: ele checa portabilidade e lints, mas não compara snapshots.

## Comandos

Testes completos:

```bash
cargo test --workspace --exclude utsim-viewer
```

Só os golden:

```bash
cargo test -p utsim-model --test golden
```

Regenerar os golden (somente após mudança intencional, ver acima):

```bash
UTSIM_BLESS=1 cargo test -p utsim-model --test golden
```

Verificação completa, no Linux ou macOS:

```bash
bash scripts/verificar.sh
```

Verificação completa, no Windows (PowerShell 5.1 ou superior):

```powershell
./scripts/verificar.ps1
```

Os dois scripts rodam, nesta ordem, e param no primeiro passo que falhar:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --exclude utsim-viewer --exclude utsim-godot --all-targets -- -D warnings`
3. `cargo test --workspace --exclude utsim-viewer --exclude utsim-godot`
4. `cargo build --release -p utsim-cli`
5. `utsim check` em cada `examples/*.ron` (arquivos `experimento*` são ignorados)
6. com `UTSIM_VIEWER=1`: clippy e testes do `utsim-viewer`
7. com `UTSIM_GODOT=1`: clippy e testes da ponte `utsim-godot` e o teste sem janela no Godot
   (`scripts/godot_teste.sh`, precisa do Godot 4.7; ver `docs/arquitetura.md`, seção 4r)

## CI

O workflow `.github/workflows/ci.yml` roda em push e pull request:

- **windows-latest:** executa `scripts/verificar.ps1` inteiro, incluindo os golden.
- **ubuntu-latest:** roda clippy e `cargo test --workspace --exclude utsim-viewer -- --skip golden`, para
  checar portabilidade sem comparar snapshots.
- **viewer (ubuntu-latest):** instala as bibliotecas do Bevy e roda clippy e `cargo test -p utsim-viewer`
  (índice da linha do tempo contra `RunReport.series`, layout, carregamento). Localmente: `UTSIM_VIEWER=1 bash scripts/verificar.sh`.
- **godot (ubuntu-latest):** clippy e `cargo test -p utsim-godot` (conversão de coordenadas, escolha
  dos modelos, empacotamento dos quadros), sem o Godot. O teste no Godot roda localmente:
  `UTSIM_GODOT=1 bash scripts/verificar.sh` ou `bash scripts/godot_teste.sh`.

## Testes roteirizados da interface

O `utsim-view` tem um modo roteirizado que abre a janela de verdade, executa um roteiro RON
com entrada sintética (mouse e teclado entram pelas mesmas mensagens do Bevy que a janela
produz, então o editor e o egui reagem como a um usuário) e grava capturas de tela em PNG.
Serve para testar o editor de ponta a ponta sem ninguém na frente da tela.

```bash
utsim-view <modelo.ron | --novo | replay.json> --roteiro <roteiro.ron> --capturas <pasta>
# exemplo
cargo run -p utsim-viewer -- --novo --roteiro crates/utsim-viewer/roteiros/editor_basico.ron --capturas saida/editor_basico
```

- A janela abre em 1280x800 (fator de escala 1) e o roteiro começa depois de ~2,5 s, quando o
  Bevy terminou de compilar os pipelines (antes disso os blocos ainda não aparecem nas capturas).
- Um modelo `.ron` existente é **copiado para a pasta de capturas** e a cópia é a editada, então
  `examples/` nunca é alterado. Para salvar um modelo novo, digite o caminho no diálogo com
  `Digitar("{capturas}/novo.ron")` (`{capturas}` vira a pasta de capturas).
- Saída: `<pasta>/<nome>.png` de cada `Captura`, e `<pasta>/log.txt` com uma linha por passo
  (índice, ação, resultado e o estado do editor: modo, selecionado, ferramenta, nº de objetos,
  ligações e problemas, `sujo`, título da janela, texto de status e tempo simulado).
- Código de saída 0 se todos os passos passaram; 1 no primeiro que falhar, com o número do passo
  e o motivo em `stderr` (e no `log.txt`).

Passos (`Passo`, em `crates/utsim-viewer/src/roteiro.rs`):

| Passo | Efeito |
|---|---|
| `Esperar(quadros: 10)`, `EsperarSegundos(1.5)` | espera quadros ou segundos |
| `EsperarAte(Modo(Simular), 30.0)` | espera uma condição ficar verdadeira (falha no limite, em segundos) |
| `MoverPara(alvo)` | move o ponteiro |
| `Clicar(Esquerdo)`, `Pressionar(..)`, `Soltar(..)` | botões `Esquerdo` ou `Direito` |
| `Arrastar(de: alvo, para: alvo)` | pressiona, move em 8 quadros, espera um quadro no destino e solta. Um bloco (`Objeto`) vale como o ponto do chão sob o seu centro, então o arrasto cai exatamente em `Mundo(x, y)` |
| `Rolar(3.0)` | roda do mouse, em linhas (positivo aproxima) |
| `Tecla("Delete")`, `Atalho(["Ctrl", "Z"])` | teclas por nome: letras, dígitos, `Delete`, `Esc`, `Enter`, `Tab`, `Space`, `Backspace`, setas (`SetaCima`...), `F1`-`F5`, modificadores `Ctrl`/`Shift`/`Alt` |
| `Segurar("Shift")`, `LargarTecla("Shift")` | aperta uma tecla e a mantém apertada até `LargarTecla` (Shift + clique, Shift + arrastar) |
| `Digitar("texto")` | digita no campo egui focado |
| `ClicarBotao("▶ Simular")` | clica no centro do widget com esse rótulo |
| `Captura("nome")` | grava `<pasta>/nome.png` e espera o arquivo existir |
| `Afirmar(condição)` | falha a execução se a condição não valer |
| `IrPara(1806.0)` | pausa e pula para esse tempo simulado (em Simular) |
| `Focar(objeto: "empilhadeira_1", distancia: 9.0, giro: Some(0.6), inclinacao: Some(0.35))` | aponta a câmera para onde o bloco está agora (e volta à órbita); `giro` e `inclinacao` (radianos) são opcionais |
| `Camera(modo: "Seguir", objeto: Some("empilhadeira_1"))` | troca o modo da câmera: `Orbita`, `Seguir`, `PrimeiraPessoa` ou `DoRecurso` (com ou sem acento). `objeto` vale para `Seguir` e `DoRecurso` (sem ele, vale o recurso selecionado ou o primeiro). Só em Simular para esses dois. Espera a câmera deslizar (1,2 s) |
| `Lod(false)`, `Tempos(true)` | liga ou desliga os níveis de detalhe (F4) e o quadro de tempos (F3) |
| `Medir(segundos: 4.0, nome: "visao_geral")` | mede os tempos de quadro por esse tempo (descontado 0,5 s de aquecimento) e grava no `log.txt` e na saída: quadros, média, mediana, p95 e pior quadro. Defina `UTSIM_SEM_VSYNC=1` para não limitar ao monitor |

Alvos: `Objeto("fila")` (centro do bloco), `Mundo(x, y)` (chão, coordenadas do modelo),
`Tela(px, py)` (pixels lógicos da janela), `Botao("rótulo")` (texto egui visível) e
`Deslocado(alvo, dx, dy)` (outro alvo deslocado em pixels, para alcançar o campo ao lado de um rótulo).
`Objeto` e `Mundo` são projetados com a câmera ativa, então os roteiros sobrevivem a mudanças de câmera.

Condições: `ObjetosIgual(n)`, `LigacoesIgual(n)`, `ProblemasIgual(n)`, `Selecionado(Some("fila"))`,
`Modo(Editar | Simular)`, `Sujo(true)`, `Ferramenta("Selecionar" | "Colocar:Queue" | "Ligar:fila")`,
`ObjetoExiste("x")`, `PosicaoDe("fila", (x, y))` (tolerância 0,05), `PosicaoPerto("fila", (x, y), tol)`,
`StatusContem("texto")`, `TituloContem("texto")`, `TempoSimuladoMin(t)` e `Nao(condição)`.
Para o painel de fluxos: `FluxosIgual(n)`, `AtividadesIgual("fluxo", n)`,
`AtividadeSelecionada(Some("decide"))` e `PainelFluxos(true)`. Os nomes das atividades são textos
desenhados no diagrama, então `ClicarBotao("decide")` seleciona a caixa.
Para a seleção, a área de transferência e a aparência (fase 9): `SelecionadosIgual(n)`,
`SelecaoContem("fila")`, `AtivoDe("processo", Some("bancada_kenney"))` (`None`: aparência padrão),
`ObjetosComPrefixo("fila", n)` e `Ligado("fila_2", "processo_2")`.
Para as câmeras e o LOD (A3D-4): `CameraModo("Primeira pessoa")`, `CameraAlvo(Some("empilhadeira_1"))`
(recurso que Seguir ou Do recurso acompanham), `CameraAlturaEntre(1.55, 1.65)` (altura da câmera, em
metros), `CameraPertoDe("empilhadeira_1", 14.0)` (a câmera está a até essa distância do bloco, em Simular)
e `LodAtivo(true)`. A primeira pessoa anda com `Segurar("W")`, `EsperarSegundos(2.0)` e `LargarTecla("W")`,
e olha com `Pressionar(Direito)`, `MoverPara(...)` e `Soltar(Direito)`.

**Como os botões são achados.** O egui 0.36 usado aqui não tem a árvore de acessibilidade
(sem `accesskit`), então, depois de cada passe do egui, o executor guarda todos os textos
desenhados no quadro com seus retângulos (lendo `EguiFullOutput`). `Botao("rótulo")` e
`ClicarBotao("rótulo")` escolhem o texto visível mais ao topo igual ao rótulo (ou, se não houver,
que o contenha) e miram o centro dele, que cai dentro do widget dono do texto (botão, rótulo
selecionável, link, caixa de seleção). Rótulos repetidos na tela: o último desenhado vence; use
`Deslocado` ou `Tela` para os demais.

Os roteiros de exemplo estão em `crates/utsim-viewer/roteiros/`: `editor_basico.ron` (`--novo`),
`propriedades.ron` (com `examples/mm1.ron`), `simular_paletes.ron` (com `examples/montagem_paletes.ron`) e
`esteiras_e_armazem.ron` (com `examples/linha_com_esteiras.ron --stop 700` e com `examples/armazem.ron --stop 400`:
itens sobre a correia, acúmulo no fim e porta-paletes enchendo). As posições na correia são reconstruídas do
rastro (`crates/utsim-scene/src/handling.rs`) e testadas em `crates/utsim-scene/tests/handling.rs`.
Fluxos de tokens e scripts (fase 6): `fluxos.ron` (com `roteiros/fluxos_modelo.ron`: painel Fluxos, diagrama,
atividade selecionada, expressão com erro em linha, novo fluxo, desfazer, `on_entry` e regra de saída por
script) e `fluxos_simular.ron` (com `examples/fluxo_docas.ron`: contagem de tokens ao vivo no diagrama em
Simular). O botão "Fluxos" da barra tem o mesmo rótulo do título do painel, então os roteiros o clicam por
`Tela(707.0, 12.0)`. `Digitar` digita maiúsculas e espaços como estão.
Fluidos e AGVs (fase 7): `fluidos_tanques.ron` (com `examples/tanques.ron`: tanques enchendo e esvaziando,
seleção pela lista e por clique, painel com nível e taxa) e `agvs.ron` (com `examples/agvs.ron`: aresta com
capacidade em vermelho, AGVs andando e um AGV bloqueado esperando a pista única).
Modelos 3D realistas (A3D-2): `visual_realista_armazem.ron` (com `examples/centro_de_distribuicao.ron`:
empilhadeira levando o palete nos garfos, garfos subindo até a célula do porta-paletes em t = 1806 e o
botão "Visual: simples / realista"), `visual_realista_agvs.ron` (com `examples/agvs.ron`: AGVs virados
para onde andam e a luz de estado verde, laranja e azul) e `visual_realista_operadores.ron` (com
`examples/celula_com_operadores.ron`: pernas alternando na caminhada e o operador trabalhando na frente
da máquina). Grave as capturas fora do repositório, por exemplo em `%TEMP%\utsim_visual\<nome>`.
Câmeras e modelos grandes (A3D-4): `a3d4_cameras.ron` (com `examples/centro_de_distribuicao.ron`: a empilhadeira
selecionada na lista e o botão "Seguir este recurso", a câmera por trás e por cima enquanto ela anda, a roda
aproximando, `Esc` voltando à órbita, a primeira pessoa pelo menu "Câmera" da barra andando com `W` e
`Shift` e olhando com o botão direito sempre a 1,6 m, o assento do motorista com a tecla `4` e `F`
enquadrando de volta) e `a3d4_lod.ron` (com um modelo de `utsim gerar-modelo --objetos 500 --tipo armazem`:
visão geral e de perto com o LOD ligado e desligado, com os tempos de quadro no `log.txt`; os números
estão em `docs/desempenho.md`). Grave as capturas fora do repositório, por exemplo em `%TEMP%\utsim_a3d4\<nome>`.
Acabamento da interface (piloto): `acabamento_interface.ron`, com `roteiros/acabamento_modelo.ron` (três inspetores no mesmo ponto
e uma fila com três saídas): aba Objetos da esquerda com filtro por nome e seleção pela lista (também com `Shift`), campos
X, Y e Giro do painel digitados e desfeitos, clique repetido no mesmo ponto alternando entre os objetos sobrepostos (dica ao lado do
cursor) e um arrasto que move o objeto destacado, ordem das ligações trocada com "Descer" e desfeita, "Salvar como" por
`Ctrl+Shift+S` com um caminho de maiúsculas, `Focar` em Editar, zoom da roda em direção ao cursor, a lista de famílias de
distribuição inteira e o texto inválido de "editar como texto" mantido em vermelho. Condições novas: `GiroDe(obj, graus)`,
`PortaDe(obj, porta, destino)` e `EntradaDe(obj, ordem, origem)` (a partir de 1), `PilhaSobCursor(n)`, `ListaObjetos(n)`,
`AbaEsquerda("Biblioteca" | "Objetos")` e `CaminhoTermina(texto)`. Os campos do painel são alcançados com
`Deslocado(Botao("X"), 30.0, 0.0)`; "Subir" e "Descer" ficam à direita do rótulo da ligação
(`Deslocado(Botao("porta 1: fila -> insp_a"), 135.0, 0.0)` é o "Descer" da porta 1).
O teste `crates/utsim-viewer/tests/glyphs.rs` confere que todo símbolo escrito nos painéis existe na fonte padrão do egui.
Editor para montar modelos (fase 9), ambos com `--novo`: `editor_biblioteca.ron` (colocar pela
biblioteca com clique e arrastando até o chão, encaixe na grade, seleção por caixa com Shift, mover
dois juntos, copiar e colar com nomes únicos e só a ligação interna, desfazer, Shift + clique,
alinhar pelo topo, duplicar, excluir e a janela de atalhos) e `editor_aparencia.ron` (ativos de
terceiros escolhidos pela busca da seção "Aparência": bancada num processo, máquina da Kenney,
pessoa e empilhadeira CC-BY, no modo realista e no simples; "Restaurar padrão" e desfazer). A seção
"Aparência" fica no fim do painel de propriedades: o roteiro rola o painel com `Rolar(-30.0)` sobre ele.
Quando um rótulo da biblioteca também aparece no painel da direita (por exemplo "Processo"), coloque
vários com Shift em vez de clicar de novo no item.
Camadas Rede e Fluidos (fase 9), ambos com `--novo`: `editor_rede.ron` (linha com duas máquinas e
uma empilhadeira, anel desenhado com a ferramenta Aresta, arrastar, renomear, excluir e desfazer um
nó, capacidade de uma aresta, base da empilhadeira num nó e a empilhadeira seguindo o anel em
Simular) e `editor_fluidos.ron` (fonte, tanque, tubulação e sumidouro ligados, ciclo apontado como
problema, vazão e capacidade pelo painel, excluir e desfazer, e o tanque enchendo em Simular).
Condições dessas camadas: `Camada("Rede")`, `NosIgual(n)`, `ArestasIgual(n)`, `Aresta("a", "b")`,
`NoEm("a", (x, y))`, `SelecaoCamada(Some("no:a" | "aresta:a->b" | "fluido:tanque"))`,
`NosFluidoIgual(n)`, `LigacoesFluidoIgual(n)`, `AvisosIgual(n)` e `NivelEntre("tanque", min, max)`;
as ferramentas aparecem como `Rede:No`, `Rede:Aresta[:nó]`, `Fluido:Tank` e `Fluido:Ligar[:nó]`.
Janela Resultados (fase 10): `resultados.ron` (com `examples/tutorial_02_linha.ron`: 5 replicações, gargalo na
`furacao`, tabela ordenada pela ocupação, os quatro gráficos e exportação de CSV e HTML para a pasta ao lado da
cópia do modelo) e `cenarios.ron` (com `examples/servidores.ron`: coluna `maquina.capacity`, cenário com 3
servidores, comparação com a diferença pareada, gráfico de uma métrica e o experimento salvo em `.ron`).
Condições: `PainelResultados(true)`, `AbaResultados("Resumo" | "Gráficos" | "Cenários" | "Otimização")`,
`AnaliseRodando(false)`, `ReplicacoesNoResultado(n)`, `Gargalo(Some("furacao"))`, `CenariosComparados(n)`,
`Exportado("resumo.csv")` (algum arquivo gravado pelas exportações termina com o texto) e
`MelhorPontoContem("maquina.capacity = 3")`. Para editar um número (`DragValue`), clique nele, use
`Atalho(["Ctrl", "A"])`, `Digitar` e `Tecla("Enter")`.
A análise do formato e das condições (sem janela) é testada em `cargo test -p utsim-viewer`.

Na interface, os painéis (barra, paleta, propriedades, simulação) são acoplados e não cobrem a
cena: a câmera enquadra o modelo na área livre ao abrir, ao entrar em Simular, após novo modelo e
com a tecla `F` ou o botão "Enquadrar". `Objeto`/`Mundo` continuam projetados com a câmera ativa.
