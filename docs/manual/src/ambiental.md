# Bloco ambiental do nó

O bloco ambiental (`environment`) é opcional em cada nó e no modelo. O nó guarda a potência de cada grupo de estados e os fluxos que usa, e o modelo guarda o catálogo de fluxos, fatores e destinos de resíduos. A simulação não lê o bloco: ele só entra nas contas depois da rodada.

O UTSim Ambiental edita o bloco e faz a conta completa (curva de carga, demanda, água, massa, Sankey, produto e metas). O UTSim mostra a potência na aba Ambiental do nó. Os dois editam o mesmo dado.

## Métricas no relatório

Quando algum nó tem bloco, o relatório de cada replicação traz, para esse nó, as métricas abaixo. O UTSim Ensino e os experimentos as leem como qualquer métrica do bloco (`<nó>.<métrica>`).

| Métrica | O que é |
|---|---|
| `energia_kwh` | potência de cada estado vezes o tempo nele, mais a energia por peça |
| `energia_kwh.<grupo>` | a parte de cada grupo: `produzindo`, `preparando`, `esperando`, `parada`, `fora_de_turno` |
| `energia_sem_valor_kwh` | esperando, parada e fora de turno somados |
| `kwh_por_peca` | energia dividida pelas peças que saíram |
| `fluxo.<fluxo>` | quantidade de cada fluxo do catálogo (0 quando o nó não o usa) |
| `co2e_kg` | eletricidade e fluxos vezes os fatores, só quando todo fator usado existe |

Esta é a conta simples, pelas frações de tempo do relatório. Setups, partidas e desligamento automático em espera entram só na conta do UTSim Ambiental.

## Chaves para missões e experimentos

- `<nó>.environment`: troca o bloco inteiro (`Some((power: (...), flows: [...]))`)
- `<nó>.environment.power.<grupo>`: muda a potência de um grupo (`producing`, `preparing`, `waiting`, `stopped`, `off_shift`) ou de um estado

O pacote de missões "Fábrica verde" do UTSim Ensino usa essas chaves. Os fatores dele são didáticos e não servem para declarar emissões.
