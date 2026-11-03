# Versionamento

O UTSim tem três números de versão independentes.

## Aplicação (semver)

O número em `Cargo.toml` (`version` do workspace) segue o [versionamento semântico](https://semver.org/lang/pt-BR/): `MAJOR.MINOR.PATCH`.
Enquanto estiver em `0.x`, mudanças incompatíveis podem acontecer em versões `MINOR`, sempre registradas no `CHANGELOG.md`.
Uma versão sai com a tag `v<versão>` (por exemplo `v0.1.0`), que dispara o fluxo de publicação em `.github/workflows/release.yml`.

## Formato do modelo (`format`)

O arquivo `.ron` pode declarar `format: N` no topo do `Model(...)`. Ausente, vale 1.
`FORMAT_VERSION` (em `utsim-model`) é a versão mais nova que esta versão do programa lê.

- Formato mais novo que o suportado: erro `modelo no formato N; esta versão do UTSim lê até M`.
- Formato mais antigo: o texto passa pelas migrações da lista `MIGRATIONS`, em ordem, antes de ser lido. Cada passo reescreve o texto de uma versão para a seguinte (`v1 → v2` e assim por diante).
- O editor sempre salva com o formato atual.
- O formato só sobe quando a mudança é incompatível (campo removido, renomeado ou com outro significado). Campos novos opcionais, com `#[serde(default)]`, não exigem nova versão.

| Formato | Mudança | Migração |
|---|---|---|
| 1 | primeira versão | — |
| 2 | modos de movimentação: `run.movement` ausente passa a valer `Free` (menor caminho no piso, desviando dos obstáculos) | `1 → 2` grava `format: 2` e, se o modelo tem transportadores, `movement: Legacy` em `run` (o comportamento do formato 1). Os resultados dos modelos antigos não mudam |

## Protocolo de cena (`utsim-scene/1`)

A constante `PROTOCOL` de `utsim-scene` identifica o fluxo de cena (linhas JSON: cabeçalho e quadros) consumido por visualizadores e motores externos.
Qualquer mudança incompatível nos tipos, nas unidades ou nos eixos incrementa o número (`utsim-scene/2`).
Acrescentar campos opcionais não muda o protocolo. Clientes devem recusar um protocolo que não conhecem.
