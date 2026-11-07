# Posto pesado

Na Distribuidora Vale do Sol, três pessoas levam a pé caixas de 15 a 25 kg do estoque até o posto de montagem, uma caixa por viagem. O sistema escala sempre a primeira pessoa livre, e a Ana está no topo da escala. Ela leva quase tudo, enquanto o Bruno e a Carla quase não trabalham.

No modelo, cada pessoa tem um nível de **fadiga** de 0 a 1. Ele sobe com o trabalho, com a caminhada e com o peso da caixa, e cai no descanso. Com fadiga alta, a pessoa trabalha mais devagar e faz pausas. Os números do modelo são estimativas para o ensino, sem dados medidos de um posto real.

Há duas frentes de melhoria:

- **quem leva a próxima caixa**: manter a primeira pessoa livre, fazer rodízio entre as pessoas livres, ou escolher a que está livre há mais tempo
- **o método**: manter a caixa de 15 a 25 kg, passar para caixas de 8 a 14 kg (R$ 2.000) ou instalar uma mesa elevatória com apoio, que deixa a carga efetiva em 4 a 8 kg (R$ 6.000)

O orçamento é de R$ 6.000.

## O que fazer

1. Antes de simular, diga quem acumula mais fadiga.
2. Escolha a regra e o método.
3. Simule e confira três coisas: fadiga máxima da Ana abaixo de 0,25, pelo menos 180 caixas entregues e o menor custo possível.

## Aviso

A nota de ergonomia do estudo de tempos não faz parte do relatório da simulação, por isso este caso usa a fadiga da pessoa. O UTSim não substitui o profissional habilitado e não certifica conformidade com a NR-17 (ergonomia) nem com outra norma. Um valor de fadiga simulado compara cenários do mesmo modelo e não avalia o risco de uma pessoa real.
