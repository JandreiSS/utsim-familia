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
   fila que atende na ordem errada.

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
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace --exclude utsim-viewer`
4. `cargo build --release -p utsim-cli`
5. `utsim check` em cada `examples/*.ron` (arquivos `experimento*` são ignorados)

## CI

O workflow `.github/workflows/ci.yml` roda em push e pull request:

- **windows-latest:** executa `scripts/verificar.ps1` inteiro, incluindo os golden.
- **ubuntu-latest:** roda clippy e `cargo test --workspace --exclude utsim-viewer -- --skip golden`, para
  checar portabilidade sem comparar snapshots.
- **viewer (ubuntu-latest):** instala as bibliotecas do Bevy e roda clippy e `cargo test -p utsim-viewer`
  (índice da linha do tempo contra `RunReport.series`, layout, carregamento). Localmente: `UTSIM_VIEWER=1 bash scripts/verificar.sh`.

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
| `Arrastar(de: alvo, para: alvo)` | pressiona, move em 8 quadros e solta |
| `Rolar(3.0)` | roda do mouse, em linhas (positivo aproxima) |
| `Tecla("Delete")`, `Atalho(["Ctrl", "Z"])` | teclas por nome: letras, dígitos, `Delete`, `Esc`, `Enter`, `Tab`, `Space`, `Backspace`, setas (`SetaCima`...), `F1`-`F5`, modificadores `Ctrl`/`Shift`/`Alt` |
| `Segurar("Shift")`, `LargarTecla("Shift")` | aperta uma tecla e a mantém apertada até `LargarTecla` (Shift + clique, Shift + arrastar) |
| `Digitar("texto")` | digita no campo egui focado |
| `ClicarBotao("▶ Simular")` | clica no centro do widget com esse rótulo |
| `Captura("nome")` | grava `<pasta>/nome.png` e espera o arquivo existir |
| `Afirmar(condição)` | falha a execução se a condição não valer |
| `IrPara(1806.0)` | pausa e pula para esse tempo simulado (em Simular) |
| `Focar(objeto: "empilhadeira_1", distancia: 9.0, giro: Some(0.6), inclinacao: Some(0.35))` | aponta a câmera para onde o bloco está agora; `giro` e `inclinacao` (radianos) são opcionais |

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
`Tela(611.0, 12.0)`. `Digitar` não digita espaços (use `2+x`, sem espaços).
Fluidos e AGVs (fase 7): `fluidos_tanques.ron` (com `examples/tanques.ron`: tanques enchendo e esvaziando,
seleção pela lista e por clique, painel com nível e taxa) e `agvs.ron` (com `examples/agvs.ron`: aresta com
capacidade em vermelho, AGVs andando e um AGV bloqueado esperando a pista única).
Modelos 3D realistas (A3D-2): `visual_realista_armazem.ron` (com `examples/centro_de_distribuicao.ron`:
empilhadeira levando o palete nos garfos, garfos subindo até a célula do porta-paletes em t = 1806 e o
botão "Visual: simples / realista"), `visual_realista_agvs.ron` (com `examples/agvs.ron`: AGVs virados
para onde andam e a luz de estado verde, laranja e azul) e `visual_realista_operadores.ron` (com
`examples/celula_com_operadores.ron`: pernas alternando na caminhada e o operador trabalhando na frente
da máquina). Grave as capturas fora do repositório, por exemplo em `%TEMP%\utsim_visual\<nome>`.
Editor para montar modelos (fase 9), ambos com `--novo`: `editor_biblioteca.ron` (colocar pela
biblioteca com clique e arrastando até o chão, encaixe na grade, seleção por caixa com Shift, mover
dois juntos, copiar e colar com nomes únicos e só a ligação interna, desfazer, Shift + clique,
alinhar pelo topo, duplicar, excluir e a janela de atalhos) e `editor_aparencia.ron` (ativos de
terceiros escolhidos pela busca da seção "Aparência": bancada num processo, máquina da Kenney,
pessoa e empilhadeira CC-BY, no modo realista e no simples; "Restaurar padrão" e desfazer). A seção
"Aparência" fica no fim do painel de propriedades: o roteiro rola o painel com `Rolar(-30.0)` sobre ele.
Quando um rótulo da biblioteca também aparece no painel da direita (por exemplo "Processo"), coloque
vários com Shift em vez de clicar de novo no item.
A análise do formato e das condições (sem janela) é testada em `cargo test -p utsim-viewer`.

Na interface, os painéis (barra, paleta, propriedades, simulação) são acoplados e não cobrem a
cena: a câmera enquadra o modelo na área livre ao abrir, ao entrar em Simular, após novo modelo e
com a tecla `F` ou o botão "Enquadrar". `Objeto`/`Mundo` continuam projetados com a câmera ativa.
