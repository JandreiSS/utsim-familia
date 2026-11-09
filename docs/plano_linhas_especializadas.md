# Plano: linhas especializadas do UTSim, começando pelo Lean Office

Plano técnico, sem implementação, do pedido do dono: ter **linhas** do UTSim para
simulações e estudos em domínios específicos, começando pelo **Lean Office** (processos
administrativos de qualquer área; veja o escopo na seção 1.1). O plano define o que é uma linha, como ela usa o núcleo que já
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
| Modelos de projeto no Seis Sigma | modelos de projeto (kaizen administrativo, por exemplo) | app UTSim Seis Sigma |

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

### 1.1 O que se estuda num processo administrativo

**Escopo (dono, 2026-11-03):** "Lembrando que o Lean Office é tudo ligado ao administrativo,
não apenas à escritórios". A linha trata processos administrativos em geral, e o escritório
não é o centro: o trabalho pode ser presencial, remoto, na caixa de e-mail ou dentro de um
sistema. Áreas típicas: compras e suprimentos; financeiro (contas a pagar e a receber,
faturamento); RH (admissão, folha); comercial (pedido ao faturamento, orçamentos);
atendimento e pós-venda; jurídico e contratos; TI e chamados; planejamento e PCP
administrativo; serviço público (protocolo, processos, licenças); secretaria acadêmica. O
nome exibido continua "Lean Office", o termo consagrado, com o subtítulo "processos
administrativos" (campo `subtitle` da biblioteca).

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
| Caixa de entrada, pasta, caixa de e-mail, lista de trabalho do sistema | `Queue` com disciplina FIFO; nova disciplina por prioridade ou por prazo (`Discipline::ByLabel`) | FIFO existe; prioridade é nova |
| Posto de trabalho (mesa, balcão, tela do sistema) | `Processor` com `operators` de uma equipe de pessoas; capacidade = casos trabalhados ao mesmo tempo; no trabalho remoto ou em sistema o posto fica na posição da pessoa, que não anda | existe |
| Pessoa (analista, aprovador) | `Transporter` com perfil `person` (habilidades, ritmo, aprendizado) dentro de uma equipe (`Dispatcher`) | existe |
| Pessoa como recurso principal, não a mesa | `Processor` de capacidade grande com `operators.count: 1`: o limite real vira o número de pessoas | existe |
| Aprovação com devolução | `Processor` com `OutputRule::Weighted([aprova, devolve])` e ligação de volta | existe |
| Verificação que acha erro de etapa anterior | rótulo `erro` sorteado na etapa que erra (pelo %C&A dela) e `OutputRule::ByLabel` na verificação | existe com script; a predefinição esconde o detalhe |
| Retrabalho (%C&A) | predefinição "Etapa com %C&A": sorteia o erro na saída; a etapa seguinte decide se percebe | novo como predefinição; núcleo existe |
| Lote (papelada, processamento diário no sistema, faturamento em lote) | `Queue.batch` (tamanho, espera máxima) | existe |
| Passagem entre departamentos (malote, e-mail, sistema) | predefinição "Tramitação": `Processor` de capacidade ilimitada com tempo de espera | existe |
| Espera por informação de fora (cliente, outro setor) | `Combiner` que junta o caso com a resposta que chega de outra `Source`, ou "Tramitação" com tempo sorteado | existe |
| Horário de trabalho, almoço, feriados | `calendars` e `shifts` (feriados no backlog) | parcial |
| Fila parada fora do horário | estado `off_shift` | existe |
| Multitarefa (uma pessoa com vários casos abertos) | `multitask` da pessoa: limite de casos abertos (WIP pessoal) e custo de troca entre casos | feito (passo 10) |
| Interrupções (telefone, chat, urgência) | `Source` de interrupções com prioridade alta e preempção do operador (E2 e E4 do plano de chegadas) | depende de E2/E4 |
| SLA, prazo | rótulo `prazo` na criação (C4 do plano de chegadas) e KPI "% dentro do prazo" | depende de C4 |
| Departamento | campo `department` por objeto, usado pelas raias e, para processos com lugar físico, pelo 3D (zonas) | feito (passo 1) |
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

- **VSM administrativo** (`utsim-diagram::vsm` com o vocabulário e campos da linha): caixa de
  processo com P/T, L/T da etapa, %C&A, pessoas e horário; caixas de entrada com casos parados
  (média) e tempo de espera; escada do lead time com touch time; no rodapé, lead time total,
  touch time total, razão de atividade e rolled %C&A. Gera o VSM atual e, com um cenário, o
  futuro, lado a lado.
- **Fluxograma de raias por departamento** (`raias.rs` já aceita áreas): as raias vêm do
  campo de área do objeto; as passagens entre raias são destacadas e contadas; as voltas de
  retrabalho aparecem com a cor própria que o desenho já tem.
- **Carta de processo** com as esperas por causa.
- **Espaguete**, para processos com lugar físico: deslocamentos das pessoas (impressora,
  arquivo, sala do aprovador, balcão) sobre a planta. No trabalho remoto ou em sistema não há
  deslocamento a mostrar.
- **Relatório da linha** (HTML autocontido, como o `html.rs`): resumo dos KPIs da seção 1.5,
  gráfico de WIP de casos no tempo, histograma do lead time com a linha do SLA, Pareto das
  esperas por causa, ocupação das pessoas por hora do dia.

### 1.7 Aparência 3D e 2D

- **Só quando há lugar físico.** Muitos processos administrativos acontecem em sistema e não
  têm planta; para eles bastam o VSM, as raias e o relatório. Quando há lugar (escritório,
  balcão de atendimento, protocolo, secretaria), a vista de planta (de cima) com zonas por
  departamento, mesas e pilhas de casos lê melhor que o 3D. A vista 2D usa a mesma cena com câmera
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
  3. **Tripo 3D e Blender** para peças com cara de ambiente administrativo real, **só depois de perguntar
     ao dono e ele aprovar o gasto** de cada lote de gerações, com a lista do que gerar e a
     estimativa de créditos. Os arquivos gerados passam pela conferência automática de
     escala, eixos e níveis de detalhe proposta no backlog.
- **Cores de estado** do posto e da pessoa iguais às do resto do UTSim; legenda com o
  vocabulário da linha.

### 1.8 Integração com o UTSim Seis Sigma

O Lean Office é o caso mais natural para o app Seis Sigma, porque muitos projetos de melhoria
em empresas são administrativos:

- **Modelos de projeto** no app: "Kaizen administrativo" (5 dias) e "DMAIC transacional",
  com os entregáveis e checklists ajustados (VSM administrativo, %C&A medido, mapa de raias).
- **Importar o modelo da linha** gera o VSM administrativo, as raias e o SIPOC com o
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

Os modelos prontos cobrem áreas diferentes, para mostrar que a linha não é só de escritório.

| Modelo | Área | O que mostra | Situação |
|---|---|---|---|
| Aprovação de compras | compras e suprimentos | três níveis de aprovação por valor, devoluções, reunião semanal do comitê, efeito de alçadas maiores | feito |
| Contas a pagar | financeiro | lote diário de notas, conferência com erro, espera por informação do fornecedor, horário e fechamento do mês | feito |
| Central de solicitações | TI e atendimento | chegadas por hora, triagem por tipo, níveis 1 e 2, SLA por prioridade, atendentes em multitarefa (as interrupções esperam a preempção) | feito |
| Admissão de funcionário | RH | passagens entre RH, TI, financeiro e gestor; espera por documentos do candidato | feito |
| Pedido ao faturamento | comercial e fiscal | trabalho em sistema (ninguém anda), crédito com espera pelo cliente, faturamento em lote, conferência fiscal | feito |
| Tutorial "do mapa ao modelo" | financeiro (reembolso) | montar o VSM administrativo de um processo de 5 etapas, rodar e comparar com o futuro | feito |
| Revisão de contratos | jurídico e contratos | contratos padrão e específicos, advogados em multitarefa, negociação com a contraparte, revisão final e aprovação | feito |
| Licença de funcionamento | serviço público | protocolo, exigência ao requerente, vistoria em lote, parecer, despacho com dois desfechos e publicação, prazo legal | feito |
| Secretaria acadêmica | secretaria acadêmica | pico de pedidos no início do semestre (taxa por período), declarações, históricos com espera pelo professor, ajustes de matrícula | feito |
| Pós-venda com interrupções | atendimento e pós-venda | trocas e garantias, analistas em multitarefa, ligações que interrompem o trabalho (preempção `Full`) | feito |
| Próximos candidatos | contas a receber | cobrança | a fazer |

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
| 7 | VSM administrativo e raias por área | 8–12 | Sonnet | 1, 6 |
| 8 | Comando `utsim linha` e `--linha` | 3–4 | Sonnet | 6 |
| 9 | Mais três modelos prontos e o tutorial "do mapa ao modelo" | 8–10 | Sonnet | 3, 7 |
| 10 | Multitarefa da pessoa com custo de troca (projeto e núcleo) | 18–26 | Opus | 5 |
| 11 | Ativos gerados por código (mesa, armário, bandeja, pilha, divisória) e vista 2D de planta (feito) | 12–18 | Sonnet | entrega do outro agente em `godot/` |
| 12 | Vocabulário e biblioteca da linha na interface Rust/egui (feito) | 8–12 | Sonnet | plano da interface Rust |
| 13 | Modelos de projeto "Kaizen administrativo" e "DMAIC transacional" no app Seis Sigma (feito) | 6–8 | Sonnet | app Seis Sigma (L0 a L5) |
| 14 | Manual da linha (capítulos com o vocabulário) | 6–8 | Sonnet | 3, 6 |
| 15 | Conferir termos e links do manual da linha | 1–2 | Haiku | 14 |
| 16 | Lote de ativos com Tripo 3D e Blender (cadeira, computador, impressora, pessoas sentadas) | 6–10 mais créditos | Sonnet | **aprovação do gasto pelo dono** |

**Situação em 2026-11-03:** passos 1 a 8 feitos e integrados em `feat/fase-1` e `main`
(primeira entrega útil). Campos `line`, `preset`, `department` e `symbol` no modelo; crate `utsim-linhas` com o leitor e a validação dos
pacotes e `utsim_edit::preset`; pacote `linhas/lean_office/` com as 12 predefinições e o
vocabulário; `order: Desc/Asc("rótulo")` na `Queue`; métricas opcionais por item no núcleo
(`Simulation::enable_item_log`); indicadores e relatório HTML da linha; VSM administrativo e
raias por departamento; `utsim linha` (módulo `crates/utsim-cli/src/linha.rs` com o seu
próprio leitor de argumentos) e `utsim run --linha`; modelo pronto
`examples/aprovacao_de_compras.ron`; capítulo do manual da linha.
Próximos: passos 9 a 16 (a multitarefa, 10, na segunda entrega).

**Passo 10 feito em 2026-11-03** (o dono pediu a multitarefa antes dos outros passos da
segunda entrega). Como ficou o projeto:

- **Campo.** `multitask: Some((max_open, slice, switch))` no `Transporter` (a pessoa), não
  no `person`: vale para qualquer membro de equipe. `max_open: 1` é idêntico a não ter o
  campo (teste byte a byte do relatório).
- **Rodízio.** A pessoa trabalha um caso por `slice`, avisa o posto (`MSG_SUSPEND`), fica
  `switch` no estado `Setup` ("trocando de caso"), anda até o posto do próximo caso e manda
  `MSG_ARRIVED` (caso novo) ou `MSG_RESUME_WORK` (caso retomado). Fatias e trocas usam o
  mesmo relógio único do transportador, então quebras e turnos as pausam como já pausavam as
  viagens.
- **Posto.** O `Processor` para o relógio do item enquanto algum operador dele está em outro
  caso; o trabalho sorteado na entrada continua sendo o touch time, e o item não conta como
  servidor ocupado. Um trabalho que termina no mesmo instante em que a fatia acaba termina
  (sem troca inútil).
- **Equipe.** O `Dispatcher` recebe o limite de cada membro (`open_limits`); um pedido de
  operador vai primeiro a um membro livre e, se não há, a um ocupado só com casos e abaixo do
  limite. Ao ficar livre, o membro pega a cabeça da fila e mais casos até o limite; ao fechar
  um caso com outros abertos, avisa (`MSG_CASE_DONE`) e recebe mais um.
- **Fora do rodízio.** Transporte de itens e manutenção (`MAINTENANCE_TAG`) nunca são
  intercalados; preempção nunca tira trabalho de quem tem casos abertos.
- **Estados.** A troca usa o estado `Setup` existente, sem estado novo, para não mudar os
  relatórios dos modelos existentes (golden inalterados). Métricas novas só com o campo:
  `multitask.switches`, `multitask.switching`, `multitask.open`.
- **Lean Office.** Ocupação das pessoas ganhou "trocando de caso"; Pessoa e Equipe ganharam
  o campo "Casos abertos ao mesmo tempo". No modelo pronto com mais mesas que pessoas, três
  casos abertos (fatia de 15 min, troca de 5 min) levam o lead time de cerca de 19,5 h para
  23,3 h com o mesmo touch time.
- **Testes.** Linha do tempo à mão (dois casos de 10 min, fatia 2, troca 1: terminam em 26 e
  29, 9 trocas), conservação do tempo da pessoa e dos casos, touch time igual ao trabalho,
  `max_open: 1` igual a nenhum, validação.

**Passos 9, 14 e 15 feitos em 2026-11-03.** Com o 10, a segunda entrega fica completa no que
não depende de outras frentes; os passos 11 (ativos e vista 2D, depois de `godot/`), 12
(vocabulário e biblioteca na interface Rust) e 13 (modelos de projeto no app Seis Sigma)
esperam as frentes deles, e o 16 espera a aprovação do gasto.

- **Passo 9.** Três modelos prontos da seção 1.9, só com predefinições e com golden novos:
  `examples/contas_a_pagar.ron` (lote diário, conferência, espera pelo fornecedor, pagamento
  parado no fechamento do mês por um calendário de quatro semanas),
  `examples/central_de_solicitacoes.ron` (chegadas por hora com `schedule`, triagem por tipo,
  níveis 1 e 2, prazo por prioridade e atendentes do nível 1 em multitarefa; as interrupções
  continuam à espera da preempção do plano de chegadas) e
  `examples/admissao_de_funcionario.ron` (passagens entre RH, Candidato, TI, Financeiro e
  Gestor, espera pelos documentos, lote de acessos). O tutorial "do mapa ao modelo" monta o
  reembolso de despesas (cinco etapas) com `utsim linha lean_office colocar`, liga, roda e
  compara com o estado futuro (`examples/reembolso_atual.ron` e `reembolso_futuro.ron`: lead
  time de cerca de 9,4 h para menos de 1 h com o mesmo touch time). Todos entram na lista
  `models` da biblioteca.
- **Passo 14.** O capítulo único virou a pasta `docs/manual/src/linhas/lean-office/`, como
  previa a seção 0.1: visão geral, biblioteca, multitarefa, indicadores e relatório, modelos
  prontos, tutorial e pacote da linha, escritos com o vocabulário da linha.
- **Passo 15.** Feito como teste automático, não como revisão única:
  `crates/utsim-linhas/tests/manual_da_linha.rs` confere que nenhum termo geral do UTSim
  (as chaves do vocabulário que são palavras, como "item" e "operador", e os nomes dos
  objetos do núcleo em português) sobra no texto dos capítulos fora do código, que todo link
  leva a um arquivo e a uma seção que existem e que todo capítulo está no sumário; e
  `crates/utsim-cli/tests/linha.rs` roda os comandos do tutorial e confere que montam
  exatamente `examples/reembolso_atual.ron`. O termo de "setup" no vocabulário passou a
  "preparando ou trocando de caso", por causa da multitarefa.

**Nome do campo de departamento.** O plano chamava o campo do objeto de `area`. Na integração
com a fase 1, o modelo ganhou a lista `areas` (áreas de piso com posição e tamanho: sala de
espera, local de pausa, casa do transportador), e `home_at` e `break_at` procuram nomes entre
objetos e essas áreas. Um `area: "Compras"` no objeto pareceria uma referência a uma delas,
então o campo passou a se chamar `department` (opção `--departamento` em `utsim linha
colocar`). Onde este plano diz "campo `area`" ou "área por objeto", leia `department`; as
`areas` do modelo seguem sendo só lugares no piso.

**Passos 11 a 13 feitos em 2026-11-06**, com as dependências já em `feat/fase-1`:

- **Passo 11.** Cinco modelos gerados por código em `utsim-assets` (módulo `office`): `mesa`,
  `armario`, `bandeja`, `pilha_papeis` (com o nó `papeis` para a altura acompanhar os casos) e
  `divisoria`, em `assets/modelos/`. As predefinições gravam o `visual` (posto, aprovação,
  verificação e reunião: mesa; caixa de entrada e pauta: pilha; lote: bandeja), e os modelos
  prontos também. A vista 2D de planta é a câmera "Planta (2D)" do Simular, para todas as
  linhas (decisão L4): projeção ortogonal de cima em `camera_orbita.gd`, uma entrada em
  `CAMERAS` de `ui_left.rs` e o modo em `principal.gd`; roteiro sem janela
  `g4_planta_lean_office.json`. Ficam para depois as zonas por departamento pintadas no chão
  (a interface Godot ainda não desenha as `areas` da cena) e a altura da pilha.
- **Passo 12.** `crates/utsim-godot/src/linha.rs`: as predefinições de cada linha instalada
  viram uma categoria da Biblioteca (id `"<linha>/<predefinição>"`, ícone do tipo do núcleo),
  colocar uma é um só passo de desfazer (`Pack::placement_command`, que também grava `line`),
  o fantasma do arrasto mostra a predefinição inteira e a lista de objetos de um modelo da
  linha usa o vocabulário. Ganchos de poucas linhas em `document.rs` e `ui_class.rs`; roteiro
  sem janela `g4_biblioteca_lean_office.json`.
- **Passo 13.** `utsim_lss_core::modelos_linha`: "Kaizen administrativo" e "DMAIC
  transacional", com o campo `modelo` do projeto acrescentando os itens da linha ao checklist
  de cada tollgate (`tollgate::prepare`); no app, a escolha "Modelo de projeto" no cartão
  Novo projeto (`App::create_line_project`). Os modelos de fluxo da empresa (seção 4.7 do
  plano Seis Sigma) continuam por fazer; estes dois são os primeiros, no código.
- **Modelos da seção 1.9** que faltavam: revisão de contratos (jurídico, com advogados em
  multitarefa), licença de funcionamento (serviço público, com dois desfechos) e secretaria
  acadêmica (pico no início do semestre), com golden novos.
- **Interrupções (2026-11-06):** feitas com a preempção `Full` no núcleo (E4, passo 16 do
  plano de chegadas): predefinição Interrupção (papel `Interruption`, que não conta como caso
  nos indicadores), campo "Interrupções" da Equipe e o modelo pronto
  `examples/pos_venda_com_interrupcoes.ron`; capítulo "Interrupções" no manual.

**Escopo administrativo (2026-11-03).** Junto com os passos 9, 14 e 15, a linha passou a
tratar processos administrativos em geral (seção 1.1): subtítulo "processos administrativos"
na biblioteca (`Library::title`), descrições das predefinições sem supor um lugar, relatório
com o "mapa do fluxo de valor do processo administrativo", manual e plano reescritos, e o
quinto modelo pronto, `examples/pedido_ao_faturamento.ron` (comercial e fiscal, em sistema).

**Primeira entrega útil** (passos 1 a 8): o dono abre o modelo de aprovação de compras com o
vocabulário, roda e recebe o relatório com lead time, touch time, razão de atividade, %C&A,
FPY, WIP e SLA, mais o VSM administrativo e as raias. São 55–78 h, sem tocar em `godot/` nem em
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
| RF-273 | A linha Lean Office gera o VSM administrativo (atual e futuro) e o fluxograma de raias por departamento a partir do modelo e da rodada |
| RF-274 | As filas aceitam ordem por rótulo (prioridade ou prazo) |
| RF-275 | Uma pessoa pode ter vários casos abertos ao mesmo tempo, com custo de troca entre eles (feito: `multitask` do `Transporter`) |
| RF-276 | O UTSim Seis Sigma oferece modelos de projeto de Kaizen administrativo e DMAIC transacional ligados à linha Lean Office |

---

## 6. Decisões do dono

**Decididas em 2026-11-03:** o dono aceitou todas as propostas de L1 a L9. Em L3, como a linha é um pacote do mesmo programa (L1), o nome exibido é "Lean Office" dentro do UTSim.

**Escopo, 2026-11-03:** o Lean Office cobre processos administrativos em geral, não só escritórios (seção 1.1). O nome exibido continua "Lean Office", com o subtítulo "processos administrativos"; vocabulário, descrições e predefinições não supõem um lugar; os modelos prontos cobrem áreas diferentes.

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

---

## 7. Segunda linha: Serviços e atendimento

Plano curto, no formato da seção 1, escrito em 2026-11-07 quando o Lean Office ficou completo
(decisão L8: serviços e atendimento como segunda linha). A linha reaproveita quase toda a
infraestrutura do Lean Office: pacote em `linhas/servicos/`, predefinições, vocabulário,
indicadores com IC, relatório HTML, Biblioteca na interface, multitarefa e preempção `Full`.

### 7.1 O que se estuda num serviço de atendimento

Agências e lojas, cartórios e balcões de serviço público, recepção de clínicas, centrais de
atendimento por telefone, chat e e-mail, balcões de pós-venda. O cliente chega (com horário de
pico), tira uma senha pelo tipo (normal, preferencial, agendado), espera, às vezes desiste, é
chamado num guichê ou atendido num canal. As perguntas típicas:

- quanto o cliente espera e que fração é atendida dentro da meta (nível de serviço, "80 % em
  até 15 minutos");
- quantos desistem e depois de quanto tempo (abandono);
- quantos atendentes são precisos em cada hora do dia (dimensionamento) e qual o efeito de
  mudar a escala, abrir um guichê só no pico, separar preferenciais ou juntar filas;
- o efeito de atender vários canais ao mesmo tempo e de deixar o telefone interromper o chat.

### 7.2 Conceitos mapeados no núcleo

| Conceito | Representação no UTSim | Existe? |
|---|---|---|
| Cliente | item, com rótulos `tipo`, `prioridade` e o canal | existe |
| Chegadas por hora do dia | `Source` com `schedule` (taxa por período) | existe |
| Senha por tipo | rótulo `tipo` na chegada; fila com `order: Desc("prioridade")`; número da senha pela ordem de chegada de cada tipo, no relatório | existe (o número é novo, só no relatório) |
| Fila de espera com desistência | `Queue` com **`patience: Dist`**: cada cliente sorteia a paciência ao entrar e, se ainda estiver na fila quando ela acaba, sai do modelo e conta como abandono | **novo no núcleo** |
| Guichê, mesa de atendimento | `Processor` com `operators` da equipe | existe |
| Atendentes e escala | `Dispatcher` com pessoas e `shifts` | existe |
| Atendimento por canais | multitarefa da pessoa (`multitask`) e telefone com prioridade maior e `preempt: Full` | existe (passos 10 e 16) |
| Painel de chamada | sequência de chamadas (senha, guichê, hora) tirada do rastro da replicação 0 | novo, no relatório |

### 7.3 Biblioteca

Entrada de clientes (com tipos e prioridade), fila de espera (com paciência e ordem por
prioridade), guichê (atendimento por uma pessoa da equipe), canal (entrada, fila e
atendimento de um canal remoto num só passo), equipe de atendentes (com multitarefa e
interrupções), atendente e saída. Visual: guichê como mesa, fila como área no chão.

### 7.4 Vocabulário

item → cliente; `Queue` → fila de espera; `Processor` → guichê; `Transporter` → atendente;
`Dispatcher` → equipe de atendimento; `time_in_system` → tempo no atendimento; `idle` →
livre; `waiting_operator` → esperando o atendente.

### 7.5 Indicadores

Por replicação, com IC de 95 %: clientes atendidos e abandonos (contagem e %), tempo médio
de espera (TME) e de atendimento (TMA), espera do 90º percentil, **nível de serviço** (% dos
atendidos que esperaram até a meta; a meta é uma opção do relatório, padrão 15 min), por tipo
de senha; tempo até o abandono; ocupação dos atendentes por hora; **dimensionamento por
hora**: chegadas por hora, TMA e o número de atendentes que o modelo de fila M/M/c (Erlang C)
pede para cumprir a meta em cada hora, ao lado da ocupação simulada.

### 7.6 Relatórios

`utsim linha servicos relatorio` com texto e HTML: cartões, tabela por tipo de senha,
histograma da espera com a linha da meta, abandonos por tempo de espera, ocupação por hora,
tabela de dimensionamento e o **painel de chamada** (as primeiras chamadas da replicação 0:
hora, senha, guichê, espera).

### 7.7 Modelos prontos

| Modelo | O que mostra |
|---|---|
| Agência com senhas | guichês, senha normal e preferencial, pico do almoço, abandono |
| Central multicanal | telefone, chat e e-mail na mesma equipe, multitarefa, telefone interrompendo o chat |
| Atendimento ao cidadão | triagem, serviços por tipo de senha, agendados com prioridade, meta legal de espera |

### 7.8 Primeiros passos (sem decisão pendente)

| # | Tarefa | Horas | Modelo |
|---|---|---|---|
| S1 | `patience` na `Queue` (abandono), testes à mão, golden inalterados | 4–6 | Opus |
| S2 | Pacote `linhas/servicos/` (predefinições e vocabulário) | 3–4 | Sonnet |
| S3 | Indicadores, nível de serviço, painel de chamada e dimensionamento (Erlang C) | 8–12 | Opus |
| S4 | Relatório da linha e `utsim linha servicos relatorio` / `--linha servicos` | 4–6 | Sonnet |
| S5 | Três modelos prontos, golden e capítulo do manual | 6–8 | Sonnet |

**Situação em 2026-11-07:** feitos os passos S1 a S5 com as propostas das decisões abaixo
(o dono ainda não respondeu; mudar qualquer uma é pequeno). `patience` na `Queue` (abandono,
`crates/utsim-model/tests/abandono.rs` com casos à mão); pacote `linhas/servicos/` com 7
predefinições e o vocabulário; módulo `utsim_linhas::servicos` com os indicadores, o painel de
chamada e o dimensionamento por Erlang C (testes à mão, inclusive a fórmula); `utsim linha
servicos relatorio [--meta M] [--alvo A] [--html] [--json]` e `utsim run --linha servicos`; os
três modelos prontos com golden novos; capítulo `docs/manual/src/linhas/servicos/index.md`. A
espera do cliente é o tempo no modelo menos o atendimento sorteado (conta a espera no guichê
por um atendente e a interrupção, não só a fila). Os atendentes dos modelos prontos saem de
turno com `Finish` e ficam meia hora depois de fechar a porta.

### 7.9 Decisões do dono

**Respondidas em 2026-11-08:** o dono aceitou as seis propostas ("Siga com isso"). Ficam como
estão no código: nome "Serviços e atendimento", meta padrão de 80 % em até 15 min, quem
desiste sai do modelo, painel de chamada só no relatório, dimensionamento por Erlang C e os
três modelos prontos da seção 7.7. Painel ao vivo, Erlang A e quem volta outro dia ficam para
depois.

1. **S-D1 Nome exibido.** "Serviços e atendimento" (proposta) ou outro.
2. **S-D2 Meta padrão do nível de serviço.** 80 % em até 15 min no presencial (proposta), com
   a meta mudável no relatório; ou outra.
3. **S-D3 Quem desiste.** Sai do modelo e conta como abandono (proposta) ou vai a uma saída
   ligável ("desistiu"), para modelar quem volta outro dia.
4. **S-D4 Painel de chamada na interface.** Só no relatório agora (proposta) ou também um
   painel ao vivo na interface Godot (pede trabalho na frente da interface).
5. **S-D5 Dimensionamento.** Erlang C, sem abandono, que é conservador (proposta), ou
   Erlang A, com abandono, depois.
6. **S-D6 Modelos prontos.** Os três da seção 7.7 (proposta) ou outros (clínica, loja,
   cartório).

---

## 8. Terceira linha: Logística e armazém

Plano curto, no formato das seções 1 e 7, escrito em 2026-11-08 (ordem da seção 3: depois de
serviços, logística). O núcleo já é forte aqui (porta-paletes, empilhadeiras, AGV, rede,
corredores, despachante, tabela de chegadas, exemplos de CD); falta empacotar e completar
quatro peças pequenas. Vale a direção do dono de 2026-11-08 (`docs/plano_integracao_familia.md`,
seção 3): manutenção das empilhadeiras e dos AGVs fica no `downtime` do próprio nó, EPI e
zonas de risco no `ppe` e nas `zones`, qualidade (avaria, conferência) no `quality` e no
`inspection` do nó. A linha não cria dados paralelos: as predefinições preenchem esses
campos e os apps da família editam os mesmos.

### 8.1 O que se estuda num armazém

Recebimento (caminhões nas docas com agenda), armazenagem (endereçamento, ocupação do
porta-paletes), separação (por pedido, em ondas, por zona), consolidação e expedição.
Perguntas típicas: quantas linhas por hora a separação faz e com quantas pessoas; quanto
tempo o caminhão fica na doca e no pátio; se as ondas cabem no corte da transportadora; o
ganho de pôr os itens de giro alto (curva A) perto da frente; quantas empilhadeiras e
docas são precisas.

### 8.2 Conceitos mapeados no núcleo

| Conceito | Representação no UTSim | Existe? |
|---|---|---|
| Caminhão com agenda | `Source` com `table` (horários e rótulo `paletes`) | existe |
| Pátio | fila | existe |
| Doca e descarga por paletes | **`SplitBy(count: "paletes", group: "caminhao")` no `Separator`**, com as empilhadeiras no `transport`: o caminhão vira os seus paletes e ocupa a doca até sair o último (uma doca por divisor) | **novo** |
| Palete, caixa, pedido | item com rótulos (`classe` A/B/C, `linhas`, `prazo`) | existe |
| Pedido com várias linhas | **`SplitBy(count, group)` no `Separator`**: uma linha por unidade do rótulo `linhas`, todas com o rótulo `pedido` | **novo** |
| Consolidação do pedido | **`gather` na `Queue`**: segura as linhas até chegarem todas do mesmo `pedido` e solta o pedido | **novo** |
| Onda de separação | **`waves` na `Queue`**: junta os pedidos e solta todos nos horários das ondas | **novo** |
| Endereçamento por curva ABC | **`Abc` no `placement` do `Rack`** (zonas pela fração das posições) e **`access`** (tempo por coluna e por nível) | **novo** |
| Empilhadeira, separador | `Transporter` (empilhadeira ou pessoa) e `Dispatcher` | existe |

### 8.3 Biblioteca

Chegada de caminhões (agenda), pátio, doca (que desmonta o caminhão em paletes),
porta-paletes (com ABC), entrada de pedidos (com linhas), onda de separação, divisão em
linhas, separação, consolidação, expedição, empilhadeiras e separadores (equipe e pessoa).

### 8.4 Vocabulário

item → palete ou pedido conforme o objeto (o vocabulário geral usa "carga"); `Processor` →
estação; `Rack` → porta-paletes; `Transporter` → empilhadeira ou separador; `Dispatcher` →
equipe; `time_in_system` → tempo no armazém.

### 8.5 Indicadores

Com IC de 95 % entre replicações: **linhas separadas por hora** (por hora do dia e por hora
de operação), pedidos expedidos por dia, linhas por pedido, tempo de ciclo do pedido e %
expedido no prazo; **tempo de doca** do caminhão (espera no pátio, na doca, total) e % dentro
da meta (padrão 60 min); ocupação das docas, das empilhadeiras e dos separadores; ocupação
média do porta-paletes; tempo de acesso por classe ABC.

### 8.6 Relatório

`utsim linha logistica relatorio` com texto, HTML e JSON: cartões, recebimento (doca),
separação (linhas por hora em gráfico por hora do dia), expedição (ciclo do pedido e prazo),
armazenagem (ocupação e ABC) e equipes.

### 8.7 Modelos prontos

| Modelo | O que mostra |
|---|---|
| CD com recebimento e expedição | agenda de caminhões, docas, descarga por paletes, armazenagem ABC, separação e expedição |
| Separação em ondas | pedidos de e-commerce com várias linhas, ondas por horário, consolidação e corte da transportadora |
| Cross-docking | paletes que passam da doca de entrada à de saída sem armazenar |

### 8.8 Primeiros passos (sem decisão pendente)

| # | Tarefa | Horas | Modelo |
|---|---|---|---|
| G1 | `SplitBy`, `gather`, `waves`, ABC e `access` no núcleo, com testes à mão; golden inalterados | 12–16 | Opus |
| G2 | Pacote `linhas/logistica/` (predefinições com manutenção, EPI e qualidade nos campos do nó) | 3–4 | Sonnet |
| G3 | Indicadores e relatório da linha, `utsim linha logistica relatorio` e `--linha logistica` | 8–10 | Opus |
| G4 | Três modelos prontos, golden e capítulo do manual | 6–8 | Sonnet |

**Estado em 2026-11-08: G1 a G4 feitos.** A doca ficou como divisor `SplitBy` em vez de um
`per_label` no processo: o caminhão ocupa a doca até as empilhadeiras tirarem o último palete,
o que dá o tempo de doca sem campo novo no `Processor`. Manual em
`docs/manual/src/linhas/logistica/index.md`.

### 8.9 Decisões do dono

**Respondidas em 2026-11-08:** o dono aceitou as seis propostas ("Siga com isso"). Ficam como
estão no código: nome "Logística e armazém", tempo de acesso por coluna e por nível, ondas por
horário fixo, só o pedido completo é expedido, meta de doca de 60 min e os três modelos prontos
da seção 8.7. Rotas reais nos corredores, ondas dinâmicas e expedição parcial ficam para
depois.

1. **G-D1 Nome exibido.** "Logística e armazém" (proposta).
2. **G-D2 Tempo no porta-paletes.** Tempo de acesso por coluna e nível da posição (proposta,
   simples e suficiente para a curva ABC) ou a rota real do separador nos corredores (mais
   fiel, bem mais trabalho).
3. **G-D3 Ondas.** Por horário fixo do dia (proposta) ou por quantidade de pedidos e janela
   dinâmica.
4. **G-D4 Expedição.** O pedido só sai completo (proposta) ou pode sair parcial.
5. **G-D5 Meta do tempo de doca.** 60 minutos por caminhão (proposta), mudável no relatório.
6. **G-D6 Modelos prontos.** Os três da seção 8.7 (proposta) ou outros (armazém frigorífico,
   centro de distribuição de varejo com loja, armazém com AGV).

---

## 9. Quarta linha: Saúde e hospital

Plano curto, no formato das seções 7 e 8, escrito em 2026-11-09 (ordem da seção 3: depois de
logística, saúde). Vale a direção do dono de 2026-11-08 (`docs/plano_integracao_familia.md`,
seção 3): a parada dos equipamentos (tomógrafo, raio X) fica no `downtime` do próprio nó, o EPI
da equipe no `ppe` e nas `zones`, a conferência (dupla checagem de medicação) no `quality` do
nó. A linha não cria dados paralelos.

### 9.1 O que se estuda num serviço de saúde

Pronto-socorro (chegada, recepção, classificação de risco, consulta, exames, medicação e
observação, alta ou internação), ambulatório (consultas agendadas, faltas e encaixes) e
internação (leitos, tempo de permanência, giro de leito). Perguntas típicas: quanto o paciente
espera até o médico em cada cor da classificação e quantos ficam dentro do tempo-alvo; quantos
vão embora sem atendimento; quantos médicos e enfermeiros por turno; quantos leitos para a
ocupação ficar abaixo de 85 %; o efeito das faltas e dos encaixes na agenda.

### 9.2 Conceitos mapeados no núcleo

| Conceito | Representação no UTSim | Existe? |
|---|---|---|
| Paciente | item com rótulos (`gravidade` 1 a 5, `agendado`) | existe |
| Chegada espontânea por hora do dia | `Source` com `schedule` (taxa por período) | existe |
| Agenda de consultas | `Source` com `table` | existe |
| Classificação de risco | processo de triagem com a enfermagem; a cor é sorteada na chegada (rótulo `gravidade`) e a triagem só consome tempo | existe |
| Fila por gravidade | `order: Asc("gravidade")` na `Queue` (1 vermelho primeiro) | existe |
| Saiu sem atendimento | `patience` na `Queue` (rótulo `abandonou`) | existe |
| Médico e enfermagem | `Transporter` (pessoa) e `Dispatcher` com habilidades (`requires`) e prioridade pelo rótulo (`priority_label`) | existe |
| Sala com médico e enfermeiro ao mesmo tempo | **`also` no `Processor`**: outras equipes pedidas junto com `operators`; a etapa começa quando todas chegam | **novo** |
| Tempo diferente por cor | roteamento `ByLabel("gravidade")` para salas com tempos próprios, a mesma equipe | existe |
| Exames | processos com `downtime` do equipamento | existe |
| Leito | `Processor` com `capacity` = leitos e tempo de permanência | existe |

### 9.3 Biblioteca

Chegada de pacientes (com as cores), agenda de consultas, recepção, classificação de risco,
fila por gravidade (com a desistência), consultório, sala de procedimento (médico e
enfermagem juntos), exame (laboratório e imagem, com parada do equipamento), medicação e
observação, leitos de internação, alta, transferência; equipes de médicos e de enfermagem
(equipe e pessoa, com EPI).

### 9.4 Vocabulário

item → paciente; `Source` → chegada; `Queue` → espera; `Processor` → atendimento;
`Transporter` → profissional; `Dispatcher` → equipe; `Sink` → alta; `time_in_system` →
permanência.

### 9.5 Indicadores

Com IC de 95 % entre replicações: pacientes atendidos por dia; **espera até o médico** (porta
ao médico) média e por cor; **% dentro do tempo-alvo por cor** (padrão: vermelho 0, laranja
10, amarelo 60, verde 120, azul 240 min); **saída sem atendimento** (% e por cor);
**permanência** no serviço por cor; ocupação dos médicos e da enfermagem; **ocupação dos
leitos** e permanência na internação; pacientes no serviço por hora do dia (censo).

### 9.6 Relatório

`utsim linha saude relatorio` com texto, HTML e JSON: cartões, tabela por cor (espera, % no
alvo, saídas sem atendimento, permanência), gráfico do censo por hora do dia, equipes e
leitos.

### 9.7 Modelos prontos

| Modelo | O que mostra |
|---|---|
| Pronto-socorro com classificação de risco | chegada por hora do dia, cinco cores, consulta, exames com parada do tomógrafo, medicação, sala de procedimento com médico e enfermagem, internação |
| Ambulatório com agenda | consultas agendadas com faltas, encaixes, especialidades por habilidade |
| Internação e leitos | entradas do pronto-socorro e eletivas, tempo de permanência, ocupação e espera por leito |

### 9.8 Primeiros passos (sem decisão pendente)

| # | Tarefa | Horas | Modelo |
|---|---|---|---|
| H1 | `also` no `Processor` (várias equipes ao mesmo tempo), com testes à mão; golden inalterados; painel 3D | 6–8 | Opus |
| H2 | Pacote `linhas/saude/` (predefinições com parada, EPI e conferência nos campos do nó) | 3–4 | Sonnet |
| H3 | Indicadores e relatório da linha, `utsim linha saude relatorio` | 8–10 | Opus |
| H4 | Três modelos prontos, golden e capítulo do manual | 6–8 | Sonnet |

**Estado em 2026-11-09: H1 a H4 feitos.** O `also` serve a qualquer processo; quem chega
primeiro espera os outros na estação, então o modelo pronto dá prioridade ao pedido do médico
na sala de procedimento. Manual em `docs/manual/src/linhas/saude/index.md`.

### 9.9 Decisões do dono

As propostas seguem no código até o dono responder.

1. **H-D1 Nome exibido.** "Saúde e hospital" (proposta).
2. **H-D2 Classificação de risco.** Cinco cores com os tempos-alvo da seção 9.5 (proposta,
   mudáveis no relatório) ou outra escala.
3. **H-D3 Cor do paciente.** Sorteada na chegada, a triagem só consome tempo (proposta,
   simples) ou decidida na triagem (precisa de rótulo gravado por um processo).
4. **H-D4 Leito durante os exames.** O leito é o tempo de permanência, exames antes da
   internação (proposta) ou o paciente segura o leito enquanto sai para exames (recurso
   preso, mais trabalho no núcleo).
5. **H-D5 Ocupação-alvo dos leitos.** 85 % (proposta).
6. **H-D6 Modelos prontos.** Os três da seção 9.7 (proposta) ou outros (centro cirúrgico,
   UTI, laboratório de análises).

---

## 10. Quinta linha: Manufatura discreta

Plano curto, no formato das seções 7 a 9, escrito em 2026-11-09 (a seção 3 não ordena as
linhas que restam depois de saúde; esta vem primeiro porque o núcleo já cobre quase tudo).
Vale a direção do dono de 2026-11-08 (`docs/plano_integracao_familia.md`, seção 3): a quebra
de cada máquina fica no `downtime` do próprio nó, o EPI no `ppe` e nas `zones`, a inspeção e o
refugo no `quality` do nó e nas saídas com `sink_role`. A linha não cria dados paralelos.

### 10.1 O que se estuda numa fábrica de peças

Ordens de produção que passam por máquinas (corte, usinagem, montagem, inspeção), com mistura
de produtos, troca de produto (setup) que depende da sequência, quebras e manutenção,
operadores, refugo e retrabalho. Perguntas típicas: qual o OEE de cada máquina e o que mais o
derruba (parada, troca ou ritmo); quantas ordens saem no prazo; o efeito de juntar ordens do
mesmo produto (menos trocas) contra atender pela data de entrega; quanto material em processo
um limite de cartões (kanban, CONWIP) segura e quanto custa em vazão; onde está o gargalo.

### 10.2 Conceitos mapeados no núcleo

| Conceito | Representação no UTSim | Existe? |
|---|---|---|
| Ordem de produção | item com rótulos (`produto`, `prazo` em minutos desde a liberação) | existe |
| Mistura de produtos | `labels` com `Discrete` na liberação | existe |
| Regra de despacho | `order` da `Queue`: chegada (FIFO), `Asc("vence")` (data de entrega), `Asc("produto")` (juntar produtos), `Desc("prioridade")` | existe |
| Data de entrega absoluta | `on_entry` na liberação: `item.vence = now + item.prazo` | existe |
| Troca de produto dependente da sequência | **`changeover` no `Processor`**: o `setup` só acontece quando o rótulo do produto muda; a matriz dá o tempo de cada troca (de, para) | **novo** |
| Quebra e manutenção | `downtime` do nó | existe |
| Kanban e CONWIP | cartões que circulam: o `Combiner` junta cartão e ordem, o `Separator` devolve o cartão ao fim | existe |
| Roteiro por produto | `ByLabel("produto")` ou `Script` com o passo do roteiro | existe |
| Inspeção, refugo e retrabalho | `quality` do nó e `sink_role: Scrap` | existe |

### 10.3 Biblioteca

Liberação de ordens (mistura de produtos e prazo), fila de ordens pela regra de despacho,
máquina com troca de produto, máquina simples, montagem, inspeção, buffer (supermercado),
quadro de cartões (kanban e CONWIP), refugo, expedição; equipe de operadores e operador (com
EPI).

### 10.4 Vocabulário

item → ordem; `Source` → liberação; `Queue` → buffer; `Processor` → máquina; `Transporter` →
operador; `Dispatcher` → equipe; `Sink` → expedição; `setup` → troca; `time_in_system` →
tempo de atravessamento.

### 10.5 Indicadores

Com IC de 95 % entre replicações: ordens por dia (no total e por produto); **tempo de
atravessamento** médio e por produto; **material em processo** médio; **ordens no prazo** e
**atraso médio** das atrasadas (com o rótulo `prazo`); **OEE por máquina** com disponibilidade
(tempo sem quebra e sem troca ÷ tempo de turno), desempenho (tempo-padrão × peças ÷ tempo
rodando, tempo-padrão = média do `process` ÷ capacidade) e qualidade (boas ÷ todas, pelas
saídas `Scrap`); **trocas por dia** e fração do tempo em troca por máquina; ocupação dos
operadores.

### 10.6 Relatório

`utsim linha manufatura relatorio` com texto, HTML e JSON: cartões, tabela de OEE por máquina
com as três perdas, tabela por produto (ordens, atravessamento, no prazo) e ocupação dos
operadores.

### 10.7 Modelos prontos

| Modelo | O que mostra |
|---|---|
| Usinagem com troca de produto | três produtos, matriz de troca, regra "juntar produtos" contra "data de entrega", quebras e OEE |
| Linha com CONWIP | cartões que limitam o material em processo, gargalo, efeito do número de cartões na vazão e no atravessamento |
| Oficina por ordens | roteiro diferente por produto, operadores com habilidades, inspeção com refugo e retrabalho |

### 10.8 Primeiros passos (sem decisão pendente)

| # | Tarefa | Horas | Modelo |
|---|---|---|---|
| M1 | `changeover` no `Processor` (troca só quando o produto muda, matriz de tempos), com testes à mão; golden inalterados; painel 3D | 6–8 | Opus |
| M2 | Pacote `linhas/manufatura/` (quebra, EPI e inspeção nos campos do nó) | 3–4 | Sonnet |
| M3 | Indicadores e relatório da linha, `utsim linha manufatura relatorio` | 8–10 | Opus |
| M4 | Três modelos prontos, golden e capítulo do manual | 6–8 | Sonnet |

### 10.9 Decisões do dono

As propostas seguem no código até o dono responder.

1. **M-D1 Nome exibido.** "Manufatura discreta" (proposta) ou "Fábrica e usinagem".
2. **M-D2 Primeira peça.** A máquina começa pronta para o primeiro produto que chega, sem
   troca (proposta) ou começa sem produto e troca já na primeira peça.
3. **M-D3 Troca com capacidade maior que 1.** Vale o último produto que começou na máquina
   (proposta, simples) ou cada posto guarda o seu.
4. **M-D4 Desempenho do OEE.** Tempo-padrão = média do `process` (proposta) ou um campo
   próprio de tempo de ciclo ideal.
5. **M-D5 Qualidade do OEE.** Boas ÷ todas do modelo inteiro, pelas saídas `Scrap` (proposta)
   ou por máquina, pela inspeção logo depois dela.
6. **M-D6 Modelos prontos.** Os três da seção 10.7 (proposta) ou outros (montagem com
   kitting, linha balanceada, célula em U).
