# UTSim

Simulador de processos produtivos por **eventos discretos**, escrito em Rust, livre e
com visualização 3D planejada. A inspiração conceitual é a família de simuladores
industriais como ferramentas comerciais. A implementação é própria (clean room) e
o comportamento é especificado por nós, em [docs/arquitetura.md](docs/arquitetura.md).

> **Status: fase 0.** O motor de eventos, os blocos Source, Queue, Processor e Sink,
> o formato de modelo e a CLI estão prontos e validados contra a teoria das filas.
> Ainda não há 3D nem editor. Veja o [roteiro](docs/roadmap.md) e os [requisitos](docs/requisitos.md).

## Rodando

```bash
cargo run --release -p utsim-cli -- run examples/mm1.ron
```

```text
M/M/1: 10 replicação(ões), estatísticas de t=2000 até t=100000

objeto     tipo        ...  conteúdo médio   permanência média
fila       queue       ...  3.2147 ± 0.1019  3.2152 ± 0.0975     (teoria: 3,2 e 3,2)
maquina    processor   ...  0.8000 ± 0.0020  0.8001 ± 0.0015
maquina    idle 20.00% | processing 80.00%                       (teoria: ρ = 0,8)
saida      time_in_system 4.0153 ± 0.0981                        (teoria: W = 4,0)
```

Outros comandos:

```bash
cargo run --release -p utsim-cli -- run examples/linha_com_bloqueio.ron -n 20 --json resultado.json
```

```bash
cargo run --release -p utsim-cli -- check examples/roteamento_por_tipo.ron
```

Séries temporais, histogramas e exportação CSV:

```bash
cargo run --release -p utsim-cli -- run examples/mm1.ron --sample 100 --bins 20 --csv saida/
```

- `--sample <intervalo>` (ou `run.sample_interval`) registra conteúdo e estado de cada objeto a cada intervalo, a partir do aquecimento.
- `--bins <n>` (ou `run.histogram_bins`) gera histogramas da permanência e das métricas extras (ex.: `time_in_system`).
- `--csv <dir>` grava `resumo.csv`, `replicacoes.csv` e, quando existem, `series.csv` e `histogramas.csv`.

```bash
cargo test --workspace --exclude utsim-viewer
```

## Editor e visualização 3D

O `utsim-view` tem dois modos: **Editar** (monta e altera o modelo em 3D) e **Simular** (anima uma replicação gravada).
A simulação nunca é conduzida pela janela: o botão "▶ Simular" roda o motor em outra thread e a janela só **lê** o replay resultante.
Por trazer o Bevy, o binário não entra no build padrão do workspace; compile-o explicitamente.

```bash
# edita um modelo existente (ou cria o arquivo no primeiro Ctrl+S, se não existir)
cargo run --release -p utsim-viewer -- examples/montagem_paletes.ron

# começa do zero
cargo run --release -p utsim-viewer -- --novo

# abre um replay gravado (somente leitura, só o modo Simular)
cargo run --release -p utsim-cli -- replay examples/mm1.ron --out replay.json
cargo run --release -p utsim-viewer -- replay.json

# verifica sem abrir janela (carrega, valida, simula e imprime um resumo)
cargo run --release -p utsim-viewer -- examples/mm1.ron --headless-check
```

- Opções: `--stop T` (tempo máximo das simulações rápidas; padrão: o menor entre `run.stop_time` e 2000, editável na barra superior), `--transit D` (duração visual das transferências), `--rep N` (replicação, para `.json`).
- Sem posições no modelo (tudo em 0,0,0), o layout automático é aplicado uma vez ao abrir, como um passo desfazível que vai para o arquivo ao salvar. Convenção: posição do modelo `(x, y, z)` vira Bevy `(x, z, -y)`, com `z` como altura.

| Modo | Ação | Como |
|---|---|---|
| Editar | Adicionar bloco | escolha o tipo na paleta e clique no chão (grade de 1 unidade) |
| Editar | Selecionar / mover | clique no bloco / arraste-o no chão (um arrasto = um passo de desfazer) |
| Editar | Ligar blocos | com um bloco selecionado, `C` ou "Ligar" e clique no destino |
| Editar | Remover ligação | clique na seta, ou `✕` na lista de ligações do painel |
| Editar | Excluir bloco | `Delete` |
| Editar | Desfazer / refazer | `Ctrl+Z` / `Ctrl+Y` (ou `Ctrl+Shift+Z`) |
| Editar | Salvar / novo | `Ctrl+S` (pede um nome se não houver arquivo) / `Ctrl+N` |
| Editar | Cancelar ferramenta | `Esc` |
| Editar | Problemas do modelo | listados no painel; blocos com problema ficam com contorno vermelho |
| Simular | Reproduzir / pausar | `Espaço` |
| Simular | Velocidade | `+` / `-` |
| Simular | Evento anterior / seguinte | `←` / `→` |
| Ambos | Câmera | botão esquerdo no vazio arrasta, direito gira, roda aproxima |

- Verificação do visualizador: `UTSIM_VIEWER=1 bash scripts/verificar.sh` (clippy e testes de `utsim-viewer`; no Linux, o Bevy pede `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`).

## Ajuste de distribuições

A partir de dados reais (tempos de atendimento, intervalos entre chegadas...), o UTSim
testa várias distribuições, mostra todas numa tabela (testes KS, Anderson-Darling e
qui-quadrado, AIC e peso de Akaike), explica a escolha e entrega a linha `Dist` pronta
para o `.ron`:

```bash
cargo run --release -p utsim-cli -- fit examples/dados/tempos_atendimento.csv --html relatorio.html
```

Aceita CSV/TXT com separador `,`, `;` ou tab, cabeçalho e vírgula decimal detectados
automaticamente. Opções: `--coluna <nome|n>`, `--timestamps` (a coluna traz instantes e o
ajuste é feito sobre os intervalos), `--alpha 0.05`, `--bootstrap 300`, `--seed 1`,
`--html relatorio.html` (histograma, Q-Q, P-P e ECDF) e `--json saida.json`.

## Formato do modelo

Os modelos são arquivos de texto [RON](https://github.com/ron-rs/ron): legíveis,
aceitam comentários e são fáceis de versionar no Git.

```ron
Model(
    name: "M/M/1",
    run: (stop_time: 100000.0, warmup: 2000.0, seed: 42, replications: 10),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
        (name: "fila",    kind: Queue(capacity: 20)),
        (name: "maquina", kind: Processor(setup: Const(0.1), process: Exp(mean: 0.8))),
        (name: "saida",   kind: Sink),
    ],
    connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
)
```

| Objeto | Campos |
|---|---|
| `Source` | `interarrival`, `first_at_zero`, `max_arrivals`, `labels: { "nome": Dist }`, `on_blocked: Wait \| Drop` (`Drop` descarta e conta em `lost` o item que não consegue sair) |
| `Queue` | `capacity` (vazio = ilimitada), `discipline: Fifo \| Lifo` |
| `Processor` | `capacity` (padrão 1), `setup` (opcional), `process` |
| `Sink` | registra `time_in_system` e destrói o item |

Em qualquer objeto: `output: FirstAvailable | RoundRobin | Random | ByLabel("rótulo")`
e `position: (x, y, z)`, que será usada pela visualização 3D.

Distribuições: `Const(v)`, `Uniform(min, max)`, `Exp(mean)`, `Triangular(min, mode, max)`,
`Normal(mean, sd)`, `LogNormal(mean, sd)`, `Gamma(shape, scale)`, `Erlang(k, mean)`,
`Weibull(shape, scale)` e `Discrete([(valor, peso), ...])`.

## Organização

```
crates/
  utsim-core/     relógio, lista de eventos, itens, RNG, estatísticas, protocolo de fluxo
  utsim-objects/  Source, Queue, Processor, Sink
  utsim-model/    formato .ron, validação, replicações em paralelo (rayon)
  utsim-cli/      binário `utsim`
  utsim-env/      ambiente de aprendizado por reforço (estilo Gym) e Q-learning tabular
  utsim-py/       ligações Python do ambiente (PyO3), fora do workspace principal
  utsim-viewer/   binário `utsim-view`: editor e visualizador 3D (Bevy); a simulação roda no motor, a janela só lê o replay
examples/         modelos de exemplo
docs/             requisitos (RF/RNF), arquitetura, roteiro, processo e testes
```

## Aprendizado por reforço

Um modelo pode **pausar e perguntar a um agente** o que fazer: no fluxo de tokens, a
atividade `Decide` com regra `Agent(observation: [...])` para a simulação, entrega a
observação e espera a escolha de um ramo. A crate `utsim-env` transforma isso num ambiente
no estilo Gym:

```rust
use utsim_env::{Env, EnvConfig, RewardTerm};

let modelo = utsim_model::Model::load("examples/roteamento_rl.ron")?;
let config = EnvConfig {
    // recompensa = -(área sob o número de itens na fila rápida) entre decisões
    reward: vec![RewardTerm::integral("fila_rapida.content.mean", -1.0)],
    observe_blocks: vec!["fila_rapida+maquina_rapida".into(), "fila_lenta+maquina_lenta".into()],
    episode_length: Some(100.0),
    ..EnvConfig::default()
};
let mut env = Env::new(modelo, config)?;
let mut obs = env.reset(1)?;              // roda até a primeira decisão
while obs.options > 0 {
    let r = env.step(0)?;                 // responde e roda até a próxima
    obs = r.observation;
    if r.done { break; }
}
```

- **Observação:** o vetor do `Decide`, o conteúdo dos blocos de `observe_blocks` (`"a+b"`
  soma) e as métricas de `observe_metrics`.
- **Recompensa:** Σ peso·Δmétrica entre decisões, menos `decision_cost`. Com
  `RewardTerm::integral` (média no tempo × tempo decorrido) a recompensa é a área acumulada;
  com peso −1 nos conteúdos dos blocos, o retorno do episódio é o oposto da soma dos
  tempos no sistema.
- **Linha de base:** `utsim_env::qlearning` traz um Q-learning tabular (faixas por
  dimensão, ε-guloso). No exemplo `examples/roteamento_rl.ron` (linha rápida de 1 máquina ×
  linha lenta de 2 máquinas), 1000 episódios de treino (~9 s em debug) bastam para vencer a
  política aleatória e ficar a ~4% da heurística "menor atraso esperado"
  (`cargo test -p utsim-env --test learning -- --nocapture` mostra os números).
- **Python:** `crates/utsim-py` expõe `utsim.Env` (listas e dicionários simples, sem numpy).
  Compile à parte, pois exige Python: `cd crates/utsim-py && maturin develop --release`
  (ou `cargo build --release` e renomeie `utsim.dll`/`libutsim.so` para `utsim.pyd`/`utsim.so`).
  Depois rode `python crates/utsim-py/python/exemplo_gym.py`.

## Licença

UTSim é software livre, distribuído sob a **GNU General Public License, versão 3 ou
posterior** (`GPL-3.0-or-later`). Veja o arquivo [LICENSE](LICENSE).

Você pode usar, estudar, modificar e redistribuir o UTSim, inclusive comercialmente.
Quem distribuir uma versão modificada precisa mantê-la sob a mesma licença e entregar
o código-fonte. Modelos `.ron` e resultados de simulação feitos com o UTSim pertencem
a quem os criou e não são afetados pela licença.

## Otimização

`utsim otimizar` procura os valores de parâmetros do modelo (número de servidores,
capacidade de fila, tempos de serviço) que minimizam ou maximizam um objetivo, respeitando
restrições. O problema é um arquivo RON (veja `examples/otimizacao_servidores.ron`):

```bash
utsim otimizar examples/otimizacao_servidores.ron --html relatorio.html
# opções: --json saida.json  --reps N  --orcamento N
```

Todos os pontos usam as mesmas sementes, então a comparação é pareada: o relatório mostra
a diferença de cada ponto para o melhor (Δ ± intervalo de 95
## Otimização

`utsim otimizar` procura os valores de parâmetros do modelo (número de servidores,
capacidade de fila, tempos de serviço) que minimizam ou maximizam um objetivo, respeitando
restrições. O problema é um arquivo RON (veja `examples/otimizacao_servidores.ron`):

```bash
utsim otimizar examples/otimizacao_servidores.ron --html relatorio.html
# opções: --json saida.json  --reps N  --orcamento N
```

Todos os pontos usam as mesmas sementes, então a comparação é pareada: o relatório mostra
a diferença de cada ponto para o melhor (Δ ± intervalo de 95%), marca com `≈` os que não
se distinguem estatisticamente e imprime o cenário (`set: { ... }`) pronto para colar em
um arquivo de experimento. O `--html` inclui gráficos do objetivo por ordem de avaliação
e dos 10 melhores pontos.

Observações: com `refine`, os melhores pontos são re-simulados com mais replicações e a
comparação final (melhor e equivalentes) usa essa lista; o relatório mostra as duas
tabelas. As métricas `<bloco>.lost.*` ainda não servem de objetivo (só `.mean` é exposto e
vale sempre 1); use, por exemplo, as entradas da Sink com peso negativo.
