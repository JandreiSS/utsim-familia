# Plano: linhas especializadas do UTSim, começando pelo Lean Office

Plano técnico, sem implementação, do pedido do dono: ter **linhas** do UTSim para
simulações e estudos em domínios específicos, começando pelo **Lean Office** (escritórios e
processos administrativos). O plano define o que é uma linha, como ela usa o núcleo que já
existe, o que o Lean Office precisa (objetos, vocabulário, modelos prontos, indicadores,
relatórios, aparência 3D e 2D, integração com o UTSim Seis Sigma), lista outras linhas
candidatas e propõe os primeiros passos com estimativa e modelo de IA. Os requisitos sugeridos
começam em RF-269 e ficam só aqui até o dono aprovar.

Documentos relacionados, que este plano não repete:

- `docs/plano_lean_seis_sigma.md`: o app UTSim Seis Sigma, VSM e SIPOC gerados do modelo,
  pacote de evidência `estudo.json`;
- `docs/plano_chegadas_e_equipes_3d.md`: tabela de chegadas, tipos de item, prioridade,
  preempção e áreas (peças de que o Lean Office depende);
- `docs/plano_3d_realista.md` e `docs/manual/src/visual-3d.md`: biblioteca de modelos,
  ativos gerados por código, ativos de terceiros;
- `docs/manual/src/fluxograma.md`: os dez diagramas do `utsim-diagram` (raias, VSM, carta,
  SIPOC, espaguete, homem-máquina);
- `docs/backlog_v02.md`: calendário com feriados, custos, setup dependente da sequência,
  submodelos.

---

## 0. O que é uma linha

### 0.1 Princípio

**Uma linha é um pacote de domínio sobre o mesmo núcleo, não um programa separado.** O motor
de eventos, os objetos, o formato `.ron`, as replicações, a estatística, os experimentos, o
3D e o painel são os mesmos. A linha acrescenta:

| Peça da linha | O que é | Onde mora |
|---|---|---|
| Biblioteca de objetos do domínio | predefinições de objetos do núcleo com nome, ícone, campos padrão e campos escondidos; objetos novos só quando o núcleo não consegue representar | `linhas/<linha>/biblioteca.ron` |
| Vocabulário | tradução dos termos da interface e dos relatórios ("item" vira "caso", "processador" vira "posto de trabalho") | `linhas/<linha>/vocabulario.ron` |
| Modelos prontos e exemplos | modelos `.ron` para começar e exemplos de estudo | `linhas/<linha>/modelos/` |
| Indicadores e relatórios | conjunto de KPIs do domínio (cálculo em crate pura) e relatório próprio | crate `utsim-linhas` (módulo por linha) |
| Aparência | modelos 3D e ícones 2D do domínio, cores, cenário padrão | `linhas/<linha>/visual/` e biblioteca de ativos |
| Tutoriais e manual | capítulos do manual e tutoriais com o vocabulário da linha | `docs/manual/src/linhas/<linha>/` |
| Modelos de projeto no Seis Sigma | modelos de projeto (kaizen de escritório, por exemplo) | app UTSim Seis Sigma |

### 0.2 Como o modelo sabe a linha

Um campo opcional `line: Option<String>` no `Model` (por exemplo `line: Some("lean_office")`).
O núcleo ignora o campo para simular; a interface usa para abrir com o vocabulário, a
biblioteca e os relatórios da linha. Modelos sem o campo abrem como hoje. Um modelo de uma
linha abre em qualquer edição do UTSim; só perde o vocabulário se a linha não estiver
instalada.

Os objetos criados pela biblioteca guardam a origem num campo opcional `preset:
Option<String>` em `ObjectDef` (por exemplo `"lean_office/aprovacao"`), para a interface
mostrar o ícone e o nome do domínio e para os relatórios saberem o papel do objeto (uma
aprovação, uma verificação). Esse campo é parente do campo `symbol` que o backlog já propõe
para o fluxograma; a proposta é decidir os dois juntos (decisão L2).

### 0.3 Como a linha chega ao usuário

- **Mesma instalação, linhas como pacotes.** A edição padrão traz todas as linhas; o launcher
  (`docs/plano_launcher.md`) pode mostrar a linha como entrada própria ("UTSim Lean Office")
  que abre o mesmo binário com `--linha lean_office`. Sem custo extra de distribuição.
- **Interface.** A tela inicial pergunta a linha (ou abre na última usada); a biblioteca à
  esquerda mostra só os objetos da linha, com a biblioteca completa a um clique.
- **Distribuição pelo Drive**, como o resto (sem loja, sem servidor).

---

## 1. Lean Office: o domínio

### 1.1 O que se estuda num escritório

Processos administrativos têm as mesmas perdas da fábrica, mas escondidas: o "estoque" é a
pilha de casos nas caixas de entrada, o "transporte" é a passagem de um departamento a outro,
e o retrabalho vem de informação incompleta ou errada. Os estudos típicos respondem:

- quanto tempo um caso leva do pedido à conclusão (lead time) e quanto disso é trabalho de
  fato (touch time);
- onde os casos esperam e por quê (pessoa ocupada, lote, espera por informação, aprovação,
  fora do horário);
- quanto retrabalho existe e onde ele nasce (%C&A, completo e correto, de cada etapa);
- o efeito de juntar ou separar etapas, mudar a ordem, tirar uma aprovação, trabalhar em
  fluxo unitário em vez de lote, limitar a multitarefa, nivelar a demanda, dedicar pessoas;
- se o SLA é cumprido e com quantas pessoas.

### 1.2 Conceitos do domínio mapeados no núcleo

| Conceito | Representação no UTSim | Existe? |
|---|---|---|
| Caso, documento, solicitação, processo | item, com tipo (C3 do plano de chegadas) e rótulos (`prioridade`, `complexidade`, `prazo`, `erro`) | item e rótulos existem; tipo é novo |
| Chegada de pedidos por hora do dia | `Source` com `schedule` (já existe) ou tabela de chegadas (C1) | parcial |
| Caixa de entrada, pasta, fila do sistema | `Queue` com disciplina FIFO; nova disciplina por prioridade ou por prazo (`Discipline::ByLabel`) | FIFO existe; prioridade é nova |
| Posto de trabalho, mesa | `Processor` com `operators` de uma equipe de pessoas; capacidade = mesas | existe |
| Pessoa (analista, aprovador) | `Transporter` com perfil `person` (habilidades, ritmo, aprendizado) dentro de uma equipe (`Dispatcher`) | existe |
| Pessoa como recurso principal, não a mesa | `Processor` de capacidade grande com `operators.count: 1`: o limite real vira o número de pessoas | existe |
| Aprovação com devolução | `Processor` com `OutputRule::Weighted([aprova, devolve])` e ligação de volta | existe |
| Verificação que acha erro de etapa anterior | rótulo `erro` sorteado na etapa que erra (pelo %C&A dela) e `OutputRule::ByLabel` na verificação | existe com script; a predefinição esconde o detalhe |
| Retrabalho (%C&A) | predefinição "Etapa com %C&A": sorteia o erro na saída; a etapa seguinte decide se percebe | novo como predefinição; núcleo existe |
| Lote de papelada | `Queue.batch` (tamanho, espera máxima) | existe |
| Passagem entre departamentos (malote, e-mail, sistema) | predefinição "Tramitação": `Processor` de capacidade ilimitada com tempo de espera | existe |
| Espera por informação de fora (cliente, outro setor) | `Combiner` que junta o caso com a resposta que chega de outra `Source`, ou "Tramitação" com tempo sorteado | existe |
| Horário de trabalho, almoço, feriados | `calendars` e `shifts` (feriados no backlog) | parcial |
| Fila parada fora do horário | estado `off_shift` | existe |
| Multitarefa (uma pessoa com vários casos abertos) | **novo**: limite de casos abertos por pessoa (WIP pessoal) e custo de troca entre casos | novo (seção 2.2) |
| Interrupções (telefone, chat, urgência) | `Source` de interrupções com prioridade alta e preempção do operador (E2 e E4 do plano de chegadas) | depende de E2/E4 |
| SLA, prazo | rótulo `prazo` na criação (C4 do plano de chegadas) e KPI "% dentro do prazo" | depende de C4 |
| Departamento | campo de área por objeto, usado pelas raias (`raias.rs` já aceita áreas) e pelo 3D (zonas) | áreas existem só no comando do fluxograma |
| Caso concluído, cancelado, arquivado | `Sink`, uma por desfecho | existe |

Conclusão: **a maior parte do Lean Office cabe no núcleo atual com predefinições**. O que
pede código novo no núcleo é pouco e serve a outras linhas também: disciplina de fila por
prioridade ou prazo, multitarefa com custo de troca, rótulo por expressão (prazo) e a
prioridade e preempção do plano de chegadas.

### 1.3 Biblioteca de objetos do Lean Office

| Objeto da linha | Ícone e visual | Base | Campos mostrados | Campos padrão |
|---|---|---|---|---|
| Entrada de pedidos | bandeja de entrada | `Source` | pedidos por hora (grade), tipos de caso, % urgente | `schedule` no horário comercial |
| Caixa de entrada | bandeja com pilha | `Queue` | ordem (chegada, prioridade, prazo), limite | FIFO |
| Posto de trabalho | mesa com computador | `Processor` + `operators` | tempo de trabalho (touch), equipe, %C&A da etapa | capacidade ilimitada; 1 pessoa |
| Aprovação | mesa com carimbo | `Processor` + `Weighted` | tempo, aprovador, % de devolução, para onde devolve | 1 aprovador |
| Verificação | lupa | `Processor` + `ByLabel` | tempo, % de erros percebidos | percebe 100 % |
| Tramitação | envelope com seta | `Processor` ilimitado | tempo de passagem, meio (malote, e-mail, sistema) | sem pessoa |
| Espera por informação | relógio com balão | `Combiner` ou `Processor` ilimitado | quem responde, tempo de resposta | tempo sorteado |
| Agrupamento em lote | pilha amarrada | `Queue.batch` | tamanho do lote, espera máxima | |
| Reunião ou comitê | mesa redonda | `Queue.batch` + `Processor` com horário | dia e hora, pauta máxima | uma vez por semana |
| Equipe | grupo de pessoas | `Dispatcher` + pessoas | pessoas, habilidades, horário, regra | `LeastBusy` (E3) |
| Pessoa | pessoa sentada | `Transporter` com `person` | horário, habilidades, multitarefa | velocidade de caminhada padrão |
| Conclusão | pasta arquivada | `Sink` | desfecho (concluído, cancelado) | |

Cada predefinição é um trecho `.ron` com marcadores para os campos mostrados, validado pelo
mesmo leitor do modelo. Colocar uma "Aprovação" cria um objeto comum: o modelo continua
legível e editável fora da linha.

### 1.4 Vocabulário

| Termo do núcleo | Termo do Lean Office |
|---|---|
| item | caso (ou o nome do tipo: pedido, nota, contrato) |
| Source | entrada de pedidos |
| Queue | caixa de entrada |
| Processor | posto de trabalho, etapa |
| Sink | conclusão |
| Transporter (pessoa) | pessoa |
| Dispatcher | equipe |
| tempo no sistema | lead time |
| tempo de processamento | touch time |
| conteúdo da fila | casos parados |
| vazão | casos concluídos por dia |
| `waiting_operator` | esperando a pessoa |
| `off_shift` | fora do expediente |
| `blocked` | sem espaço na próxima etapa |

O vocabulário é um dicionário por linha aplicado na interface, nos relatórios e no manual da
linha. O núcleo e o `.ron` não mudam de nome.

### 1.5 Indicadores do Lean Office

Calculados numa crate pura (`utsim-linhas::lean_office`) a partir do relatório e do rastro de
eventos, cada um com intervalo de confiança entre replicações e tamanho da amostra:

| KPI | Definição | De onde vem |
|---|---|---|
| Lead time (L/T) | da entrada do caso à conclusão | `time_in_system` na `Sink` (existe) |
| Tempo de processamento (P/T) por etapa | tempo em que a etapa trabalha no caso, incluindo pequenas esperas internas | estados `processing` e `setup` por item |
| Touch time | tempo de trabalho efetivo das pessoas no caso | estado `utilize` das pessoas, somado por item |
| Razão de atividade | touch time ÷ lead time | os dois acima |
| %C&A por etapa | fração dos casos que saem da etapa completos e corretos | rótulo `erro` na saída da etapa |
| Rendimento de primeira passagem (FPY, rolled %C&A) | fração dos casos concluídos sem nenhum retrabalho; e o produto dos %C&A | contador de voltas por item |
| WIP de casos | casos em aberto ao longo do tempo e por etapa | séries de conteúdo (existe) |
| % dentro do SLA | casos concluídos até o `prazo` | rótulo `prazo` na `Sink` |
| Casos concluídos por dia | vazão por dia útil do calendário | `Sink` com o calendário |
| Ocupação das pessoas | fração do expediente em trabalho, em troca de caso, em espera | estados das pessoas |
| Tempo de espera por causa | espera por pessoa, por lote, por informação, fora do expediente, por aprovação | estado da fila e papel do objeto (`preset`) |
| Voltas de retrabalho | média e distribuição de voltas por caso | contador de voltas |

O núcleo precisa de duas coisas pequenas e gerais para isso: (a) contar as visitas de um item
a cada objeto (o rastro já tem, falta o resumo por item); (b) somar por item o tempo de
`utilize` das pessoas que trabalharam nele. As duas viram métricas opcionais, ligadas pela
linha, sem mudar o relatório dos modelos atuais.

### 1.6 Relatórios e diagramas

- **VSM de escritório** (`utsim-diagram::vsm` com o vocabulário e campos da linha): caixa de
  processo com P/T, L/T da etapa, %C&A, pessoas e horário; caixas de entrada com casos parados
  (média) e tempo de espera; escada do lead time com touch time; no rodapé, lead time total,
  touch time total, razão de atividade e rolled %C&A. Gera o VSM atual e, com um cenário, o
  futuro, lado a lado.
- **Fluxograma de raias por departamento** (`raias.rs` já aceita áreas): as raias vêm do
  campo de área do objeto; as passagens entre raias são destacadas e contadas; as voltas de
  retrabalho aparecem com a cor própria que o desenho já tem.
- **Carta de processo** com as esperas por causa.
- **Espaguete do escritório**: deslocamentos das pessoas (impressora, arquivo, sala do
  aprovador) sobre a planta.
- **Relatório da linha** (HTML autocontido, como o `html.rs`): resumo dos KPIs da seção 1.5,
  gráfico de WIP de casos no tempo, histograma do lead time com a linha do SLA, Pareto das
  esperas por causa, ocupação das pessoas por hora do dia.

### 1.7 Aparência 3D e 2D

- **2D primeiro.** No escritório, a vista de planta (de cima) com zonas por departamento,
  mesas e pilhas de casos lê melhor que o 3D. A vista 2D usa a mesma cena com câmera
  ortográfica de cima e ícones planos; vale também para as outras linhas.
- **3D**: mesas, cadeiras, computador, armário de pastas, impressora, mesa de reunião,
  divisórias, bandejas de entrada. Pessoas sentadas trabalhando (a pose sentada já está
  prevista para o motorista da empilhadeira) e andando com pastas. Caso como pasta ou
  envelope colorido pelo tipo; caixa de entrada como pilha cuja altura acompanha os casos
  parados, que é o "estoque" visível do Lean Office.
- **Origem dos ativos**, em ordem de custo:
  1. **gerados por código** (como a empilhadeira e o porta-paletes): mesa, armário, bandeja,
     pilha de papéis e divisória são formas simples e paramétricas; custo zero e sem dúvida
     de licença;
  2. **ativos livres CC0** para cadeira, computador e impressora, com o crédito e a
     conferência de licença do `visual-3d.md`;
  3. **Tripo 3D e Blender** para peças com cara de escritório real, **só depois de perguntar
     ao dono e ele aprovar o gasto** de cada lote de gerações, com a lista do que gerar e a
     estimativa de créditos. Os arquivos gerados passam pela conferência automática de
     escala, eixos e níveis de detalhe proposta no backlog.
- **Cores de estado** do posto e da pessoa iguais às do resto do UTSim; legenda com o
  vocabulário da linha.

### 1.8 Integração com o UTSim Seis Sigma

O Lean Office é o caso mais natural para o app Seis Sigma, porque muitos projetos de melhoria
em empresas são administrativos:

- **Modelos de projeto** no app: "Kaizen de escritório" (5 dias) e "DMAIC transacional", com
  os entregáveis e checklists ajustados (VSM de escritório, %C&A medido, mapa de raias).
- **Importar o modelo da linha** gera o VSM de escritório, as raias e o SIPOC com o
  vocabulário (seção 3.1 do plano Seis Sigma, sem novidade de formato).
- **Medir**: os KPIs da seção 1.5 viram características do projeto, com dado real (exportado
  do sistema de chamados ou de fluxo de trabalho da empresa em CSV), simulado ou do gêmeo.
- **Analisar**: Pareto das esperas por causa, %C&A por etapa e passagens entre departamentos
  como evidência na matriz C&E.
- **Melhorar**: cenários típicos prontos como modelos de experimento (tirar uma aprovação,
  juntar duas etapas, fluxo unitário em vez de lote, limitar a multitarefa, dedicar uma pessoa
  às urgências) com comparação pareada e o `estudo.json`.
- **Controlar**: lead time, % SLA e WIP de casos nas cartas do plano de controle.

### 1.9 Modelos prontos e exemplos

| Modelo | O que mostra |
|---|---|
| Aprovação de compras | três níveis de aprovação por valor, devoluções, reunião semanal do comitê, efeito de alçadas maiores |
| Contas a pagar | lote diário de notas, conferência com erro, espera por informação do fornecedor, horário e fechamento do mês |
| Central de solicitações | chegadas por hora, triagem por tipo, níveis 1 e 2, SLA por prioridade, interrupções |
| Admissão de funcionário | passagens entre RH, TI, financeiro e gestor; espera por documentos do candidato |
| Tutorial "do mapa ao modelo" | montar o VSM de escritório de um processo de 5 etapas, rodar e comparar com o futuro |

---

## 2. O que precisa de código novo

### 2.1 Na infraestrutura das linhas

| Peça | Descrição | Onde |
|---|---|---|
| Campos `line` e `preset` | opcionais, ignorados pela simulação | `utsim-model` |
| Leitor de pacote de linha | lê `biblioteca.ron`, `vocabulario.ron` e a lista de modelos; valida cada predefinição | crate nova `utsim-linhas` |
| Aplicar predefinição | cria o objeto a partir do trecho e dos campos preenchidos | `utsim-edit` |
| Vocabulário na interface | troca de termos na interface Rust/egui e nos relatórios | interface (depois da migração) |
| KPIs por linha | módulo por linha com os KPIs e o relatório | `utsim-linhas` |
| Comando | `utsim linha lean_office relatorio modelo.ron` e `--linha` ao abrir | `utsim-cli` |

### 2.2 No núcleo (servem a todas as linhas)

| Peça | Descrição | Também serve a |
|---|---|---|
| Disciplina de fila por rótulo (`ByLabel(nome, Desc/Asc)`) | caixa de entrada por prioridade ou prazo | saúde (triagem), serviços, manufatura |
| Multitarefa da pessoa | `max_open: n` casos abertos por pessoa ao mesmo tempo, com tempo de troca `switch: Dist` cobrado ao alternar; a pessoa reveza os casos em fatias de tempo | serviços, saúde |
| Contagem de visitas e touch time por item | métricas opcionais do item | todas |
| Área por objeto | campo `area: Option<String>` usado nas raias, nas zonas 2D e 3D e nos relatórios por departamento | todas |
| Calendário com feriados | já está no backlog | todas |

A multitarefa é a única peça de comportamento realmente nova. Ela muda como o operador é
usado (hoje uma pessoa atende uma tarefa até o fim), então pede projeto cuidadoso: estados
novos (`switching`), interação com o despachante (a pessoa conta como livre até `max_open`) e
testes de conservação do tempo trabalhado. As interrupções dependem da prioridade e da
preempção do plano de chegadas (E2 e E4).

---

## 3. Outras linhas candidatas

| Linha | Itens e recursos | O que já existe | O que precisaria |
|---|---|---|---|
| **Saúde e hospital** | pacientes, leitos, salas, médicos, enfermagem, exames | pessoas com habilidades, turnos, fluxos, roteamento por rótulo | paciente como item que anda (pessoa-item), triagem por gravidade (disciplina por rótulo), recursos múltiplos ao mesmo tempo (médico e sala), agendamento de consultas, KPIs de espera por gravidade e ocupação de leitos, ativos de hospital |
| **Logística e armazém** | paletes, caixas, pedidos, docas, empilhadeiras, separadores | porta-paletes, esteiras, AGV, rede, corredores, despachante, exemplos de CD | ondas de separação, endereçamento por curva ABC, pedidos com várias linhas (combinação), docas com agenda de caminhões (tabela de chegadas), KPIs de linhas por hora e tempo de doca |
| **Serviços e atendimento** | clientes, guichês, atendentes, senhas | `Source` com horário, `Drop` (desistência), filas | desistência depois de esperar (abandono com paciência sorteada), senha por tipo, painel de chamada, nível de serviço (% em até X min), dimensionamento por hora, multitarefa (atendimento por canais) |
| **Manufatura discreta** | peças, ordens, máquinas, operadores | quase tudo: processador, montagem, setup, quebras, manutenção, OEE | regras de despacho e matriz de setup (backlog), kanban e CONWIP (backlog), ordens com roteiro (tabela de chegadas em sequência), KPIs OEE e tempo de atravessamento por ordem |
| **Processos contínuos** | fluidos, tanques, bombas, lotes de batelada | `utsim-fluid`, tanques, exemplos | receitas de batelada, limpeza entre produtos (CIP), sensores do gêmeo como entrada principal, KPIs de rendimento e tempo de ciclo da batelada, ativos de tanques e tubulação |
| **Ensino** (transversal) | exercícios sobre qualquer linha | modo sala de aula no backlog | enunciado e gabarito por linha, correção automática |

Ordem sugerida depois do Lean Office: **serviços e atendimento** (reaproveita quase toda a
infraestrutura do Lean Office, a multitarefa e a disciplina por rótulo), depois **logística**
(o núcleo já é forte; falta empacotar), depois **saúde** (mais trabalho novo no núcleo).

---

## 4. Primeiros passos para o Lean Office

| # | Tarefa | Horas | Modelo | Depende de |
|---|---|---|---|---|
| 1 | Desenho do pacote de linha: formato de `biblioteca.ron` e `vocabulario.ron`, campos `line`, `preset` e `area`, decisão junto com o `symbol` do fluxograma | 4–6 | Opus | decisões L1 e L2 |
| 2 | Crate `utsim-linhas`: leitor e validação do pacote, aplicar predefinição em `utsim-edit`, testes | 10–14 | Sonnet | 1 |
| 3 | Pacote Lean Office: as 12 predefinições, o vocabulário e o modelo "Aprovação de compras" | 8–12 | Sonnet | 2 |
| 4 | Disciplina de fila por rótulo (prioridade, prazo) | 4–6 | Sonnet | nada |
| 5 | Métricas por item: visitas e touch time (opcionais) | 6–8 | Opus | nada |
| 6 | KPIs do Lean Office e relatório HTML da linha | 12–16 | Opus (definições) + Sonnet (relatório) | 3, 5 |
| 7 | VSM de escritório e raias por área | 8–12 | Sonnet | 1, 6 |
| 8 | Comando `utsim linha` e `--linha` | 3–4 | Sonnet | 6 |
| 9 | Mais três modelos prontos e o tutorial "do mapa ao modelo" | 8–10 | Sonnet | 3, 7 |
| 10 | Multitarefa da pessoa com custo de troca (projeto e núcleo) | 18–26 | Opus | 5 |
| 11 | Ativos gerados por código (mesa, armário, bandeja, pilha, divisória) e vista 2D de planta | 12–18 | Sonnet | entrega do outro agente em `godot/` |
| 12 | Vocabulário e biblioteca da linha na interface Rust/egui | 8–12 | Sonnet | plano da interface Rust |
| 13 | Modelos de projeto "Kaizen de escritório" e "DMAIC transacional" no app Seis Sigma | 6–8 | Sonnet | app Seis Sigma (L0 a L5) |
| 14 | Manual da linha (capítulos com o vocabulário) | 6–8 | Sonnet | 3, 6 |
| 15 | Conferir termos e links do manual da linha | 1–2 | Haiku | 14 |
| 16 | Lote de ativos com Tripo 3D e Blender (cadeira, computador, impressora, pessoas sentadas) | 6–10 mais créditos | Sonnet | **aprovação do gasto pelo dono** |

**Primeira entrega útil** (passos 1 a 8): o dono abre o modelo de aprovação de compras com o
vocabulário, roda e recebe o relatório com lead time, touch time, razão de atividade, %C&A,
FPY, WIP e SLA, mais o VSM de escritório e as raias. São 55–78 h, sem tocar em `godot/` nem em
`crates/utsim-godot`. A multitarefa (10) entra na segunda entrega, junto com as interrupções
(E2 e E4 do plano de chegadas). Total dos passos 1 a 15: 114–162 h, mais o passo 16 se
aprovado.

### Testes

| O quê | Como |
|---|---|
| Predefinições | cada uma gera um objeto válido; modelo montado só com elas roda; ler e gravar sem perda |
| Compatibilidade | modelos sem `line`, `preset` e `area` dão relatório idêntico; snapshots golden sem mudança |
| KPIs | casos pequenos com resposta à mão: uma etapa determinística (L/T = P/T, razão 1); duas etapas com espera conhecida; %C&A de 0,9 em duas etapas dá rolled 0,81 dentro do IC |
| Retrabalho | conservação: casos que entram = concluídos + em aberto; voltas médias = 1/(1 − p) − 1 para devolução com probabilidade p |
| Disciplina por rótulo | ordem de saída da fila igual à ordenação esperada; empates em FIFO |
| Multitarefa | tempo trabalhado + tempo de troca + ocioso = expediente; com `max_open: 1` igual ao comportamento atual |
| VSM e raias | snapshots SVG dos modelos prontos |
| Vocabulário | nenhum termo do núcleo sobra nas telas e relatórios da linha (varredura automática dos textos) |

---

## 5. Requisitos sugeridos

| Id | Requisito |
|---|---|
| RF-269 | O UTSim oferece linhas especializadas como pacotes de domínio (biblioteca de predefinições, vocabulário, modelos prontos, KPIs, relatório, aparência) sobre o mesmo núcleo e formato |
| RF-270 | O modelo pode indicar a sua linha (`line`) e cada objeto a sua predefinição (`preset`) e a sua área (`area`), sem efeito na simulação |
| RF-271 | A linha Lean Office representa casos, caixas de entrada, postos, aprovações, verificações, tramitações, esperas por informação, lotes, reuniões, equipes e pessoas por predefinições de objetos do núcleo |
| RF-272 | A linha Lean Office calcula lead time, tempo de processamento, touch time, razão de atividade, %C&A por etapa, FPY, WIP de casos, % dentro do SLA, ocupação das pessoas e espera por causa, com intervalo de confiança |
| RF-273 | A linha Lean Office gera o VSM de escritório (atual e futuro) e o fluxograma de raias por departamento a partir do modelo e da rodada |
| RF-274 | As filas aceitam ordem por rótulo (prioridade ou prazo) |
| RF-275 | Uma pessoa pode ter vários casos abertos ao mesmo tempo, com custo de troca entre eles |
| RF-276 | O UTSim Seis Sigma oferece modelos de projeto de Kaizen de escritório e DMAIC transacional ligados à linha Lean Office |

---

## 6. Decisões do dono

1. **L1 Linha como pacote no mesmo programa** (proposta) ou como edição separada com binário
   próprio. A proposta mantém um só núcleo e uma só instalação, com entrada própria no launcher.
2. **L2 Campos `preset`, `area` e `symbol`.** Decidir juntos: `preset` (origem na biblioteca),
   `area` (departamento) e `symbol` (símbolo do fluxograma, já no backlog). Proposta: os três,
   opcionais.
3. **L3 Nome exibido.** "UTSim Lean Office" no launcher ou só "Lean Office" dentro do UTSim.
4. **L4 Vista 2D de planta para todas as linhas** (proposta) ou só para o Lean Office.
5. **L5 Multitarefa no núcleo** (seção 2.2): fazer na segunda entrega (proposta) ou já na
   primeira.
6. **L6 Ativos.** Começar só com gerados por código e CC0 (proposta, custo zero) e decidir o
   lote do Tripo 3D depois, com a lista e a estimativa de créditos na mão.
7. **L7 Público inicial do Lean Office.** Ensino (disciplinas de engenharia de produção e
   administração), consultoria de melhoria ou a empresa piloto do Seis Sigma. Define quais
   modelos prontos vêm primeiro.
8. **L8 Segunda linha.** Serviços e atendimento (proposta), logística ou saúde.
9. **L9 Dados reais de sistemas de chamados.** Aceitar exportações CSV genéricas com
   mapeamento de colunas (proposta) ou conectores para sistemas específicos (só se a empresa
   piloto pedir, sem nomear produto no repositório).
