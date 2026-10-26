# Processo de desenvolvimento

Como o UTSim é construído: quem faz o quê, em que ordem e como garantimos que nada quebra.
Vale para pessoas e para agentes de IA. O roteiro de funcionalidades está em
[roadmap.md](roadmap.md), e a especificação de comportamento em [arquitetura.md](arquitetura.md).

## 1. Ciclo de cada fase

1. **Contrato primeiro.** Antes de dividir o trabalho, o coordenador define as mudanças
   no núcleo (`utsim-core`) que a fase exige: traits, hooks do `Ctx`, tipos novos. Elas
   entram num commit único, com testes, e atualizam `arquitetura.md`.
2. **Divisão em frentes independentes.** Cada frente tem arquivos próprios e não depende
   do resultado de outra. Frentes que precisam mexer no mesmo arquivo recebem a instrução
   de mexer o mínimo (ex.: só registrar um subcomando no `main.rs`).
3. **Execução isolada.** Cada frente roda num branch/worktree próprio, criado a partir
   do commit do contrato.
4. **Revisão e integração.** O coordenador lê o diff, integra com `merge --no-ff`
   (um merge por frente), resolve conflitos e roda a verificação completa (seção 3).
5. **Fechamento.** Atualizar [requisitos.md](requisitos.md) (RF/RNF: situação e teste
   que comprova), `roadmap.md` (marcar itens), `arquitetura.md` (semântica nova) e os
   snapshots golden, se houve mudança intencional de comportamento.

## 2. Quem faz o quê

| Tipo de tarefa | Executor | Exemplos |
|---|---|---|
| Decisões de design, contrato do núcleo, revisão, integração | Coordenador (Opus) | eventos canceláveis, quebras, protocolo de fluxo |
| Funcionalidade bem especificada, com testes | Sonnet | Combiner/Separator, CSV, cenários |
| Tarefa mecânica ou de teste, com especificação fechada | Haiku | testes contra teoria, snapshots, scripts |

Toda instrução enviada a um agente contém: arquivos que pode e não pode tocar, critério
de pronto (testes que devem passar), mensagem de commit e o que relatar no fim. Um
agente nunca "afrouxa" a tolerância de um teste para fazê-lo passar: se o número está
longe da teoria, ele para e relata.

## 3. Verificação ("nada quebrou")

Comando único, antes de todo merge e de todo commit no branch principal:

```bash
bash scripts/verificar.sh
```

No Windows (PowerShell):

```bash
powershell -File scripts/verificar.ps1
```

Ele executa, em ordem: formatação, clippy sem avisos, todos os testes, build release e
`utsim check` em todos os exemplos. O CI (`.github/workflows/ci.yml`) roda o mesmo.
As camadas de teste estão descritas em [testes.md](testes.md):

| Camada | Arquivo | Pega o quê |
|---|---|---|
| Unitários | `crates/*/src/**` (`#[cfg(test)]`) | estatísticas, distribuições, RNG, CSV |
| Teoria | `tests/validation.rs`, `tests/queueing_theory.rs` | motor errado (M/M/1, M/M/c, Jackson, quebras) |
| Blocos | `tests/objects.rs`, `tests/output_data.rs`, `tests/experiment.rs` | semântica de cada bloco e das saídas |
| Golden | `tests/golden.rs` + `tests/golden/*.json` | **qualquer** mudança de resultado nos exemplos |

**Regra dos snapshots golden.** Se um golden falhar, assuma regressão. Regravar
(`UTSIM_BLESS=1`) só é permitido quando a mudança de comportamento é intencional, e o
commit precisa explicar o porquê e mostrar o diff dos `.json`.

**Regras para coisas novas:**
- Toda funcionalidade tem um código em `requisitos.md` (RF-xx ou RNF-xx), atualizado no mesmo commit.
- Bloco novo → teste determinístico exato em `objects.rs` + exemplo em `examples/` (que ganha snapshot golden).
- Campo novo em `ObjectKind` → braço correspondente no patcher de cenários (`experiment.rs`).
  O compilador obriga a tratar variantes novas, mas **não** campos novos.
- Semântica nova → seção em `arquitetura.md`, marcando "Nosso jeito" nas decisões de projeto que merecem destaque.
- Funcionalidade com resultado conhecido na teoria → teste contra a teoria, com tolerância justificada.

## 4. Mapa de paralelismo

Fases que não dependem uma da outra correm ao mesmo tempo. Dentro de cada fase, as
frentes também correm em paralelo depois do contrato.

```
           contrato         frentes em paralelo                          integração
Fase 1  ── turnos (Opus) ─┬─ Sonnet: perda de chegadas + M/M/1/K ──────┐
                          └─ Haiku: testes e exemplo de turnos ────────┴─ merge + verificar
Fase 1b ── utsim-fit ─────┬─ Sonnet A: famílias + MLE (+ Dist novas)  ─┐
           (Opus)         ├─ Sonnet B: KS, AD, χ², bootstrap ──────────┤
                          ├─ Sonnet C: diagnóstico dos dados ──────────┤
                          └─ Sonnet D: relatório + `utsim fit` ────────┴─ merge ─ Haiku: testes de recuperação
Fase 2  ── Observer ──────┬─ Sonnet: visualizador Bevy (replay) ───────┐
           (Opus)         └─ Haiku: consistência replay × eventos ─────┴─ merge + verificar
Fase 3  ── utsim-edit ────┬─ Sonnet: modo de edição no utsim-view ─────┐
           (Opus)         ├─ Sonnet: painel de propriedades ───────────┤
                          └─ Haiku: sequências aleatórias de comandos ─┴─ merge + verificar
Fase 4  ── transporte ────┬─ Sonnet A: transportadores e regras ───────┐
           (Opus)         ├─ Sonnet B: rede de caminhos ───────────────┤
                          ├─ Sonnet C: operadores nos processadores ───┤
                          └─ Sonnet D: veículos no 3D e propriedades ──┴─ merge + verificar
Fase 5  ── esteira/rack ──┬─ Sonnet A: esteira cinemática ─────────────┐
           (Opus)         ├─ Haiku B: porta-paletes ────────────────────┤
                          └─ (depois) Sonnet: esteira/rack no 3D ───────┴─ merge + verificar
Fase 6  ── tokens/scripts ┬─ Sonnet A: motor do fluxo de tokens ───────┐
           (Opus)         ├─ Sonnet B: scripts em Rhai ────────────────┤
                          ├─ Sonnet C: fluxos e scripts no editor ─────┤
                          └─ (depois) Haiku: teoria com fluxos ────────┴─ merge + verificar
Fase 7  ── utsim-opt ─────┬─ Sonnet A: recozimento, genético, refinamento ─┐
           (Opus)         ├─ Sonnet B: `utsim otimizar` e relatório ───────┤
                          ├─ Haiku C: ótimos conhecidos (Erlang C, M/M/1/K) ┤
                          └─ Sonnet D: navegador (WebAssembly) ────────────┴─ merge + verificar
Fase 7b ── passo a passo, ┬─ Sonnet E: 3D no navegador ────────────────┐
           decisões,      ├─ Sonnet F: rede de fluidos ─────────────────┤
           fluidos, AGV   ├─ Sonnet G: tráfego de AGVs ─────────────────┤
           (Opus)         ├─ Sonnet H: emulação OPC UA ─────────────────┤
                          ├─ Sonnet I: aprendizado por reforço ─────────┤
                          └─ (depois) fluidos/AGV no 3D + validação ────┴─ merge + verificar
QA da UI  ── Sonnet: modo roteirizado (entrada sintética + capturas) ───── o coordenador roda
             os roteiros, olha as capturas e delega as correções
```

**Teste da interface sem acesso à tela.** O coordenador não clica na janela: ele
escreve roteiros (`crates/utsim-viewer/roteiros/*.ron`) que o próprio `utsim-view`
executa com entrada sintética e capturas em PNG, e lê as imagens. Defeitos de interface
achados assim viram tarefas para uma frente de correção.

Regras para rodar em paralelo sem conflito:
- Cada frente tem **arquivos próprios**. Quando duas precisam do mesmo arquivo, uma delas
  só registra o mínimo (uma linha de `mod`, um subcomando).
- Só **um** agente por vez mexe em cada arquivo do núcleo (`engine.rs`, `dist.rs`...).
- Frentes que dependem do resultado de outra (ex.: testes de recuperação dependem dos
  estimadores) esperam a integração da anterior.

## 5. Plano das próximas fases

### Fase 1: fechamento
- Integrar a rede de segurança (golden, scripts, CI) e gravar os snapshots dos exemplos novos.
- Pendências: M/M/1/K (precisa de "perda" de chegadas: opção `on_blocked: Drop` na Source)
  e tabelas de turnos (estado `down` programado). Ambas vão para Sonnet, com contrato já existente.

### Fase 1b: ajuste de distribuições
- **Contrato (Opus):** crate novo `utsim-fit` com os tipos `Dataset`, `Candidate`, `FitResult` e
  `FitReport`. Inclui a regra de ranking e o novo tipo de `Dist` empírica no núcleo.
- **Frentes em paralelo:**
  - Sonnet A: estimadores MLE por família, usando os métodos dos momentos como ponto de partida.
  - Sonnet B: testes KS, AD e χ², com bootstrap paramétrico.
  - Sonnet C: diagnóstico dos dados (autocorrelação, corridas, Mann-Kendall, outliers).
  - Sonnet D: relatório HTML com gráficos (histograma+PDF, Q-Q, P-P, ECDF) e subcomando `utsim fit`.
- **Haiku:** testes de recuperação de parâmetros e da taxa de erro tipo I.
- **Validação:** exemplos do Law, cap. 6.

### Fase 2: visualização 3D (só leitura)
- **Contrato (Opus):** trait `Observer` no kernel (transferências, estados, criação/destruição),
  registro de trajetórias `(t0, t1, origem, destino)` e formato de *replay*.
- **Frentes:**
  - Sonnet: app Bevy que carrega o `.ron`, desenha os objetos e anima o *replay*.
  - Sonnet: controles de tempo (velocidade, pausa, passo).
  - Haiku: teste de consistência entre *replay* e log de eventos.

### Fase 3: editor
- **Contrato (Opus):** modelo editável em memória com desfazer/refazer e serialização estável (diffs limpos no Git).
- **Frentes Sonnet:** gizmos/grade, conexões com o mouse, painel de propriedades (egui).

### Fases 4 a 7
Recursos móveis, manuseio de materiais, lógica do usuário e itens avançados (ver o
roteiro). Cada uma começa pelo contrato do kernel. Recursos móveis exigem o maior
contrato (TaskSequence, Dispatcher, rede de caminhos) e ficam inteiros com o
coordenador até o primeiro teste de ponta a ponta passar.
