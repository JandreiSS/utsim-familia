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
cargo test --workspace
```

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
| `Source` | `interarrival`, `first_at_zero`, `max_arrivals`, `labels: { "nome": Dist }` |
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
examples/         modelos de exemplo
docs/             requisitos (RF/RNF), arquitetura, roteiro, processo e testes
```

## Licença

UTSim é software livre, distribuído sob a **GNU General Public License, versão 3 ou
posterior** (`GPL-3.0-or-later`). Veja o arquivo [LICENSE](LICENSE).

Você pode usar, estudar, modificar e redistribuir o UTSim, inclusive comercialmente.
Quem distribuir uma versão modificada precisa mantê-la sob a mesma licença e entregar
o código-fonte. Modelos `.ron` e resultados de simulação feitos com o UTSim pertencem
a quem os criou e não são afetados pela licença.
