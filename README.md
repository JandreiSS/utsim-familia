# UTSim

[![CI](https://github.com/JandreiSS/utsim/actions/workflows/ci.yml/badge.svg)](https://github.com/JandreiSS/utsim/actions/workflows/ci.yml)

Simulador de processos produtivos por **eventos discretos**, escrito em Rust, livre e
com visualização 3D. A implementação é própria e o comportamento é especificado por
nós, em [docs/arquitetura.md](docs/arquitetura.md).

> **Status: fase 0.** O motor de eventos, os blocos Source, Queue, Processor e Sink,
> o formato de modelo e a CLI estão prontos e validados contra a teoria das filas.
> Ainda não há 3D nem editor. Veja o [roteiro](docs/roadmap.md) e os [requisitos](docs/requisitos.md).

## Manual do usuário

O [Manual do usuário](docs/manual/src/SUMMARY.md) explica cada objeto, os recursos móveis, os resultados e a estatística, e traz seis tutoriais passo a passo (`examples/tutorial_*.ron`). É um livro [mdBook](https://rust-lang.github.io/mdBook/) (`cd docs/manual && mdbook build`), mas todos os capítulos são arquivos Markdown legíveis direto.

## Instalação

Os pacotes prontos ficam na página de [Releases](https://github.com/JandreiSS/utsim/releases).

**Windows**: baixe `utsim-v<versão>-windows-x64-instalador.exe` e abra. A instalação é por usuário, sem administrador, em `%LOCALAPPDATA%\Programs\UTSim`. Ela traz `utsim.exe` e `utsim-view.exe` (em `bin\`), `examples\`, `assets\` (modelos 3D e ativos de terceiros, com as licenças), `manual\` (HTML), `README.md` e `CHANGELOG.md`. Cria a pasta "UTSim" no Menu Iniciar ("UTSim" e "UTSim (exemplos)") e, se você marcar a opção, põe `utsim` no `PATH`. Desinstale em Configurações > Aplicativos. O instalador ainda não é assinado, então o SmartScreen pode avisar. Não há associação de extensão: `.ron` é genérica demais. Abra um modelo com `utsim-view modelo.ron` ou arrastando o arquivo sobre o `utsim-view.exe`. Também há um `.zip` portátil.

**Debian e Ubuntu**: `sudo apt install ./utsim_<versão>_amd64.deb` instala em `/opt/utsim`, com os comandos `utsim` e `utsim-view` em `/usr/bin` e atalhos no menu.

**Outras distribuições**: descompacte `utsim-v<versão>-linux-x64.tar.gz` e rode `./instalar.sh` (sem `sudo`, instala em `~/.local`). `./instalar.sh --desinstalar` remove.

Detalhes e a opção de compilar do código-fonte estão na seção "Instalação" do [manual](docs/manual/src/introducao.md).

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

`utsim check` e `utsim run` mostram `aviso: ...` para problemas que não impedem a execução, em especial distâncias (objetos todos na origem, ligações longas e instantâneas). Para que a distância entre objetos pese no resultado, use `transfer: (speed: 1.0)` em um objeto ou em `run` (tempo de viagem de `delay + distância / speed` nas ligações diretas, em linha reta ou pela rede com `path: Network`); veja `examples/transferencia_por_distancia.ron`.

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

### Cena 3D para outros motores

`utsim cena` grava onde cada bloco, item e recurso móvel está, e o que está fazendo, ao longo de uma rodada.
O formato é JSON em linhas (protocolo `utsim-scene/1`): um cabeçalho com o layout e depois um quadro por linha, em metros, com o eixo Z para cima.
É a mesma cena que o `utsim-view` desenha, então o cliente Godot (3D e realidade virtual/aumentada) mostra exatamente o mesmo.

```bash
cargo run --release -p utsim-cli -- cena examples/agvs.ron --fps 10 --ate 600 --saida agvs.jsonl
```

- `--rep N` escolhe a replicação, `--fps` dá os quadros por unidade de tempo simulado e `--ate T` o último instante; sem `--saida`, escreve na saída padrão.

Ao vivo, durante uma emulação OPC UA (ou sozinha, para desenvolver clientes):

```bash
cargo run --release -p utsim-cli -- emular examples/tutorial_02_linha.ron --cena-porta 7878
cargo run --release -p utsim-cli -- cena examples/agvs.ron --servir 7878 --velocidade 5
cargo run -p utsim-scene --features server --example cliente_cena -- 127.0.0.1:7878 --parar furacao
```

A mesma porta atende TCP (um JSON por linha) e WebSocket; abra `web/cena_ao_vivo.html` no navegador para ver a vista de topo. O cliente pode mandar comandos (`{"tipo":"comando","variavel":"bloco.Parar","valor":true}`). Não há autenticação: por padrão só escuta em `127.0.0.1`.

## No navegador

A simulação, o ajuste de distribuições e o 3D também rodam no navegador (WebAssembly),
sem instalar nada. Para gerar a página:

```bash
bash scripts/web.sh          # ou scripts/web.ps1; --sem-3d pula o visualizador 3D
python3 -m http.server -d web
```

Abra `http://localhost:8000/`, escolha um exemplo ou cole um modelo, e use as abas
Simular, Ajustar distribuição e 3D. O arquivo `web/dist/utsim.html` é autocontido e abre sem servidor,
mas sem a aba 3D. O 3D precisa de um navegador com WebGL2. A primeira compilação do
visualizador leva uns 10 minutos.

## Ajuste de distribuições

A partir de dados reais (tempos de atendimento, intervalos entre chegadas...), o UTSim
testa várias distribuições, mostra todas numa tabela (testes KS, Anderson-Darling e
qui-quadrado, AIC e peso de Akaike), explica a escolha e entrega a linha `Dist` pronta
para o `.ron`:

```bash
cargo run --release -p utsim-cli -- fit examples/dados/tempos_atendimento.csv --html relatorio.html
```

Aceita CSV/TXT com separador `,`, `;` ou tab, cabeçalho e vírgula decimal detectados
automaticamente. Sem `--coluna`, o programa pula as colunas de numeração (1, 2, 3...), diz
qual coluna usou e por quê, e pede `--coluna` se restar mais de uma plausível. Opções: `--coluna <nome|n>`, `--timestamps` (a coluna traz instantes e o
ajuste é feito sobre os intervalos), `--alpha 0.05`, `--bootstrap 300`, `--seed 1`,
`--html relatorio.html` (histograma, Q-Q, P-P e ECDF) e `--json saida.json`.

## Emulação (OPC UA)

O UTSim pode rodar um modelo **em tempo real** e expô-lo por um servidor OPC UA, para
comissionamento virtual: um CLP, um SCADA ou qualquer cliente OPC UA lê o estado dos blocos
e comanda paradas como se fosse a planta.

```bash
cargo run --release -p utsim-cli -- emular examples/linha_com_bloqueio.ron --velocidade 10 --porta 4840 --gravar comandos.ron
# Servidor OPC UA em opc.tcp://127.0.0.1:4840/
```

Opções: `--porta` (padrão 4840; 0 escolhe uma porta livre), `--velocidade` (1.0 = tempo
real, 10.0 = dez vezes mais rápido; uma unidade de tempo do modelo vale 1 s de relógio na
velocidade 1.0), `--ate T` (tempo simulado final; padrão `run.stop_time`), `--gravar f.ron`
(grava os comandos recebidos) e `--reproduzir f.ron` (reaplica os comandos nos mesmos tempos
simulados, sem OPC UA e sem esperar o relógio; produz o mesmo relatório). Ctrl-C encerra e
grava o log.

**Espaço de endereços** (namespace `urn:utsim:emulacao`, nós de texto, endpoint sem
segurança e com acesso anônimo; use só em rede confiável):

| Nó (`UTSim/<modelo>/...`) | Tipo | Acesso | Significado |
|---|---|---|---|
| `TempoSimulado` | Double | leitura | tempo simulado atual |
| `Velocidade` | Double | leitura/escrita | fator de velocidade (maior que 0) |
| `<bloco>/Conteudo` | UInt32 | leitura | itens dentro do bloco |
| `<bloco>/Estado` | String | leitura | estado atual (`idle`, `processing`, `stopped`...) |
| `<bloco>/Entradas` | UInt64 | leitura | itens que entraram |
| `<bloco>/Saidas` | UInt64 | leitura | itens que saíram |
| `<bloco>/Parar` | Boolean | leitura/escrita | `true` para o bloco, `false` o retoma |

Os valores são atualizados a cada tick (50 ms) e as escritas são aplicadas no tick
seguinte, no tempo simulado corrente; a simulação continua num só thread e determinística
para a mesma sequência de comandos. Exemplo com um cliente genérico (UaExpert ou
`opcua-commander`): conecte em `opc.tcp://127.0.0.1:4840/`, expanda `Objects > UTSim >
<modelo>`, assine `fila/Conteudo` e `maquina/Estado` e escreva `true` em `maquina/Parar`:
o estado muda para `stopped` e a fila passa a crescer.

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
| `Source` | `interarrival`, `first_at_zero`, `max_arrivals`, `labels: { "nome": Dist }`, `on_blocked: Wait \| Drop` (`Drop` descarta o item que não consegue sair e publica `lost.count`, `entered.count` e `loss_fraction`) |
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
  utsim-emu/      emulação em tempo real e servidor OPC UA (só nativo, fora do build wasm)
  utsim-scene/    cena 3D independente de motor: poses e movimentos de tudo em cada instante (`utsim cena`)
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

Ainda não definida.

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
tabelas. Perdas servem de objetivo e restrição por `<fonte>.lost.count`, `<fonte>.loss_fraction`
e os totais `model.lost.count` e `model.loss_fraction`. A ocupação (`<objeto>.utilization`,
`.utilization_in_shift`, `.utilization_excl_blocked`, `.utilization_incl_blocked`) e o tempo
entre dois pontos do fluxo (`spans`, métrica `<medicao>.value.mean`) também têm nome estável.
