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
