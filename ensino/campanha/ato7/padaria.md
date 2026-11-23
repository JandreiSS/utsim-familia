# Do enunciado ao modelo

Uma padaria industrial quer um modelo da linha de pães. Você vai construir o modelo do zero no UTSim, a partir desta descrição. O tempo está em minutos.

- As massas chegam em média a cada 6 minutos (exponencial) e esperam numa fila antes da divisora.
- A **divisora** (`divisora`) leva de 3 a 5,5 minutos por massa, com moda de 4 (triangular).
- A **câmara de fermentação** (`camara`) recebe até 10 massas ao mesmo tempo e segura cada uma por 45 minutos exatos.
- Depois da câmara, as massas esperam numa fila antes do **forno** (`forno`), que assa 2 massas ao mesmo tempo, de 10 a 16 minutos cada, com moda de 12 (triangular). O forno quebra em média a cada 300 minutos (exponencial) e o conserto leva em média 20 minutos (exponencial).
- As massas assadas esperam numa fila antes da **embalagem** (`embalagem`), que leva de 4 a 6 minutos (uniforme), e vão para a **expedição** (`expedicao`).

Simule uma semana de 7 dias (10.080 minutos depois de 480 minutos de aquecimento), como no ponto de partida.

## O que fazer

1. Antes de modelar, diga qual etapa deve limitar a padaria.
2. Grave o ponto de partida, monte o modelo no UTSim com os nomes de objeto entre parênteses (sem acento) e grave o arquivo .ron.
3. Indique o arquivo e simule. O app confere a estrutura e compara as massas expedidas, a utilização do forno e o lead time com o modelo de referência.
