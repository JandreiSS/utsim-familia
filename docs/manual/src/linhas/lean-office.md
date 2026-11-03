# Lean Office

O **Lean Office** é a primeira **linha especializada** do UTSim: um pacote de domínio para
estudar escritórios e processos administrativos sobre o mesmo núcleo, o mesmo formato `.ron`
e as mesmas replicações do resto do programa. A linha traz:

- uma **biblioteca** de 12 predefinições de objetos de escritório (entrada de pedidos,
  caixa de entrada, posto de trabalho, aprovação, verificação, tramitação, espera por
  informação, lote, reunião, equipe, pessoa e conclusão), em `linhas/lean_office/biblioteca.ron`;
- um **vocabulário** ("item" vira "caso", "tempo no sistema" vira "lead time"), em
  `linhas/lean_office/vocabulario.ron`;
- os **indicadores** do Lean Office (lead time, touch time, razão de atividade, %C&A, FPY,
  WIP de casos, SLA, ocupação das pessoas, esperas por causa) com intervalo de confiança;
- o **relatório da linha** em HTML, com o **mapa do fluxo de valor do escritório** e o
  **fluxograma de raias por departamento**;
- o modelo pronto **Aprovação de compras** (`examples/aprovacao_de_compras.ron`).

Colocar uma predefinição cria objetos comuns do UTSim: o modelo continua legível e editável
fora da linha, e roda em qualquer edição do programa.

## Primeiro contato

```bash
utsim linha lista
utsim linha lean_office predefinicoes
utsim linha lean_office relatorio examples/aprovacao_de_compras.ron --html relatorio.html
```

O terminal mostra os indicadores com a média entre replicações ± a meia-amplitude do
intervalo de 95 %; o arquivo `relatorio.html` é uma página única, sem recursos externos, que
abre em qualquer navegador e pode ser impressa em PDF.

Para comparar o estado atual com um estado futuro (uma alçada maior, uma etapa a menos,
fluxo unitário), grave o cenário em outro `.ron` e use `--comparar futuro.ron`: o relatório
mostra os dois mapas do fluxo de valor e a tabela de indicadores lado a lado.

`utsim run modelo.ron --linha lean_office` roda o modelo como sempre e, depois do resultado
de costume, mostra os indicadores da linha.

## Unidades e convenções

- **Tempo em minutos**, distâncias em metros. As pessoas andam a 72 m/min (1,2 m/s).
- **Tempo corrido**: o lead time conta noites, fins de semana e esperas. O horário de
  trabalho entra pelos turnos (`shifts`), em geral um calendário `"expediente"` aplicado à
  entrada de pedidos e às pessoas. O modelo pronto mostra um calendário semanal de segunda a
  sexta, das 8h às 18h com almoço das 12h às 13h.
- **Rótulos dos casos** que os indicadores leem:

| Rótulo | Significado | Quem grava |
|---|---|---|
| `prioridade` | número, maior é mais urgente | a entrada de pedidos (campo `rotulos`) |
| `sla` | prazo em minutos a partir da entrada | a entrada de pedidos |
| `prazo` | instante limite (`instante da primeira caixa + sla`) | a primeira caixa de entrada |
| `erro` | 1 quando o caso tem um erro ainda não percebido | o posto (pelo %C&A), zerado pela verificação que percebe |
| `erros_<etapa>` | quantos erros a etapa gerou no caso | o posto |

## Campos do modelo usados pela linha

Quatro campos opcionais, que a simulação não lê (veja [Formato `.ron`](../referencia-ron.md)):

- `line: "lean_office"` no modelo: o modelo é da linha;
- `preset: "lean_office/aprovacao"` no objeto: a predefinição que o criou, que dá o **papel**
  do objeto nos indicadores;
- `department: "Compras"` no objeto: o departamento, que vira a raia do fluxograma (não
  confundir com `areas` do modelo, que são áreas de piso, como sala de espera ou de pausa);
- `symbol: "inspecao"` no objeto: o símbolo do fluxograma (`inicio`, `fim`, `operacao`,
  `transporte`, `inspecao`, `espera`, `armazenagem`).

Modelos sem esses campos abrem e rodam como antes. Um objeto sem `preset` tem o papel do seu
tipo (fonte é entrada, fila é caixa de entrada, processador é trabalho, saída é conclusão),
então os indicadores também valem para modelos que não são da linha.

## Biblioteca

| Predefinição | Base no núcleo | Campos | Papel |
|---|---|---|---|
| `entrada` (Entrada de pedidos) | `Source` | intervalo, rótulos, horário | entrada |
| `caixa_entrada` (Caixa de entrada) | `Queue` com `order` | ordem (chegada, `Desc("prioridade")`, `Asc("prazo")`), limite | caixa de entrada |
| `posto` (Posto de trabalho) | `Processor` com `operators` | tempo, equipe, %C&A, mesas | trabalho |
| `aprovacao` (Aprovação) | `Processor` com `Weighted([aprova, devolve])` | tempo, aprovador, pesos | aprovação |
| `verificacao` (Verificação) | `Processor` com `ByLabel("rota")` | tempo, equipe, fração percebida | verificação |
| `tramitacao` (Tramitação) | `Processor` de capacidade ilimitada, sem pessoa | tempo de passagem | tramitação |
| `espera_informacao` (Espera por informação) | `Processor` de capacidade ilimitada | tempo de resposta | espera por informação |
| `lote` (Agrupamento em lote) | `Queue` com `batch` | tamanho, espera máxima | lote |
| `reuniao` (Reunião ou comitê) | `Queue` (pauta) + `Processor` com horário | tempo por caso, horário, pesos | reunião |
| `equipe` (Equipe) | `Dispatcher` + duas pessoas | regra, horário | equipe |
| `pessoa` (Pessoa) | `Transporter` com perfil de pessoa | habilidades, horário | pessoa |
| `conclusao` (Conclusão) | `Sink` | | conclusão |

Na **aprovação**, na **verificação** e na **reunião**, a primeira ligação de saída é a que
segue e a segunda a que devolve: ligue nessa ordem. O posto sorteia o erro na entrada do
caso (com probabilidade `1 − %C&A`); a verificação percebe uma fração dos erros, zera o
rótulo `erro` e manda o caso pela segunda saída.

Para colocar uma predefinição num modelo (o arquivo é criado se não existir, e o modelo
ganha `line: "lean_office"`):

```bash
utsim linha lean_office colocar escritorio.ron entrada --nome pedidos --campo 'intervalo=Exp(mean: 20.0)'
utsim linha lean_office colocar escritorio.ron caixa_entrada --posicao 2 0 --departamento Compras \
    --campo 'ordem=Desc("prioridade")'
utsim linha lean_office colocar escritorio.ron posto --posicao 4 0 --departamento Compras --campo ca=0.9
utsim linha lean_office colocar escritorio.ron equipe --posicao 4 2 --departamento Compras
utsim linha lean_office colocar escritorio.ron conclusao --posicao 6 0
```

Os valores dos campos são texto RON, como no arquivo do modelo. As ligações entre os objetos
continuam sendo feitas no editor ou no arquivo (`connections`); o comando avisa o que ainda
falta para o modelo ficar válido.

## Fila por prioridade ou por prazo

Qualquer fila aceita `order: Desc("rótulo")` (maior valor primeiro) ou `order: Asc("rótulo")`
(menor primeiro), com o rótulo numérico de cada caso. Empates e casos sem o rótulo ficam na
ordem de chegada, os sem rótulo depois dos outros. A ordem vale para a saída da fila; não use
junto com `discipline: Lifo`. Para que a ordem tenha efeito, o posto seguinte deve ter poucas
mesas (`capacity` igual ao número de casos trabalhados ao mesmo tempo): os outros casos
esperam na caixa de entrada, onde a ordem é aplicada.

## Indicadores

Os indicadores contam os casos que terminaram depois do aquecimento; o WIP é a média no tempo
da mesma janela. Cada valor vem com o intervalo de 95 % entre replicações.

| Indicador | Definição |
|---|---|
| Lead time (L/T) | da entrada do caso à conclusão |
| Tempo de processamento (P/T) | trabalho sorteado para o caso nas etapas (postos, aprovações, verificações, reuniões); tramitação e espera por informação não são trabalho |
| Touch time | parte do trabalho feita por pessoas (com `operators`) |
| Razão de atividade | touch time médio ÷ lead time médio |
| %C&A por etapa | 1 − erros gerados pela etapa (rótulo `erros_<etapa>`) ÷ visitas à etapa |
| %C&A acumulado (rolled) | produto do %C&A das etapas |
| FPY | fração dos casos concluídos sem nenhuma volta e sem erro pendente |
| WIP de casos | casos em aberto no modelo, média no tempo (e casos parados por objeto) |
| % dentro do SLA | casos com rótulo `sla` concluídos em até `sla` minutos |
| Casos concluídos por dia | concluídos ÷ dias corridos da janela |
| Voltas de retrabalho | maior número de visitas a um mesmo objeto − 1, por caso |
| Ocupação das pessoas | fração do tempo trabalhando no caso, andando, livre e fora do expediente; ocupação no expediente e por hora do dia |
| Esperas por causa | tempo médio por caso na caixa de entrada, na etapa sem trabalho (pessoa ocupada, fora do expediente, bloqueio), esperando informação de fora, em tramitação, esperando a reunião, esperando completar o lote |

Vale sempre **lead time = tempo de processamento + soma das esperas**: o relatório usa isso
para o Pareto das esperas.

Os indicadores vêm de métricas por caso que o núcleo guarda só quando a linha pede: visitas,
permanência, trabalho e touch time por objeto. Com elas desligadas (todo `utsim run` sem
`--linha`), os resultados do modelo são idênticos.

## Relatório da linha

O `--html` grava uma página com:

- cartões com lead time, touch time, razão de atividade, FPY, SLA e WIP;
- a tabela de indicadores com as definições;
- o gráfico do WIP de casos no tempo, o histograma do lead time com a linha do SLA e o
  Pareto das esperas por causa;
- as tabelas das etapas (P/T e touch por visita, %C&A), dos casos parados por objeto e da
  ocupação das pessoas, também por hora do dia;
- a distribuição das voltas de retrabalho;
- o **mapa do fluxo de valor do escritório** do caminho principal: cada etapa com P/T, touch,
  L/T da etapa, %C&A, pessoas e horário; as caixas de entrada com os casos parados e a espera;
  a escada do lead time (espera em vermelho, trabalho em verde) e o rodapé com lead time,
  touch time, razão de atividade e %C&A acumulado. Com `--comparar`, o mapa do estado futuro;
- o **fluxograma de raias por departamento**, com as raias do campo `department` e as passagens
  entre departamentos contadas.

O `--json` grava os indicadores de cada replicação, para planilhas e para o UTSim Seis Sigma.

## O modelo pronto: Aprovação de compras

`examples/aprovacao_de_compras.ron` foi montado só com as predefinições da biblioteca. Os
pedidos chegam no horário comercial com valor, prioridade e prazo de 3 dias corridos. Compras
cota o pedido (15 % saem com informação errada) e confere a cotação (percebe 90 % dos erros e
devolve ao solicitante, que demora a responder). A alçada depende do valor: até 5 mil o
gerente aprova, até 25 mil a diretoria, acima disso o comitê, que se reúne às quartas das 14h
às 16h. Cada aprovação pode devolver o pedido para nova cotação. Aprovado, o pedido é emitido
e concluído.

Experimentos típicos: aumentar a alçada do gerente (mudar o limite no script de `alcadas`),
fazer o comitê duas vezes por semana (mais uma janela no horário de `comite`), conferir na
própria cotação (%C&A maior), dedicar um comprador aos urgentes.

## Formato do pacote de uma linha

Para quem cria uma linha nova (a próxima prevista é serviços e atendimento):

```ron
Library(
    line: "lean_office",          // identificador, o valor de Model.line
    name: "Lean Office",          // nome exibido
    description: "...",
    models: [(file: "examples/aprovacao_de_compras.ron", name: "Aprovação de compras")],
    presets: [
        (
            id: "posto",          // a predefinição grava preset: "lean_office/posto"
            name: "Posto de trabalho",
            icon: "mesa_computador",
            role: Work,           // Entry, Inbox, Work, Approval, Check, Transit, WaitInfo,
                                  // Batch, Meeting, Team, Person ou Done
            name_base: "posto",
            fields: [(key: "tempo", label: "Tempo de trabalho", default: "Exp(mean: 20.0)")],
            objects: [r#"(name: "${name}", kind: Processor(process: ${tempo}))"#],
            connections: [],
        ),
    ],
)
```

Cada objeto é o texto RON de um objeto comum com marcadores `${chave}`: `${name}` é o nome
escolhido; as outras chaves são os campos, com o valor padrão em RON. O primeiro objeto se
chama `${name}` e os outros começam por ele. O UTSim confere o pacote ao carregar: todo
marcador tem campo, todo campo é usado, e cada predefinição monta objetos válidos com os
valores padrão. O vocabulário é `Vocabulary(line: "...", terms: {"item": "caso", ...})`.

## O que ainda não está nesta entrega

- **Multitarefa** (uma pessoa com vários casos abertos e custo de troca) e **interrupções**:
  segunda entrega, junto com a prioridade e a preempção do plano de chegadas.
- **Vocabulário e biblioteca na interface** e a **vista 2D de planta** com zonas por
  departamento (para todas as linhas): depois da migração da interface.
- **Ativos 3D de escritório**: primeiro os gerados por código e os livres CC0.
- Os outros modelos prontos (contas a pagar, central de solicitações, admissão) e o tutorial
  "do mapa ao modelo".
