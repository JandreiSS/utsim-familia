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

## Formato do projeto do UTSim LSS (`formato`)

Cada arquivo JSON de uma pasta de projeto `.lss` (`projeto.json`, cada operação de `ops/`,
cada instantâneo, a presença) começa com `"formato": N`; ausente, vale 1. `FORMAT` (em
`utsim-lss-core::format`) é o mais novo que o programa lê e o que ele grava.

- Mais novo que o suportado: erro `projeto no formato N; esta versão do UTSim LSS lê até
  M` (num arquivo de operação, ele fica pendente e o aviso aparece na barra de baixo).
- Mais antigo: o documento passa pelos passos de `MIGRATIONS`, em ordem, como `serde_json::Value`.
- Campos novos opcionais não mudam o formato. A versão do app (`0.1.0`, tag `lss-v<versão>`) é
  separada da do simulador.

| Formato | Mudança | Migração |
|---|---|---|
| 1 | primeira versão | — |

## Protocolo de cena (`utsim-scene/1`)

A constante `PROTOCOL` de `utsim-scene` identifica o fluxo de cena (linhas JSON: cabeçalho e quadros) consumido por visualizadores e motores externos.
Qualquer mudança incompatível nos tipos, nas unidades ou nos eixos incrementa o número (`utsim-scene/2`).
Acrescentar campos opcionais não muda o protocolo. Clientes devem recusar um protocolo que não conhecem.
