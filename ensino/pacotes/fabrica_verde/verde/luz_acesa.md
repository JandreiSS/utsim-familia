# Quem deixou a luz acesa?

Na Pinturas Vale Verde, as peças passam por três máquinas: a **lavadora**, que tira o óleo num tanque aquecido, a **estufa**, que seca a peça lavada, e a **cabine de pintura**, com exaustão ligada o tempo todo. Chega uma peça a cada 6 minutos em média, e nenhuma das três máquinas desliga quando fica sem peça.

O bloco ambiental de cada máquina guarda a potência produzindo e esperando:

| Máquina | Produzindo | Esperando |
|---|---|---|
| Lavadora | 15 kW | 9 kW |
| Estufa | 20 kW | 12 kW |
| Cabine | 8 kW | 5 kW |

Cada máquina pode ganhar um **modo econômico** em espera (R$ 500) ou um **desligamento automático** (R$ 1.500). O orçamento é de R$ 2.000.

## O que fazer

1. Antes de simular, diga qual máquina gasta mais energia ligada sem produzir.
2. Escolha o que instalar em cada máquina, dentro do orçamento.
3. Simule e confira se a energia sem produzir das três ficou abaixo de 70 kWh no período.

A energia sem produzir de uma máquina é a potência em espera vezes o tempo em que ela fica esperando. A potência mais alta nem sempre é a que mais gasta.
