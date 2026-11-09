# Plano de integração da família UTSim

Proposta para os apps da família conversarem entre si, sem servidor. Junta as duas direções do dono:

- **2026-11-07:** o UTSim Qualidade é onde as ferramentas da qualidade são feitas, para todos usarem por ali. Todos os softwares devem conversar entre si.
- **2026-11-08:** os dados de qualidade, segurança e manutenção de cada equipamento ficam numa ou mais abas do painel de propriedades do nó, no UTSim. Os apps dedicados mostram os resultados e dão acesso detalhado aos fluxos, onde também se edita. É mão dupla: o mesmo dado pode ser editado em qualquer app, e os números vêm da simulação.

## 1. Regras

1. **Uma fonte por dado.** Dado do nó (defeitos, inspeção, retrabalho, zonas, EPI, perigos, falhas e preventivas) mora no nó do modelo `.ron`. Dado da organização (pessoas, incidentes, planos, 8D, inventário completo) mora na pasta de unidade. Projeto DMAIC mora na pasta `.lss`. Nenhum app guarda cópia de dado que mora em outro lugar.
2. **Ferramenta em um lugar.** As telas das ferramentas da qualidade (Pareto, Ishikawa, FMEA, MSA, capacidade, CEP, 8D, amostragem e as demais) existem só no UTSim Qualidade, sobre as crates comuns (`utsim-ferramentas`, `utsim-quality`, `utsim-stats`). Os outros apps abrem a ferramenta lá e recebem o resultado de volta.
3. **Origem sempre visível.** Todo dado trocado diz de onde veio (Real, Gêmeo, Simulado, Manual), o app, o arquivo e o hash.
4. **Cada recurso novo diz o que manda e o que recebe** dos outros apps, numa linha da seção 3.

## 2. Mecanismo comum para um app abrir outro

Sem servidor e sem rede, só arquivos e linha de comando. Fica numa crate pequena, `utsim-troca`, usada por todos os apps.

### 2.1 Achar o app

O app chamador procura o produto pela instalação do launcher (`utsim_launcher_core::install::launch_command`, que o Seis Sigma já usa para abrir o UTSim). Na máquina de desenvolvimento, sem launcher, o executável é procurado ao lado do próprio executável e depois no `PATH`. Se não achar, o app diz qual produto falta e oferece abrir o launcher (`utsim-launcher --abrir <produto>`).

### 2.2 Pedido por arquivo

O chamador grava um **pedido** (`.utsim-pedido.json`) numa pasta de troca e abre o outro app com um argumento só:

```text
utsim-qualidade --utsim-pedido <caminho do pedido>
```

| Campo | O que é |
|---|---|
| `formato` | versão do formato do pedido |
| `id` | identificador único do pedido |
| `de` | produto e versão de quem chama (`lean-seis-sigma 0.9.0`) |
| `acao` | o que fazer: `abrir_ferramenta`, `editar_no`, `abrir_modelo`, `rodar_missao` e outras |
| `ferramenta` | quando é uma ferramenta: `pareto`, `ishikawa`, `fmea`, `msa`, `capacidade`, `carta`, `oito_d`, `amostragem`... |
| `entrada` | os dados de partida: caminhos de arquivo com hash, ou o próprio conteúdo pequeno |
| `contexto` | onde o resultado vai morar: projeto `.lss` e entregável, modelo e nó, unidade e posto |
| `resposta` | caminho onde gravar a resposta |
| `somente_leitura` | abre só para ver |

Argumentos diretos continuam valendo para os casos simples: `utsim <modelo.ron>`, `utsim --utsim-no <nome>` para abrir o editor já no nó.

### 2.3 Resposta

Quem atende grava a **resposta** (`.utsim-resposta.json`) no caminho pedido, de uma vez (gravação atômica de `utsim-pasta`), ao clicar em "Concluir e voltar":

| Campo | O que é |
|---|---|
| `pedido` | o `id` do pedido |
| `estado` | `concluido`, `cancelado` ou `erro` |
| `resultado` | o resultado da ferramenta no formato de troca dela (seção 2.4) |
| `arquivos` | arquivos gerados (relatório HTML, PDF, CSV), com hash |
| `origem` | app, versão, quem, quando |

O chamador vigia o arquivo de resposta (consulta a cada segundo enquanto espera, sem travar a tela). Ao achar, confere o `id` e os hashes e grava o resultado no seu lugar (entregável do projeto, aba do nó, registro da unidade). Se o usuário fechar o outro app sem concluir, o pedido expira e nada muda.

### 2.4 Formato de troca das ferramentas

Cada ferramenta tem um tipo serde estável em `utsim-ferramentas` (ou `utsim-quality`) que é ao mesmo tempo a entrada e o resultado: `Pareto`, `Diagram` do Ishikawa, linhas do FMEA, `Chain` dos 5 porquês, plano de amostragem, carta de controle, estudo de MSA, capacidade, `OitoD`. Gravado em JSON com `{ "ferramenta": ..., "formato": n, "dados": ... }`, com migração quando o formato mudar. O Seis Sigma guarda esse JSON como entregável. O UTSim guarda as partes de nó dentro do próprio nó.

### 2.5 Dados do nó em mão dupla

O que é do nó é editado no arquivo do modelo. O UTSim mostra as abas Qualidade, Segurança e Manutenção no painel de propriedades do nó. O UTSim Qualidade e o UTSim Segurança do Trabalho leem os mesmos campos do `.ron` e gravam neles por comandos de edição (`utsim-edit`), com desfazer. Para não haver dois escritores ao mesmo tempo no mesmo modelo, o app que abre o modelo para editar grava uma trava ao lado do arquivo (`.<modelo>.trava`, com app, pessoa e hora). O outro abre só para leitura e avisa, e recarrega quando a trava some.

## 3. Campos por nó e edição nos dois sentidos

### 3.1 O que o nó guarda

Tudo no próprio objeto do `.ron`, usando o que já existe e completando o resto. Campos ausentes deixam o modelo igual ao de antes.

| Aba no painel do nó | Campo do objeto | Já existe | O que guarda |
|---|---|---|---|
| Qualidade | `quality` | sim | defeitos (probabilidade, fator do ciclo ou do rótulo), características medidas com limites, custos unitários |
| Qualidade | `inspection` | sim | frequência, detecção, alarme falso, medição com desvio, destino da rejeitada, limite de retrabalho, custo |
| Qualidade | `rework` | sim | chance de corrigir por tipo, custo |
| Qualidade | `sink_role` | sim | cliente ou refugo |
| Qualidade | `quality.controle` | **novo** | linhas do plano de controle da característica (método, frequência, amostra, carta, reação) e plano de amostragem da inspeção |
| Segurança | `ppe` | sim | EPI exigido no posto |
| Segurança | `zones` do modelo | sim | zonas de risco desenhadas no piso (o nó só mostra as zonas que o tocam) |
| Segurança | `safety` | **novo** | perigos do posto (grupo, descrição, fonte, probabilidade, severidade, controles, ações) e marcações de ergonomia por elemento |
| Manutenção | `downtime` | sim | MTBF, MTTR, desgaste, preventivas, equipe de reparo, custos, OEE |

O que não é de um nó continua na pasta de unidade: pessoas e dados de saúde (pasta restrita), incidentes, 8D, inventário da organização inteira. O registro da unidade guarda só a referência ao modelo e ao nó, nunca uma cópia dos campos acima. Os perigos e as marcações que hoje estão na pasta de unidade migram para o `safety` do nó, e a pasta passa a guardar só o que não tem nó.

### 3.2 Fluxo de edição

1. **No UTSim:** o painel de propriedades do nó ganha as abas Qualidade, Segurança e Manutenção, editadas pelos comandos de `utsim-edit` (com desfazer). A interface implementa as abas. O formato e os comandos são comuns.
2. **No UTSim Qualidade e no UTSim Segurança do Trabalho:** o app abre o mesmo `.ron`, mostra os resultados da simulação por nó e por fluxo, e edita os mesmos campos pelos mesmos comandos de `utsim-edit`, gravando o modelo inteiro de uma vez (gravação atômica).
3. **Um escritor por vez:** quem abre o modelo para editar grava `.<modelo>.trava` ao lado do arquivo (app, pessoa, hora, renovada a cada minuto). O outro app abre só para ver, avisa quem está editando e recarrega quando o arquivo muda ou a trava some. Trava com mais de cinco minutos sem renovar é considerada abandonada.
4. **Mesmos comandos nos três apps:** `SetQuality`, `SetInspection`, `SetRework`, `SetPpe`, `SetSafety`, `SetDowntime` e `SetZone` (este já existe), cada um trocando o bloco inteiro do nó, um passo de desfazer.

## 4. Mapa por par de apps

| De | Para | Vai | Volta |
|---|---|---|---|
| UTSim | Qualidade | modelo e nó com defeitos, inspeção, retrabalho, medições simuladas, custos | planos de controle e de amostragem aplicados ao nó, desvio de medição |
| UTSim | Segurança | modelo e nó com zonas, EPI, rotas, exposição simulada, quase-conflitos | perigos e EPI do posto gravados no nó, zonas ajustadas |
| UTSim | Seis Sigma | modelo, cenários, resultados (já existe: abrir no UTSim) | parâmetros alterados por experimento, cenário melhorado |
| UTSim | Ensino | missão abre o modelo do aluno (já existe) | resultado do aluno para a nota |
| UTSim | linhas | pacotes de linha (Lean Office, Qualidade, Segurança) | objetos colocados no modelo |
| Seis Sigma | Qualidade | pedido de ferramenta com os dados do projeto (Pareto, Ishikawa, FMEA, MSA, capacidade, carta, 5 porquês) | a ferramenta pronta, gravada como entregável do projeto |
| Qualidade | Seis Sigma | 8D sem causa achada (já existe: entrada da carteira) | o projeto aberto, para ligar ao 8D |
| Segurança | Qualidade | perigo ou incidente que pede análise de causa | Ishikawa, 5 porquês ou 8D ligado ao incidente |
| Qualidade | Segurança | falha do FMEA com efeito sobre pessoas | perigo do inventário ligado à linha do FMEA |
| Segurança | Ensino | casos de segurança | resultado do aluno |
| Qualidade | Ensino | casos de qualidade (já existe: pacote de missões) | resultado do aluno |
| Ensino | Qualidade | missão que pede uma ferramenta (Pareto dos defeitos, carta) | a ferramenta feita pelo aluno, para conferir |
| launcher | todos | onde cada app está instalado, versão ativa | nada |

## 5. Ordem proposta

1. Crate `utsim-troca` com pedido, resposta, busca do app e vigia da resposta, mais testes de ida e volta entre dois processos de teste.
2. Formato de troca das ferramentas que já existem em `utsim-ferramentas` e `utsim-quality`.
3. Área "Ferramentas" no UTSim Qualidade, que funciona sozinha (sem projeto) e também atende pedidos.
4. Seis Sigma abre Pareto, Ishikawa e FMEA no Qualidade e recebe o resultado como entregável (com a sessão do Seis Sigma).
5. Abas Qualidade, Segurança e Manutenção no painel do nó do UTSim, com a trava de edição (com a sessão da interface).
6. Os demais pares da seção 3, na ordem que o dono preferir.

## 6. Decisões do dono

| # | Pergunta | Proposta |
|---|---|---|
| I1 | Um app abre outro por arquivo de pedido e resposta, sem servidor? | Sim, como na seção 2. Funciona em qualquer máquina e pelo Drive. |
| I2 | Quando o Seis Sigma abre uma ferramenta no Qualidade, o resultado fica guardado onde? | No projeto `.lss`, como entregável, no formato de troca. O Qualidade não guarda cópia. |
| I3 | O Seis Sigma mantém as telas próprias de FMEA, Ishikawa e Pareto durante a transição? | Sim, até a tela do Qualidade cobrir tudo o que a dele faz. Depois o botão passa a abrir no Qualidade. |
| I4 | Dado do nó editado em dois apps ao mesmo tempo? | Um escritor por modelo, com trava ao lado do arquivo, e o outro só lê. |
| I5 | Sem o Qualidade instalado, o Seis Sigma faz o quê? | Mostra o que falta e abre o launcher para instalar. Não cai para uma cópia local. |
| I6 | Perigos e ergonomia do posto moram no nó ou na pasta de unidade? | No nó (`safety`), como o dono pediu. A pasta de unidade guarda o que não tem nó (incidentes, pessoas, 8D). |
| I7 | Plano de controle mora no nó? | Sim, a linha da característica fica em `quality.controle`. Planos de postos sem modelo ficam na unidade. |
