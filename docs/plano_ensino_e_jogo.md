# Plano: app de ensino da família UTSim e o jogo de problemas da indústria

Este é um plano técnico, sem implementação, de um aplicativo próprio da família UTSim para o ensino de engenharia de produção pela simulação. O núcleo do app é um **jogo** em que o aluno resolve problemas reais da indústria. O pedido do dono (2026-11-03, marcado como muito importante) tem três partes. O aluno aprende sem precisar usar o simulador no começo. A trilha vai do básico (gargalo, filas, balanceamento de linha, arranjo físico, capacidade, tempos e métodos) aos problemas complexos. O fim é um caso que exige Lean Seis Sigma, ligado ao app UTSim Seis Sigma.

O nome provisório é **UTSim Ensino**. O launcher já reserva para ele o produto `ensino`, com o subtítulo "Ensino pela simulação" e o ícone `assets/icone/familia/ensino_256.png` (`crates/utsim-launcher/src/catalogo.rs`). As crates novas levam o prefixo `utsim-ensino`. O formato ainda não foi escolhido pelo dono. A seção 2 traz as opções, e a seção 10 lista as decisões E1 a E16 com a proposta de cada uma.

Cada seção diz o que o app faz, onde mora o cálculo e como é testado. A seção 9 traz as estimativas em horas Opus 5.5 e o modelo de IA de cada passo. Os requisitos (prefixo `ENS-RF-nn` e `ENS-RNF-nn`) só serão escritos depois das decisões e ficam fora de `docs/requisitos.md` até o dono aprovar.

Documentos relacionados, que este plano não repete:

- `docs/plano_lean_seis_sigma.md`, seção 8.6. É o antigo modo de ensino (L8), que saiu do UTSim Seis Sigma pela decisão N2 e passa para cá. A seção 8.5 descreve as provas com sorteio reprodutível, que este app reaproveita.
- `docs/plano_linhas_especializadas.md`. Descreve o pacote de linha e o Lean Office, fonte de missões administrativas.
- `docs/plano_launcher.md`. Descreve os produtos, os canais, o acesso aberto (seção 13) e o Drive.
- `docs/plano_interface_rust.md`. Descreve o egui dentro do cliente Godot.
- `docs/estudos_de_caso/` e `docs/manual/src/tutoriais/`. São o conteúdo de partida.
- `docs/backlog_v02.md`, seção "Ensino" e seção "Engenharia de qualidade e segurança do trabalho".

---

## 0. Ponto de partida e princípios

### 0.1 O que já existe e será reaproveitado

| Peça | Onde | Uso no app de ensino |
|---|---|---|
| Motor de eventos, objetos e modelo `.ron` | `utsim-core`, `utsim-objects`, `utsim-model` | toda missão é um modelo UTSim rodado sem janela |
| Mudança de campo por chave (`<objeto>.<campo>`, `run.<campo>`, calendários, fluxos) | `utsim_model::experiment::{patch, apply_patches}` | cada alavanca do jogo vira um conjunto de chaves e valores sobre o modelo base |
| Cenários com números aleatórios comuns, diferença pareada | `utsim_model::experiment::{Experiment, run_scenarios}` | comparar a decisão do aluno com o estado inicial e com a solução de referência |
| Estatísticas por objeto, gargalo, gargalo móvel, Little, aquecimento | `utsim-model/src/analysis.rs`, `analysis/advanced.rs`, `utsim-stats` | métricas da nota, dicas automáticas, perguntas de previsão |
| Gravação de uma replicação com o rastro (`Replay`) | `utsim-model/src/replay.rs` | animação 2D da missão e "ver em 3D" |
| Otimização com comparação pareada | `utsim-opt` | cálculo da solução de referência de cada missão |
| Estudo homem-máquina (`utsim tempos`), matriz de-para, VSM, SIPOC, raias, carta de processo | `utsim-diagram` (`tempos/`, `depara.rs`, `vsm.rs`, `layout.rs`) | missões de tempos e métodos e de arranjo físico, figuras dos enunciados |
| Intervalos, testes, ajuste de distribuições | `utsim-stats`, `utsim-fit` | tolerância da nota e geração de dados didáticos |
| CEP, capacidade, MSA | `utsim-quality` | caso final Lean Seis Sigma e pacotes da frente de qualidade |
| Pacote de linha (biblioteca, vocabulário, modelos prontos) | `utsim-linhas`, `linhas/lean_office/` | missões administrativas com o vocabulário da linha |
| Provas por sorteio com semente, problema prático calculado pelas crates | `utsim-lss-core/src/formacao/exams.rs` | modo prova (o mesmo desenho, com os tipos de questão do jogo) |
| Diário de operações em pasta sincronizada, sem servidor | `utsim-lss-store` | turma no Drive, entregas dos alunos |
| Visual da família em egui | `utsim-ui-tema` (egui `=0.36.2`, `egui_plot =0.37.0`) | mesma aparência do launcher, do Seis Sigma e da interface do Godot |
| Ponte egui dentro do Godot | `crates/utsim-godot` (`egui_host.rs`) | caminho para o modo 3D (seção 2) |
| Três estudos de caso com gabarito e dados com semente | `docs/estudos_de_caso/01` a `03` | missões do ato de problemas integrados |
| Modelos de exemplo | `examples/*.ron`, `examples/qualidade/*.csv` | base das missões (seção 3.3) |
| Produto `ensino` e acesso aberto por canal | `utsim-launcher`, `utsim-publicar acesso` | distribuição aos alunos sem código por pessoa |

### 0.2 Princípios

- **A simulação é o juiz.** Toda nota vem de métricas do núcleo, calculadas com replicações e intervalo de confiança. Nada é digitado como gabarito. A resposta esperada é recalculada pelas crates, como na decisão do problema prático das provas do Seis Sigma.
- **Primeiro decidir, depois modelar.** No começo o aluno mexe em alavancas de uma fábrica pronta e vê o efeito. Montar o modelo vem depois, quando ele já entende o que está modelando (seção 3.2).
- **Prever antes de rodar.** Sempre que fizer sentido, a missão pede uma previsão (qual é o gargalo, qual vai ser a fila) antes da simulação. A previsão vale nota própria. Isso troca a tentativa e erro pelo raciocínio.
- **Cálculo no Rust, tela fina.** Missão, alavanca, nota, dica e progresso moram numa crate pura, testada sem janela. A interface só desenha.
- **Reprodutível.** A mesma missão, a mesma semente, as mesmas decisões e a mesma versão do núcleo dão o mesmo número na máquina do aluno e na do professor.
- **Custo zero.** Não há servidor nem banco. As turmas usam uma pasta compartilhada no Google Drive do professor ou um arquivo de entrega trocado à mão, como o launcher e o Seis Sigma.
- **Texto próprio.** Enunciados, histórias, dicas e dados são escritos para o UTSim. Nada é copiado de livro, e nenhum outro programa de simulação nem jogo comercial é citado.
- **Aberto a pacotes.** As linhas especializadas, a frente de qualidade e segurança e o próprio professor acrescentam missões pelo mesmo formato de pacote (seção 6.6).

---

## 1. Objetivos e público

### 1.1 Objetivos

1. Ensinar os conceitos centrais da engenharia de produção pela consequência das decisões numa fábrica simulada, com variabilidade real, e não só por fórmula.
2. Levar o aluno do uso de alavancas à construção do próprio modelo no UTSim, em degraus.
3. Fechar a trilha com um caso DMAIC completo, que leve o aluno ao UTSim Seis Sigma com dados e modelo prontos.
4. Dar ao professor exercícios corrigidos automaticamente, um painel da turma e um modo prova, sem servidor.
5. Servir de porta de entrada para a família UTSim. Quem termina a campanha sabe o que o simulador e o Seis Sigma fazem.

### 1.2 Público

| Público | O que usa | Prioridade proposta |
|---|---|---|
| Aluno de graduação em engenharia de produção (disciplinas de PCP, arranjo físico, tempos e métodos, simulação, qualidade) | campanha, caixa de areia, entregas da turma | primeira (aulas do dono na UTFPR) |
| Professor | pacotes, turma, painel, prova, construtor de casos | primeira, junto com o aluno |
| Aluno de pós-graduação e curso técnico | campanha a partir do ato 3, construtor de casos | segunda |
| Formação de belts na empresa (yellow e green) | atos 1 a 3 e caso final, com registro de competência no Seis Sigma | segunda |
| Estudante autodidata | campanha individual | sem esforço próprio, vem de graça |

### 1.3 Fora do escopo

- Edição completa de modelos. Quem precisa dela abre o UTSim (seção 7.4).
- Ambiente virtual da disciplina com fórum, chat ou calendário. O app exporta as notas em CSV para o ambiente que a instituição já usa.
- Jogo em rede em tempo real. As competições são assíncronas, pela pasta da turma (seção 2.4).

---

## 2. Formato: opções e recomendação

### 2.1 Onde o jogo roda

| Opção | Como é | A favor | Contra |
|---|---|---|---|
| **A. App egui próprio** | binário `utsim-ensino` com eframe (glow), como o launcher e o Seis Sigma. Núcleo sem janela e vista 2D esquemática animada pelo `Replay` | leve (abre em computador de laboratório sem placa de vídeo), um só egui no `Cargo.lock`, testável sem motor, reaproveita telas e tema do Seis Sigma, entrega rápida | sem 3D, menos apelo visual |
| **B. Jogo dentro do cliente Godot 3D** | modo "missão" no cliente `utsim-3d`, com o painel egui pela ponte | fábrica em 3D, pessoas e empilhadeiras andando, mais apelo | pacote grande, exige placa de vídeo, depende do ritmo do plano do Godot, mais difícil de testar, aluno herda a complexidade do editor |
| **C. Híbrido** | o jogo é o app A. As telas do jogo ficam numa crate egui sem backend (`utsim-ensino-ui`), que também pode ser hospedada na ponte do Godot. O botão "ver em 3D" abre o `Replay` da missão no cliente 3D quando ele está instalado | começa leve e ganha 3D sem reescrever, uma só lógica de jogo | exige disciplina para manter a crate de telas sem dependência de eframe |

### 2.2 Uso individual e em sala

- **Campanha individual.** O aluno joga sozinho, salva o progresso localmente e pode exportar uma entrega.
- **Modo sala de aula.** O professor cria uma turma, publica pacotes com prazos e recebe as entregas. O painel mostra a matriz aluno por missão (seção 4.4). Esse modo é o que o backlog chama de "modo sala de aula".

As duas coisas usam o mesmo motor de missão. A sala de aula só acrescenta a pasta da turma e as telas do professor.

### 2.3 Turma offline ou sincronizada pelo Drive

| Opção | Como funciona | Quando serve |
|---|---|---|
| **Offline** | o professor exporta um pacote `.utse` e o distribui pelo ambiente da disciplina ou por pen drive. O aluno devolve um arquivo de entrega `.utsr`, assinado e com o diário das tentativas. O professor importa as entregas de uma pasta | laboratório sem Drive, prova presencial, turma grande |
| **Pasta no Drive** | pasta da turma compartilhada pelo professor, com `pacotes/` só para leitura e uma subpasta por aluno compartilhada só com o professor. O app não usa a API do Drive, só lê e grava na pasta sincronizada, como o Seis Sigma | acompanhamento durante o semestre, painel atualizado sozinho |

As duas formas gravam os mesmos arquivos. A diferença é só o transporte, e a proposta é ter as duas desde o primeiro modo sala de aula.

### 2.4 Campanha ou competição

- **Campanha** com atos e missões em ordem, cada uma liberando a próxima.
- **Desafios da turma**: uma missão com prazo e placar da turma por apelido, assíncrono, montado pelo painel do professor a partir das entregas. A ordenação é pela nota e depois pelo custo. Empate estatístico (intervalos que se sobrepõem) aparece como empate, para não premiar o ruído.
- **Competição entre equipes**: grupos de três ou quatro alunos numa missão grande (ato 7). Cada equipe entrega uma solução, e o painel compara as equipes por diferença pareada com os mesmos números aleatórios.

### 2.5 Recomendação

**C (híbrido) começando por A**, com campanha individual e modo sala de aula no MVP, turma offline e pelo Drive com o mesmo formato, e desafios da turma logo depois do MVP. As razões são quatro. Os laboratórios das universidades nem sempre têm placa de vídeo. O egui já é o padrão da família. A lógica do jogo fica testável. O 3D entra depois como recompensa visual, sem bloquear o conteúdo. A competição entre equipes fica para quando houver uma turma piloto (decisões E1 a E4).

---

## 3. Desenho do jogo

### 3.1 Ideia central

Cada **missão** é uma fábrica (ou um processo administrativo) com um problema, uma meta e um conjunto pequeno de **alavancas**. O aluno lê a situação, faz a previsão pedida, escolhe as decisões dentro do orçamento e manda simular. O app roda as replicações, mostra a animação 2D, os indicadores com intervalo e a nota. O aluno pode repetir quantas vezes a missão permitir.

Tipos de alavanca, todos traduzidos para chaves de `apply_patches`:

| Tipo | Exemplo na tela | Chave no modelo |
|---|---|---|
| Quantidade | "máquinas na furação: 1 a 3" | `furacao.capacity` |
| Escolha | "regra da fila: chegada, prazo, prioridade" | `fila.order` |
| Valor contínuo | "tamanho do estoque entre corte e furação" | `buffer1.capacity` |
| Calendário | "um, dois ou três turnos" | `calendars.<nome>` |
| Posição | arrastar máquinas numa grade (seção 3.3, ato 5) | posições e distâncias dos objetos |
| Atribuição | arrastar tarefas para estações (ato 4) | tempos dos `Processor` gerados pelo módulo de balanceamento |
| Método | escolher a sequência de elementos do operador (ato 6) | fluxos e tempos do operador |

Cada alavanca tem custo (dinheiro, área, pessoas) e a missão tem um orçamento. Uma missão pode também ter um **orçamento de rodadas**. Cada simulação "custa uma semana de fábrica", o que ensina a planejar o experimento em vez de tentar tudo.

### 3.2 Estrutura da campanha e degraus de autonomia

A campanha tem oito atos. Cada ato tem de quatro a seis missões e uma missão de fechamento que mistura o que veio antes. A autonomia cresce em três degraus.

1. **Alavancas** (atos 1 a 3). A fábrica está pronta e o aluno só decide.
2. **Peças** (atos 4 a 6). O aluno monta partes do problema (atribui tarefas a estações, posiciona máquinas, escolhe o método), e o app gera o modelo.
3. **Modelo** (atos 7 e 8). O aluno recebe o enunciado e os dados, monta ou completa o modelo no UTSim e entrega o `.ron`. O app corrige pelas métricas, com tolerância. Este é o exercício de modelagem que o backlog prevê.

Um **glossário vivo** acompanha a campanha. Cada conceito novo (utilização, lead time, takt, WIP) aparece no momento em que a missão o usa, com um texto curto próprio e o link para o capítulo do manual.

### 3.3 Atos e missões de exemplo

Os números abaixo são de partida e serão calibrados no construtor de casos (seção 6.2). Os modelos citados existem em `examples/`.

**Ato 1. Gargalo e fluxo**

| Missão | Situação | Alavancas | Meta e nota | Ensina |
|---|---|---|---|---|
| 1.1 A furadeira que segura a linha | três máquinas em série (`tutorial_02_linha.ron`), peça a cada 4 min, tempos médios de 3,0, 3,6 e 2,5 min | prever o gargalo. Comprar uma máquina para uma das três estações | permanência média abaixo de 20 min com uma compra. Previsão certa vale 20 pontos | utilização como razão entre tempo e intervalo, gargalo |
| 1.2 Reforçar o lugar errado | a mesma linha, com o aluno obrigado a comprar primeiro na estação errada | duas compras em ordem | comparar o ganho das duas compras pela diferença pareada | melhorar fora do gargalo não muda a vazão |
| 1.3 Quanto tem no chão | linha estável com vazão e lead time medidos | prever o estoque em processo | previsão dentro de 10 % do medido | lei de Little |
| 1.4 O gargalo que muda | linha com quebras (`manutencao.ron`) | prever o gargalo e conferir pelo gargalo móvel | escolher onde pôr a manutenção preventiva | gargalo móvel, efeito das paradas |

**Ato 2. Filas e atendimento**

| Missão | Situação | Alavancas | Meta e nota | Ensina |
|---|---|---|---|---|
| 2.1 Balcão da ferramentaria | mecânicos esperando ferramenta (`mm1.ron`, `servidores.ron`) | número de atendentes | custo total mínimo (atendente mais mecânico parado) | fila com variabilidade, troca entre custo de serviço e custo de espera |
| 2.2 Quem desiste vai embora | atendimento com sala limitada (`atendimento_com_perda.ron`) | lugares na sala, atendentes | perda abaixo de 5 % no menor custo | bloqueio e perda de demanda |
| 2.3 Urgente na frente | pedidos com prioridade (`equipe_com_prioridade.ron`) | regra da fila | prazo dos urgentes sem estourar o dos normais | disciplina de fila, prioridade |
| 2.4 Mesma média, outra fila | dois processos com a mesma média e coeficientes de variação diferentes | prever qual tem a maior fila | previsão e explicação objetiva | variabilidade gera fila |

**Ato 3. Capacidade, turnos e estoque intermediário**

| Missão | Situação | Alavancas | Meta e nota | Ensina |
|---|---|---|---|---|
| 3.1 A demanda dobrou | 1.200 peças por semana numa linha de 600 | turnos, máquinas, horas extras | atender a demanda com o menor custo semanal | capacidade, turnos (calendários) |
| 3.2 Pulmão certo | linha com bloqueio (`linha_com_bloqueio.ron`) | tamanho dos estoques entre estações, com custo de espaço | vazão de 95 % da teórica com o menor estoque | bloqueio, falta, estoque como pulmão |
| 3.3 Corretiva ou preventiva | máquina que quebra (`manutencao.ron`) | intervalo da preventiva, equipe de manutenção | disponibilidade e custo | disponibilidade, manutenção |
| 3.4 Lote e setup | duas famílias de peças com troca de ferramenta | tamanho do lote | lead time e vazão | efeito do lote e do setup |

**Ato 4. Balanceamento de linha**

| Missão | Situação | Alavancas | Meta e nota | Ensina |
|---|---|---|---|---|
| 4.1 Montagem do ventilador | 14 tarefas com precedência, demanda que dá takt de 60 s | arrastar tarefas para estações, respeitando a precedência | menor número de estações que cumpre o takt. Eficiência e atraso de balanceamento calculados | takt, precedência, eficiência |
| 4.2 O balanceamento no papel e no chão | o balanceamento da 4.1 com tempos variáveis (CV de 0,3) | estoques entre estações, rebalancear | vazão simulada contra a de projeto | variabilidade na linha balanceada |
| 4.3 Mudou o mix | dois modelos do produto na mesma linha | sequência e atribuição | vazão dos dois modelos | linha de modelos mistos |

**Ato 5. Arranjo físico e movimentação**

| Missão | Situação | Alavancas | Meta e nota | Ensina |
|---|---|---|---|---|
| 5.1 Oficina de usinagem | seis máquinas, cinco famílias com roteiros e volumes, grade de 4 por 3 | posicionar as máquinas na grade | menor distância vezes viagens (matriz de-para de `utsim-diagram`) e vazão com a empilhadeira simulada | arranjo funcional, matriz de-para |
| 5.2 Funcional ou celular | a mesma oficina | agrupar máquinas em células por família | lead time e distância contra a 5.1 | arranjo celular |
| 5.3 Giro perto da doca | armazém (`armazem.ron`, `tutorial_04_armazem.ron`) | posição dos itens por giro | tempo médio de separação | endereçamento por giro |
| 5.4 Frota de transporte | AGVs ou empilhadeiras (`agvs.ron`) | tamanho da frota e regra de chamada | entregas no prazo com o menor custo | movimentação interna |

**Ato 6. Tempos e métodos**

| Missão | Situação | Alavancas | Meta e nota | Ensina |
|---|---|---|---|---|
| 6.1 Quantas máquinas por operador | um operador e máquinas automáticas (`homem_maquina.ron`) | prever o número ideal pela relação homem-máquina e conferir na simulação | custo por peça mínimo | ciclo, interferência, gráfico homem-máquina (`utsim tempos`) |
| 6.2 Trabalho externo | o mesmo posto, com elementos que podem sair do tempo de máquina parada (`homem_maquina_melhorado.ron`) | escolher a sequência de elementos | ciclo menor | melhoria do método |
| 6.3 Célula com operadores | `celula_com_operadores.ron` | número de operadores e rota | vazão por operador | célula em U, rota do operador |
| 6.4 Cronometragem | dados de tempos com ritmo e tolerâncias | calcular o tempo padrão e o número de ciclos a medir | resposta numérica com tolerância | tempo normal, tempo padrão, amostra |

**Ato 7. Problemas integrados**

| Missão | Base | O que muda no formato |
|---|---|---|
| 7.1 Linha com inspeção e retrabalho | estudo de caso 1 | primeiro com alavancas, depois o aluno completa o modelo |
| 7.2 Pronto atendimento por turnos | estudo de caso 3 | escala de equipes com custo, chegadas por hora |
| 7.3 Centro de distribuição | estudo de caso 2 | missão de equipe, competição pareada (seção 2.4) |
| 7.4 Reembolso de despesas | `reembolso_atual.ron` e `reembolso_futuro.ron` do Lean Office | estado atual e futuro com VSM administrativo |
| 7.5 Do enunciado ao modelo | enunciado e dados novos | o aluno modela do zero no UTSim e entrega o `.ron` (degrau 3) |

**Ato 8. O caso Lean Seis Sigma** (seção 3.7).

### 3.4 Pontuação e nota

A nota de uma missão vem de quatro partes, com pesos do pacote (padrão abaixo).

1. **Portão de restrições.** Orçamento, área, número de pessoas e outros limites. Uma restrição violada deixa a missão "não resolvida", com a razão na tela.
2. **Desempenho (70 pontos).** Para cada métrica `k` da missão, o app calcula `d_k = (v_k − base_k) / (ref_k − base_k)`, limitado a 0 e 1. Aqui `v_k` é o valor do aluno, `base_k` o do estado inicial e `ref_k` o da solução de referência (seção 6.2). O desempenho é a média ponderada dos `d_k`. Uma solução melhor que a referência ganha um selo, mas não passa de 70 pontos.
3. **Previsão (20 pontos).** Escolha certa, ou valor dentro da faixa de tolerância da previsão (por exemplo 10 % do valor simulado).
4. **Justificativa (10 pontos).** Duas ou três perguntas objetivas sobre o porquê ("qual indicador mostrou o gargalo"). Opcionalmente o professor troca por um texto curto, corrigido por ele.

**Tolerâncias e estatística.** Toda métrica é medida com `n` replicações (padrão 10) e números aleatórios comuns, com a semente da missão. A meta conta como atingida quando o intervalo de 95 % fica todo do lado certo da meta. Quando o intervalo contém a meta, a missão aplica a faixa de tolerância do pacote (relativa, como ±2 %, ou absoluta). Se o valor cai na faixa, conta como atingida e a tela mostra "dentro da tolerância". O desempenho usa a média. O aluno vê o intervalo junto do número, desde a primeira missão.

**Estrelas.** Uma estrela para resolvida, duas para desempenho de pelo menos 0,8, três para pelo menos 0,95. As estrelas liberam missões, e a nota de 0 a 100 é o que vai para o professor.

**Desconto.** Cada nível de dica usada desconta pontos (padrão 0, 3 e 8 nos três níveis, seção 3.5). Rodadas acima do orçamento de rodadas descontam quando a missão tem esse orçamento. Repetir a missão é livre, e a nota registrada é a melhor ou a última, conforme o pacote (decisão E7).

**Mesmo número em toda parte.** A entrega leva as decisões, não a nota. O app do professor ressimula com a mesma semente e a mesma versão do núcleo. Se a versão do núcleo for outra, ressimula na versão do professor e aceita diferença dentro da tolerância, com aviso.

### 3.5 Dicas

Três níveis por missão, sempre em texto próprio.

1. **Pergunta que orienta** ("qual estação fica ocupada mais tempo?").
2. **Ferramenta**: o app destaca o indicador certo na tela (utilização por estação, fila média, gráfico homem-máquina).
3. **Caminho**: a direção da solução sem os números ("a compra deve ir para a estação de maior utilização").

Há também **dicas automáticas** a partir do resultado do aluno, calculadas pelas análises do núcleo. Exemplos são "a sua compra foi para uma estação com utilização de 0,62" e "o estoque entre corte e furação fica cheio 40 % do tempo". Essas dicas não descontam pontos, porque explicam o que aconteceu e não a solução.

### 3.6 Caixa de areia

Depois de resolver uma missão, o aluno abre a mesma fábrica sem meta e sem orçamento, com todas as alavancas e com mais algumas (distribuições, tempos, chegadas). Serve para explorar "e se". O resultado não conta nota. A caixa de areia tem também um modo **experimento**, com dois cenários lado a lado e a diferença pareada com intervalo. É a mesma ideia de `run_scenarios`, apresentada de forma simples. Um botão "levar para o UTSim" grava o `.ron` com as decisões aplicadas e abre o simulador, se estiver instalado.

### 3.7 Caso final Lean Seis Sigma

**Enredo.** Uma fábrica fictícia de eixos usinados atrasa entregas e tem refugo por diâmetro fora da especificação. O aluno é chamado como green belt. O modelo da fábrica é feito para o caso, com três estações, dois turnos e uma inspeção, e reaproveita a estrutura de `examples/qualidade/diametros.csv` e `grr.csv` para os dados.

**Efeitos plantados** (seção 6.3), sorteados por aluno a partir da semente.

- O instrumento de medição tem R&R alto. O aluno que não faz o MSA antes trata ruído como causa.
- A segunda máquina no turno da noite desgasta a ferramenta mais rápido, com deriva da média.
- O lote grande na estação de acabamento causa a maior parte do atraso.

**Fases.**

| Fase | No jogo | No UTSim Seis Sigma |
|---|---|---|
| Definir | escolher o CTQ e a meta entre opções e escrever o problema | termo de abertura do projeto de treino |
| Medir | planejar a coleta (cada amostra custa dias de fábrica), receber os dados simulados | MSA, linha de base, capacidade |
| Analisar | levantar hipóteses e pedir mais dados estratificados | testes de hipótese, ANOVA, cartas |
| Melhorar | testar as soluções no simulador do jogo, com orçamento | estudo de cenários (`estudo.json`) |
| Controlar | escolher a carta e o plano de reação. O jogo roda mais semanas e injeta uma deriva para ver se o aluno a detecta | plano de controle, carta de fase II |

**Correção.** O jogo confere as respostas-chave pelas crates comuns: a causa raiz identificada, o R&R calculado, o Cpk antes e depois, o ganho do cenário escolhido e a deriva detectada no prazo. Os entregáveis do Seis Sigma (termo, tollgates) são avaliados pelo professor no próprio Seis Sigma, ou o pacote dispensa essa parte (decisão E9).

**Dois caminhos.** O caso pode ser feito todo dentro do jogo, numa versão reduzida das ferramentas, para quem não tem o Seis Sigma. Ou com a análise no Seis Sigma, que é a proposta. Assim o aluno usa a ferramenta de verdade e o jogo não duplica a estatística.

### 3.8 Narrativa, tom e idioma

Uma empresa fictícia, com setores que o aluno visita ao longo dos atos (usinagem, montagem, expedição, escritório). Os personagens têm só um papel ("a supervisora da montagem") e falam pouco. O texto é direto, em português do Brasil, com os termos da engenharia de produção. Todos os textos ficam num catálogo, como no Seis Sigma, para a tradução futura. Há contraste alto, tamanho de letra ajustável e nenhuma informação dada só por cor.

---

## 4. Recursos do professor

### 4.1 Pacotes de exercícios

Um pacote (`.utse`, zip) reúne missões, modelos, dados, textos e figuras, com um manifesto `pacote.ron` (seção 6.1). O professor pode fazer quatro coisas.

- Usar os pacotes da campanha como estão.
- Montar um pacote da disciplina com missões escolhidas, ordem, prazos e pesos.
- Criar missões novas no construtor de casos (seção 6.2).
- Importar pacotes de outras frentes (linhas, qualidade e segurança).

Pacotes oficiais são assinados com a chave do autor (ed25519, a mesma infraestrutura do launcher). Pacotes de professor podem ser assinados com a chave do professor, gerada no app. O app mostra a origem do pacote.

### 4.2 Correção automática

- **Missões de alavanca e de peças**: a entrega traz as decisões e o diário das tentativas. O app do professor ressimula e calcula a nota (seção 3.4).
- **Missões de modelo** (degrau 3): o aluno entrega o `.ron`. O app confere se o modelo carrega (`utsim check`), roda as replicações e compara as métricas pedidas com as do modelo de referência, com tolerância. Ele também confere a estrutura mínima declarada no pacote (por exemplo "tem uma inspeção com retrabalho"). Um relatório por aluno diz o que bateu e o que não bateu.
- **Questões numéricas e de escolha**: corrigidas pela receita, como no Seis Sigma.

A correção roda num fio de trabalho, com a fila de entregas e a barra de progresso. Uma turma de 60 alunos com 10 replicações por missão pequena deve corrigir em poucos minutos num notebook comum. A meta será medida no passo EN4.

### 4.3 Turmas

Uma `Turma` tem nome, período, lista de alunos (nome, registro acadêmico, apelido para o placar), pacotes publicados com prazos e o modo (offline ou Drive). A lista de alunos é importada por CSV. O aluno entra na turma abrindo o arquivo de convite (`.utsc`, com o identificador da turma e a chave pública do professor) ou a pasta da turma.

```
Turma Arranjo Físico 2027-1/          (pasta do professor no Drive, ou local)
  turma.ron                     manifesto: id, nome, período, chave pública do professor
  pacotes/                      pacotes publicados (alunos só leem)
  alunos.csv                    lista (só professor)
  entregas/<registro>/          uma pasta por aluno, compartilhada com o aluno e o professor
    <missao>/<tentativa>.json   decisões, previsão, respostas, diário assinado
  correcao/                     notas e relatórios (só professor)
  provas/                       pacotes de prova (sem gabarito)
```

### 4.4 Painel do professor

- **Matriz aluno por missão**, com nota, estrelas, tentativas, dicas usadas e tempo, mais o filtro por pacote e prazo.
- **Erros mais comuns por missão**: as decisões mais frequentes entre os que não resolveram (por exemplo "52 % compraram na estação de corte") e as previsões mais erradas. Volta para o planejamento da aula.
- **Conceitos com dificuldade**: cada missão é ligada a conceitos do glossário, e o painel agrega a nota por conceito.
- **Desafio da turma**: placar por apelido e empates estatísticos (seção 2.4).
- **Exportação** das notas em CSV e `.xlsx` (o mesmo código de `utsim_lss_store::planilha`) e de um relatório da turma em PDF.

### 4.5 Modo prova

O desenho é o das provas do Seis Sigma (`formacao/exams.rs`), estendido às missões.

- A prova é montada por sorteio com semente registrada, entre missões e questões do banco do professor, por conceito e dificuldade.
- Os dados de cada aluno variam pela semente (seção 4.6), e o pacote de prova não leva solução de referência, dicas nem gabarito.
- Há janela de horário, tempo máximo, uma tentativa por missão (ou o número que o professor definir) e a caixa de areia bloqueada.
- A correção acontece só na máquina do professor, que reconstrói a prova pela semente.
- A prova também pode ser em papel, com a fábrica projetada e as respostas lançadas pelo professor.

### 4.6 Anti-cola básico

Sem servidor, nada impede consulta. O manual diz isso com clareza. O que o app faz é tornar a cópia pouco útil e visível.

- **Variante por aluno.** Os parâmetros da missão variam dentro de uma faixa pela semente do aluno (por exemplo, o intervalo entre chegadas de 3,8 a 4,2 min e a demanda de 1.150 a 1.250). A decisão boa de um colega raramente é a melhor para outro, e as respostas numéricas são diferentes.
- **A nota não viaja.** A entrega leva as decisões e o diário, e o professor ressimula. Editar o arquivo para mudar a nota não tem efeito.
- **Diário assinado.** Cada tentativa é assinada com a chave do aluno, criada no primeiro uso, e encadeada por hash à anterior, com data e hora. Uma entrega sem as tentativas que a levaram até ali, ou com a cadeia quebrada, é marcada.
- **Semelhança.** O painel aponta pares de alunos com a mesma sequência de decisões em variantes diferentes, ou com tempos de tentativa coincidentes. A decisão é sempre do professor.
- **Limites ditos.** Quem tem acesso ao arquivo pode tentar forjá-lo. A assinatura revela a alteração, mas não a impede. O mesmo vale para a pasta do Drive.

### 4.7 Dados pessoais

Nome, registro acadêmico e notas são dados pessoais sob a LGPD. O app registra a finalidade na criação da turma e mostra o apelido no placar, nunca o nome. A pasta de cada aluno fica compartilhada só com ele e o professor. A retenção é configurável, com uma operação de anonimização no fim do período (a mesma ideia de `utsim-lss-core::privacidade`).

---

## 5. Arquitetura

### 5.1 Crates novas

| Crate | Papel | Depende de |
|---|---|---|
| `utsim-ensino-core` | pura e testada. Missão, alavanca, tradução para chaves de `patch`, execução das rodadas, nota, tolerância, dicas automáticas, progresso, turma, prova, variante por semente, balanceamento e grade de arranjo (decisão E11) | `utsim-model`, `utsim-stats`, `utsim-opt`, `utsim-diagram`, `utsim-quality`, `utsim-linhas` |
| `utsim-ensino-store` | pacotes `.utse`, entregas `.utsr`, convites `.utsc`, pasta da turma, diário assinado, cache local | `utsim-ensino-core`, o crate comum `utsim-diario` (decisão E12) |
| `utsim-ensino-ui` | telas egui sem backend: mapa da campanha, missão, vista 2D, resultados, painel do professor | `egui =0.36.2`, `egui_plot =0.37.0`, `utsim-ui-tema` |
| `utsim-ensino-app` | binário `utsim-ensino` com eframe (glow), fios de trabalho, janela de arquivos | as três acima |

As crates com janela ficam fora dos `default-members`, como o launcher e o Seis Sigma. A `utsim-ensino-core` entra nos `default-members` e na CI.

### 5.2 Execução de uma missão

1. O pacote fornece o modelo base `.ron` e a lista de alavancas com as chaves.
2. As decisões do aluno viram um `BTreeMap<String, String>` de chaves e valores, e `apply_patches` gera o modelo da tentativa. Alavancas de peças (balanceamento, grade) passam antes por um compilador do `utsim-ensino-core`, que gera os objetos e as chaves.
3. A variante do aluno é outro conjunto de chaves, aplicado antes das decisões e sorteado da semente `hash(missao, aluno)`.
4. `run_scenarios` roda o estado inicial, a tentativa e (só na máquina do professor ou na calibração) a referência, com números aleatórios comuns, num fio de trabalho.
5. As métricas vêm de `analysis` (utilização, fila, permanência, vazão, gargalo móvel) e dos indicadores da linha, quando a missão é de uma linha.
6. Uma replicação é gravada como `Replay` para a animação.
7. O `utsim-ensino-core` calcula a nota, as dicas automáticas e a entrada do diário.

Missões com tempo de rodada grande (ato 7) mostram a barra de progresso e permitem cancelar. A meta é responder em menos de 3 s nas missões dos atos 1 a 3 num computador de laboratório.

### 5.3 Vista 2D

O desenho é esquemático, com caixas para estações, filas como pilhas de itens e setas do fluxo. A posição vem do `layout.rs` de `utsim-diagram`, ou das posições do modelo quando existem (ato 5). A animação lê o rastro do `Replay` e mostra itens andando, filas crescendo e estações coloridas pelo estado (trabalhando, ociosa, bloqueada, quebrada, em setup). Há controle de velocidade e pausa. É desenhada com o pintor do egui, sem dependência nova. A mesma vista serve para a caixa de areia e para o painel do professor.

### 5.4 Modo 3D (híbrido)

- **Primeiro passo**: o botão "ver em 3D" grava o `.ron` da tentativa e o `Replay` e abre o cliente `utsim-3d` (pelo `--abrir` do launcher), se estiver instalado.
- **Passo seguinte** (decisão E1): a ponte `utsim-godot` hospeda a `utsim-ensino-ui` num modo "missão" do cliente 3D, com a mesma lógica do `utsim-ensino-core`. A crate de telas não pode depender de eframe nem de fios próprios. Ela recebe um executor de tarefas do hospedeiro.

### 5.5 Versões e reprodutibilidade

Cada entrega guarda a versão do núcleo, o hash do pacote e o hash do modelo. Um teste de CI roda todas as missões embutidas com a solução de referência e confere a nota e as métricas guardadas (golden), para que uma mudança no núcleo não quebre o gabarito em silêncio. Mudança intencional regenera o golden com revisão.

---

## 6. Pipeline de conteúdo

### 6.1 Formato da missão

```ron
Missao(
    id: "ato1/furadeira",
    formato: 1,
    titulo: "A furadeira que segura a linha",
    conceitos: ["gargalo", "utilizacao"],
    modelo: "modelos/linha3.ron",
    texto: "textos/furadeira.md",
    variante: [ Faixa(chave: "fonte.interarrival.mean", de: 3.8, ate: 4.2) ],
    previsao: [ Escolha(pergunta: "Qual estação é o gargalo?", opcoes_de: Objetos(["corte", "furacao", "pintura"])) ],
    alavancas: [
        Quantidade(rotulo: "Máquinas na furação", chave: "furacao.capacity", de: 1, ate: 3, custo_unitario: 80000),
        Quantidade(rotulo: "Máquinas no corte", chave: "corte.capacity", de: 1, ate: 3, custo_unitario: 60000),
    ],
    orcamento: 80000,
    metricas: [ Metrica(nome: "permanencia_media", objetivo: Menor, meta: 20.0, peso: 1.0, tolerancia: Relativa(0.02)) ],
    rodadas: (replicacoes: 10, duracao: "run.duration"),
    dicas: ["dicas/furadeira.md"],
    referencia: Calcular(otimizador: Exaustivo),
)
```

O leitor valida tudo ao carregar: as chaves existem no modelo, as faixas são possíveis e o orçamento permite pelo menos uma solução. O formato é versionado com migração, como o `.ron` do modelo (`docs/versionamento.md`).

### 6.2 Construtor de casos a partir de modelos UTSim

Fica no app (modo professor) e num comando `utsim ensino caso` para o autor.

1. **Escolher o modelo** `.ron` (de `examples/`, de um estudo de caso, de uma linha ou do professor).
2. **Marcar as alavancas**: o construtor lista os campos que `patch` aceita, por objeto, e o professor escolhe faixas e custos.
3. **Escolher as métricas** entre as que o modelo produz, com meta e tolerância.
4. **Calcular a referência** com `utsim-opt` (exaustivo para espaços pequenos e a busca do otimizador para os grandes). O construtor mostra a base, a referência e a distância entre elas.
5. **Conferir a missão**: se a referência e a base empatam estatisticamente, a missão não ensina nada e o construtor avisa. Ele avisa também se mais de 30 % das combinações batem a meta (missão fácil demais) ou se nenhuma bate.
6. **Gerar o texto inicial** do enunciado a partir de um modelo de texto próprio, que o professor reescreve.
7. **Exportar** a missão para um pacote.

### 6.3 Dados com efeitos plantados

Geradores com semente, em `utsim-ensino-core`, sobre `utsim-stats` e `utsim-model`, que retomam o antigo L8.

- **Tempos de processo** com distribuição escolhida, para as missões de ajuste e de cronometragem.
- **Estudo de MSA** com componentes de variância conhecidos (peça, operador, repetição).
- **Média deslocada** por turno, operador ou máquina, com o tamanho do efeito em desvios padrão.
- **Série com padrão** para cada regra de Nelson (tendência, deriva, mistura, estratificação).
- **Fatorial** com efeitos principais e interação definidos.
- **Dados vindos da simulação**: o gerador roda o modelo com o efeito plantado (por exemplo, mais variabilidade numa estação ou uma quebra mais frequente num turno) e exporta os registros como CSV de coleta.

Cada conjunto guarda a receita (gerador, parâmetros, semente) e a resposta conhecida, que a correção usa. Os dados são próprios do UTSim.

### 6.4 Conferência na CI

- Toda missão embutida carrega, valida e tem a referência recalculada e comparada com a guardada.
- Todo gerador de dados recupera o efeito plantado dentro da tolerância esperada (o teste de recuperação usa as mesmas funções que o aluno usará).
- Os textos passam pela varredura de termos da linha, quando a missão é de uma linha (como `crates/utsim-linhas/tests/manual_da_linha.rs`), e pela conferência de links.

### 6.5 Fontes do conteúdo inicial

Os tutoriais 01 a 06 cobrem os atos 1 a 3. Os modelos de `examples/` cobrem os atos 2, 3, 5 e 6. Os três estudos de caso cobrem o ato 7. Os cinco modelos prontos do Lean Office cobrem as missões administrativas. Faltam modelos próprios para o balanceamento (ato 4), para a grade de arranjo (ato 5) e para o caso final (ato 8).

### 6.6 Pacotes de conteúdo de outras frentes

O formato de pacote é o ponto de encaixe. Um pacote declara o que precisa (`requer: ["nucleo >= 0.3", "linha:lean_office"]`), e o app avisa quando falta alguma coisa.

- **Linhas especializadas**: a missão de uma linha abre com o vocabulário e os indicadores dela (`line` no modelo, `utsim-linhas`).
- **Qualidade**: missões de inspeção e amostragem, custo da qualidade, refugo e retrabalho no modelo, CEP sobre dados simulados.
- **Segurança do trabalho**: missões de risco por posto, zonas de risco no arranjo físico (pessoas e empilhadeiras na mesma área), ergonomia ligada ao estudo de tempos. A métrica pode ser de exposição (tempo de pessoas dentro de uma zona de risco) quando o núcleo a oferecer.
- **Professor**: pacotes próprios, com a mesma assinatura.

---

## 7. Integrações

### 7.1 UTSim Seis Sigma

- **Projeto de treino.** O ato 8 exporta um projeto `.lss` (ou `.lssz`) marcado como treino, com o modelo ligado por hash, os conjuntos de dados com origem `Simulado` e o termo pré-preenchido. O Seis Sigma já separa o espaço de treino do programa real (LSS-RNF-15).
- **Volta das respostas.** As respostas-chave do aluno (causa raiz, R&R, Cpk, ganho) são digitadas no jogo ou lidas do `estudo.json` do Seis Sigma. O jogo corrige sem depender da tela do outro app.
- **Competências.** Em programa de empresa, as missões concluídas gravam entradas `competencia` de origem `exercicio` no diário do programa (`mentoria::record_level`), o ponto de ligação deixado pela decisão N2. Os conceitos das missões são ligados às ferramentas do currículo do Seis Sigma por uma tabela no pacote.
- **Sem o Seis Sigma**, o caso final roda na versão reduzida dentro do jogo (seção 3.7).

### 7.2 Linhas especializadas

As missões usam os modelos prontos e a biblioteca da linha. Uma missão do Lean Office mostra "caso", "posto de trabalho" e os indicadores da linha (lead time, touch time, %C&A), sem termo geral do núcleo. Linhas futuras (serviços, logística, saúde) trazem os próprios pacotes.

### 7.3 Frente de qualidade e segurança do trabalho

Essa frente está em planejamento paralelo. Este plano só fixa o contrato. A frente entrega pacotes `.utse` e, se precisar, métricas novas no núcleo (exposição, refugo por causa). O app de ensino as mostra sem código próprio, desde que venham como métricas nomeadas do relatório. Se a frente virar app próprio, o caminho é o mesmo do Seis Sigma, com exportação de projeto e volta das respostas.

### 7.4 Simulador e cliente 3D

"Levar para o UTSim" grava o `.ron` da tentativa e abre o editor. "Ver em 3D" abre o `Replay` no cliente Godot. Os dois só aparecem quando o produto está instalado pelo launcher, como no Seis Sigma.

### 7.6 UTSim Estatística e UTSim Qualidade

O pacote "Estatística e qualidade na fábrica" (`ensino/pacotes/estatistica_qualidade`, feito em 2026-11-09) liga o jogo aos dois apps. Ele traz seis missões de análise:
- replicações e intervalo de confiança;
- comparação pareada com números aleatórios comuns;
- carta X̄-R com deriva plantada;
- Cpk antes e depois;
- R&R com a fonte dominante;
- estratificação.

Os geradores `Pareado` e `Subgrupos` e as questões `MeiaLargura`, `Replicacoes`, `DiferencaMedia`, `DiferencaSignificativa`, `PrimeiroSubgrupoFora`, `Cpk` e `FonteDominante` entram no núcleo. A missão diz em `analisar_em` para qual app os dados vão. O botão grava os `.csv` no perfil e abre o app com `--utsim-abrir=<arquivo.csv>`, um por conjunto, na convenção da família. A leitura do `.csv` como conjunto de dados fica com os dois apps. Até lá, o app só abre, e os arquivos ficam no perfil para a importação manual.

### 7.5 Placas com microcontrolador

O protocolo próprio para placas (ESP32, Arduino) ainda será conversado com o dono (backlog). Quando existir, o app de ensino pode usá-lo de duas formas.

- **Bancada didática**: uma mini linha montada em laboratório (esteira, sensores de passagem) envia os tempos de cada estação. O aluno usa esses tempos reais como dados da missão de cronometragem ou de ajuste, e o professor compara com o modelo.
- **Missão com dado ao vivo**: a mesma bancada alimenta o gêmeo (`utsim-twin`), e o aluno detecta a deriva numa carta de controle.

O app só lê os registros por meio do formato de dados que o protocolo definir (CSV ou o fluxo do gêmeo). Nada do jogo depende das placas.

---

## 8. Distribuição pelo launcher

- **Produto** `ensino`, já previsto no catálogo do launcher com nome, subtítulo e ícone. Falta publicar o pacote com `utsim-publicar publicar --produto ensino`.
- **Canais**: `estavel` com **acesso aberto** (seção 13 do plano do launcher) para os alunos, que instalam sem código por pessoa. Um canal `testes` com código para o professor e os monitores receberem versões novas antes.
- **Um só binário** para aluno e professor. O modo professor é ligado por um perfil local com a chave do professor. Os segredos (soluções de referência das provas, notas) moram na pasta do professor, nunca no binário (decisão E13).
- **Pacotes de conteúdo** embutidos (campanha) e pacotes extras distribuídos pelo launcher como produto de conteúdo, ou pela pasta da turma. Assim o professor atualiza o conteúdo sem esperar uma versão nova do app (decisão E14).
- **Laboratório**: instalação por usuário, sem administrador, como o launcher. A instrução do manual cobre os computadores compartilhados (perfil do aluno numa pasta escolhida, por exemplo um pen drive).
- **Script** `scripts/publicar-ensino.sh`, no padrão do `publicar-seis-sigma.sh`.

---

## 9. Roteiro em fases

Horas Opus 5.5 por fase. Opus fica com o formato, a estatística da nota, a calibração das missões e as regras de concorrência. Sonnet fica com as telas, o conteúdo a partir de plano e os testes. Haiku fica com as tarefas mecânicas.

### 9.1 MVP proposto

O aluno instala o UTSim Ensino pelo launcher, joga os atos 1 a 3 (cerca de 14 missões) com previsão, alavancas, vista 2D animada, nota com intervalo e dicas, e usa a caixa de areia. O professor cria uma turma (offline e pelo Drive), publica o pacote, recebe as entregas, ressimula, vê a matriz da turma e exporta as notas. Ficam fora do MVP o balanceamento, a grade de arranjo, tempos e métodos, o caso final, o modo prova, as competições e o 3D.

### 9.2 Fases

| Fase | Conteúdo | Horas | Modelo | Critério de aceitação |
|---|---|---|---|---|
| **EN0 Fundação** (MVP) | crates `utsim-ensino-core`, `-store`, `-ui`, `-app`. Formato `missao.ron` e `pacote.ron` com validação e versão. Janela eframe com o tema da família. Catálogo de textos. Entrada no launcher | 18–26 | Opus 7–10 (formatos, versionamento); Sonnet 10–14 (janela, navegação); Haiku 1–2 (CI, catálogo) | o app abre pelo launcher. Um pacote de exemplo carrega, e um pacote com chave inexistente é recusado com a razão |
| **EN1 Motor de missão e nota** (MVP) | alavancas para chaves de `patch`, variante por semente, rodadas com números aleatórios comuns num fio, métricas, nota com portão, desempenho, previsão e justificativa, tolerância com intervalo, dicas automáticas, diário assinado das tentativas | 26–36 | Opus 18–24 (nota, tolerância, variante, assinatura); Sonnet 8–12 (testes) | a mesma missão, semente e decisões dão a mesma nota em duas máquinas. Testes da nota nos limites da tolerância. Missão do ato 1 responde em menos de 3 s |
| **EN2 Telas do jogo** (MVP) | mapa da campanha, tela da missão, previsão, alavancas com orçamento, vista 2D animada pelo `Replay`, resultados com intervalo, dicas, caixa de areia com dois cenários | 26–36 | Sonnet 22–30; Opus 4–6 (vista 2D a partir do rastro) | aluno de teste resolve a missão 1.1 sem ajuda externa. A animação mostra fila e estados coerentes com o relatório |
| **EN3 Conteúdo dos atos 1 a 3** (MVP) | cerca de 14 missões com textos, dicas, referências calculadas e golden na CI. Glossário vivo | 24–34 | Opus 8–12 (calibração, conferência das missões); Sonnet 16–22 (textos, modelos) | todas as missões passam no conferidor (seção 6.2). Golden de referência na CI. Revisão do dono dos textos |
| **EN4 Sala de aula básica** (MVP) | turma, convite, importação de alunos por CSV, pasta no Drive e entrega offline, correção em lote no professor, matriz aluno por missão, erros comuns, exportação CSV e `.xlsx` | 28–38 | Opus 8–11 (pasta da turma, cadeia de entregas, correção em lote); Sonnet 20–27 (telas, painel) | turma simulada de 60 alunos com entregas geradas é corrigida em menos de 5 min num notebook. Entrega com nota editada à mão não muda a nota |
| *Subtotal do MVP* | | *122–170* | *Opus ≈ 45–63; Sonnet ≈ 76–105; Haiku ≈ 1–2* | piloto numa turma do dono |
| **EN5 Balanceamento e arranjo** | módulo de balanceamento (tarefas, precedência, takt, eficiência, compilação para estações). Grade de arranjo com distâncias e matriz de-para. Telas de arrastar. Atos 4 e 5 | 30–42 | Opus 12–16 (balanceamento, grade para modelo); Sonnet 18–26 | toda atribuição que viola a precedência é recusada com a razão. A distância da grade bate com a matriz de-para do `utsim-diagram` |
| **EN6 Tempos e métodos e problemas integrados** | ato 6 sobre `utsim tempos` e ato 7 a partir dos estudos de caso e do Lean Office, missão de modelo (degrau 3) com correção do `.ron` | 28–38 | Opus 8–12 (correção de modelos por métricas e estrutura); Sonnet 20–26 | os modelos de gabarito dos estudos de caso tiram nota máxima. Um modelo com erro plantado é reprovado no critério certo |
| **EN7 Construtor de casos e dados plantados** | construtor no modo professor e `utsim ensino caso`, referência por `utsim-opt`, conferência da missão, geradores de dados com efeito plantado e teste de recuperação | 28–38 | Opus 14–18 (geradores, conferidor); Sonnet 14–20 (telas, comando) | um professor cria uma missão nova a partir de um `.ron` de `examples/` em menos de 30 min. Todo gerador recupera o efeito na CI |
| **EN8 Caso final e Seis Sigma** | ato 8 com enredo, efeitos plantados por aluno, versão reduzida das ferramentas, exportação do projeto de treino, volta das respostas, competências no diário do programa | 32–44 | Opus 14–18 (caso, correção das respostas-chave, ligação com o Seis Sigma); Sonnet 18–26 | o caso é resolvido de ponta a ponta no jogo e com o Seis Sigma. A causa plantada é recuperável pelas ferramentas dos dois caminhos |
| **EN9 Modo prova e anti-cola** | sorteio com semente, pacote sem gabarito, janela e tempo, correção só no professor, indicador de semelhança | 16–22 | Opus 7–9 (sorteio, reconstrução, semelhança); Sonnet 9–13 | mesma semente, mesma prova. O pacote de prova não contém referência (conferido por teste). Pares copiados em teste sintético são apontados |
| **EN10 Desafios e equipes** | placar por apelido com empate estatístico, missão de equipe com comparação pareada | 10–14 | Opus 3–4; Sonnet 7–10 | duas soluções estatisticamente iguais aparecem empatadas |
| **EN11 3D** | "ver em 3D" pelo cliente Godot e, depois, `utsim-ensino-ui` hospedada na ponte | 18–28 | Opus 5–8 (executor de tarefas comum); Sonnet 13–20 | a mesma missão roda nos dois hospedeiros com a mesma nota |
| **EN12 Pacotes de outras frentes e placas** | `requer` nos pacotes, primeiro pacote da frente de qualidade e segurança, leitura dos dados da bancada quando o protocolo existir | 10–16 | Sonnet 8–12; Opus 2–4 | pacote com requisito ausente avisa e não quebra. Missão de cronometragem roda com dados de bancada gravados |
| **EN13 Idiomas** | extração dos textos, inglês e espanhol | 6–10 | Haiku 2–3; Sonnet 4–7 | nenhum texto fora do catálogo (varredura automática) |
| *Total depois do MVP* | | *178–252* | *Opus ≈ 65–89; Sonnet ≈ 111–160; Haiku ≈ 2–3* | |
| **Total geral** | | **300–422** | | |

### 9.3 Ordem sugerida

1. EN0 e EN1 juntos. O motor pode ser testado pela linha de comando antes da janela existir.
2. EN2 e EN3 em paralelo, com o dono revisando os textos do ato 1 cedo.
3. EN4 e piloto numa turma do dono, com os atos 1 a 3.
4. EN5 e EN7 em seguida, porque o construtor de casos acelera todo o conteúdo seguinte.
5. EN6 e EN8, que fecham a trilha pedida pelo dono.
6. EN9 antes de usar o app em avaliação valendo nota.
7. EN10 a EN13 conforme o uso e as outras frentes.

### 9.4 Testes

| O quê | Como |
|---|---|
| Nota | casos à mão nos limites (intervalo encostado na meta, dentro e fora da tolerância), portão com cada restrição, desempenho limitado a 0 e 1 |
| Reprodutibilidade | mesma entrega ressimulada em processos diferentes dá resultado idêntico byte a byte |
| Missões embutidas | golden da referência e das métricas na CI. Conferidor de missão em todas |
| Variante | a variante de um aluno é a mesma em toda execução. Variantes diferentes mudam a resposta numérica |
| Entregas | cadeia de hash quebrada detectada, nota editada sem efeito, entregas de 60 alunos corrigidas em lote |
| Balanceamento | precedência respeitada, eficiência e atraso de balanceamento à mão em casos pequenos |
| Geradores | recuperação do efeito plantado pelas crates comuns |
| Prova | pacote sem referência, sorteio reprodutível |
| Telas | teste de fumaça da janela sem exibição, como nos outros apps egui |

---

## 10. Decisões do dono

**Aceitas pelo dono em 2026-11-06.** Todas as propostas desta seção valem como decididas (E1 a E16). Em E8 fica a ordem proposta dos oito atos. O MVP (EN0 a EN4) começa numa sessão própria.

Propostas, a aceitar ou mudar.

1. **E1 Formato.** Proposta: híbrido (C), começando pelo app egui próprio (A) com vista 2D, e o 3D pelo cliente Godot depois (EN11). Alternativas: só A, ou o jogo dentro do cliente Godot (B).
2. **E2 Sala de aula no MVP.** Proposta: sim, com turma, entregas, correção e painel (EN4). Alternativa: MVP só com a campanha individual e a sala de aula logo depois.
3. **E3 Transporte da turma.** Proposta: pasta no Drive e arquivo de entrega offline, com o mesmo formato, as duas no MVP. Alternativa: só o arquivo offline no MVP.
4. **E4 Competição.** Proposta: campanha individual primeiro, desafios da turma por apelido em EN10, equipes no ato 7. Alternativa: sem competição.
5. **E5 Nome.** Proposta: manter "UTSim Ensino" (produto `ensino` já no launcher). O nome do jogo dentro do app fica para o dono escolher.
6. **E6 Pesos da nota.** Proposta: desempenho 70, previsão 20, justificativa 10, desconto por dica de 0, 3 e 8, configuráveis por pacote.
7. **E7 Tentativas.** Proposta: tentativas livres na campanha com a melhor nota registrada, e o professor escolhe "melhor" ou "última" na turma.
8. **E8 Atos e ordem.** Confirmar os oito atos da seção 3.3 e a ordem (gargalo, filas, capacidade, balanceamento, arranjo, tempos e métodos, integrados, Lean Seis Sigma). Alternativa: tempos e métodos antes do balanceamento.
9. **E9 Caso final.** Proposta: análise no UTSim Seis Sigma com volta das respostas-chave ao jogo, mais uma versão reduzida dentro do jogo para quem não tem o Seis Sigma. Os tollgates são avaliados pelo professor no Seis Sigma, se ele quiser.
10. **E10 Missões de modelo.** Proposta: o degrau 3 (o aluno entrega o `.ron`) entra no ato 7 e no modo sala de aula. Alternativa: deixar a modelagem só para a disciplina de simulação, fora da campanha.
11. **E11 Lugar do balanceamento.** Proposta: módulo em `utsim-ensino-core` agora, movido para uma crate comum quando o Seis Sigma fizer o balanceamento da P6. Alternativa: crate `utsim-metodos` desde já, usada pelos dois apps.
12. **E12 Diário das entregas.** (**decidida** em 2026-11-09, de acordo com a sessão Seis Sigma) Crate comum `utsim-diario`, criado pelo Ensino e usado pelo Seis Sigma na P7. Ele só tem primitivas: chave ed25519 com a pública em hex, `sha256_hex`, `Elo {corpo, hash, assinatura}` com o corpo guardado como texto, cadeia por autor dentro do corpo (trait `Encadeado`: autor, sequência e hash do anterior), `conferir` e `conferir_desde` (conferência incremental a partir do último elo conferido) e `Selo` para assinatura avulsa, com várias assinaturas possíveis sobre o mesmo corpo. A conferência devolve problemas sem texto (`Problema`), e cada app escreve a mensagem no seu catálogo. Um buraco no meio da cadeia é apontado, e um final faltando não é, porque a pasta sincroniza com atraso. Dois elos do mesmo autor com a mesma sequência e hashes diferentes são uma bifurcação (backup restaurado). `Selo.publica` é sempre a chave de quem assinou, e `quando` vem de quem chama, nunca do relógio do crate. Ficam no Seis Sigma o formato do `Op` com `sequencia` e `anterior`, o chaveiro pessoa→chave, a troca de chave e os textos. Os arquivos já gravados pelo Ensino continuam válidos.
13. **E13 Binário do professor.** Proposta: um só binário com modo professor ligado por perfil local. Alternativa: produto `ensino-professor` separado, em canal com código.
14. **E14 Conteúdo pelo launcher.** Proposta: campanha embutida no app e pacotes extras como produto de conteúdo do launcher ou pela pasta da turma. Alternativa: só embutido.
15. **E15 Público da segunda leva.** Proposta: formação de belts na empresa piloto do Seis Sigma, com registro de competência. Alternativas: pós-graduação ou curso técnico.
16. **E16 Prova valendo nota.** Proposta: o app oferece o modo prova com as limitações ditas no manual, e o professor decide se vale nota. Alternativa: prova só em papel com a fábrica projetada.

---

## 11. Riscos

- **Conteúdo é o gargalo.** O motor sai antes das missões. Sem o construtor de casos (EN7), cada missão custa horas de calibração. A mitigação é começar com missões dos tutoriais e dos exemplos e trazer o construtor logo depois do MVP.
- **Jogo vira tentativa e erro.** Com rodadas grátis, o aluno pode varrer as alavancas sem pensar. A previsão com nota, o orçamento de rodadas em parte das missões e as perguntas de justificativa reduzem isso. O painel mostra quem resolve com muitas rodadas.
- **Ruído na nota.** Com poucas replicações, dois alunos com a mesma decisão poderiam ter notas diferentes. A nota usa a semente da missão e números aleatórios comuns, e a tolerância trata o empate estatístico.
- **Gabarito que muda com o núcleo.** Uma correção no motor pode mudar as métricas. O golden das missões na CI pega isso antes da publicação, e a entrega guarda a versão do núcleo.
- **Computadores de laboratório.** Máquinas antigas e sem placa de vídeo. A proposta é o app egui com glow e a vista 2D, com o 3D opcional.
- **Sincronização do Drive.** Atraso e cópias em conflito. As entregas são arquivos só de acréscimo, um por tentativa, e o arquivo offline cobre quem não usa o Drive.
- **Cola sem servidor.** Não há como impedir. A variante por aluno, a ressimulação e o diário assinado a tornam pouco útil e visível. O manual diz o limite.
- **Dados pessoais.** Notas e registro acadêmico estão sob a LGPD. A proteção real é o compartilhamento das pastas, e o app registra a finalidade e anonimiza no fim.
- **Duplicação com o Seis Sigma.** A versão reduzida das ferramentas no caso final pode divergir da ferramenta real. Ela chama as mesmas crates (`utsim-stats`, `utsim-quality`) e só a tela é diferente.
- **Escopo das frentes paralelas.** A frente de qualidade e segurança e o protocolo das placas ainda não têm plano. O app só depende do formato de pacote e de métricas nomeadas, para não esperar por elas.
- **Material protegido.** Enunciados, histórias, dados e tabelas são escritos para o UTSim. O conferidor de textos e a revisão do dono antes de cada pacote oficial evitam trechos copiados.
