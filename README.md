# Família UTSim: bibliotecas comuns

Este repositório tem as bibliotecas que os apps da família UTSim compartilham: o núcleo da
simulação por eventos discretos (`utsim-core`, `utsim-model`, `utsim-objects`, `utsim-stats`,
`utsim-scene` e outros), o tema e as janelas (`utsim-ui-tema`), as traduções
(`utsim-idiomas`), as ferramentas visuais, a pasta de trabalho, o `utsim-launcher-core` e os
núcleos de domínio que mais de um app usa (Lean Seis Sigma, Qualidade, Segurança, Ensino).

Cada app tem o seu repositório e usa esta família por git, numa tag fixa:

| App | Repositório |
|---|---|
| UTSim (simulador, 3D, linha de comando) | [utsim-simulador](https://github.com/JandreiSS/utsim-simulador) |
| Launcher e utsim-publicar | [utsim-launcher](https://github.com/JandreiSS/utsim-launcher) |
| Lean Seis Sigma | [utsim-lean-seis-sigma](https://github.com/JandreiSS/utsim-lean-seis-sigma) |
| Qualidade | [utsim-qualidade](https://github.com/JandreiSS/utsim-qualidade) |
| Segurança do Trabalho | [utsim-seguranca](https://github.com/JandreiSS/utsim-seguranca) |
| Estatística | [utsim-estatistica](https://github.com/JandreiSS/utsim-estatistica) |
| Ensino | [utsim-ensino](https://github.com/JandreiSS/utsim-ensino) |
| Produto | [utsim-produto](https://github.com/JandreiSS/utsim-produto) |

## Verificar e versionar

```bash
bash scripts/verificar.sh      # formatação, lints e testes; termina com "Tudo certo"
```

Uma mudança que um app precisa sai como tag semver nova (`git tag v0.2.0 && git push --tags`).
Em cada app que a usa: `bash scripts/atualizar-familia.sh v0.2.0` e o verificar.sh de lá.
Para mudar a família e um app juntos, use `scripts/familia-local.sh` no repositório do app.

`examples/` tem os modelos de exemplo e `tests/golden/` os resultados de referência; o manual
(mdBook) está em `docs/manual`.

## Regras do projeto

- Não há GitHub Actions: a verificação roda na máquina.
- Integração só na branch `main`.
- Não cite outros softwares de simulação ou de estatística pelo nome.
- Textos da interface vêm de `idiomas/` (veja `docs/traducao.md`).
- Não acrescente arquivo ou texto de licença.

## Licença

Licença ainda não definida; para reutilizar, fale com o autor.
