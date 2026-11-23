# Tradução dos apps UTSim

Todos os textos que os apps mostram ficam em arquivos de texto, um por app e por idioma, que
um tradutor edita sem programar. O português do Brasil é a fonte: todo texto nasce nele, e um
texto que falta em outro idioma aparece em português.

Idiomas de hoje: `pt-BR` (fonte), `en`, `es`, `fr` e `it`. Os arquivos `fr` e `it` foram
traduzidos por máquina e aguardam revisão humana (o cabeçalho de cada um avisa).

Apps já nos arquivos: UTSim Ensino (`ensino.ftl`), UTSim Seis Sigma (`seis-sigma.ftl`) e as
telas próprias do UTSim Qualidade (`qualidade.ftl`) e do UTSim Segurança do Trabalho
(`seguranca.ftl`), o UTSim Estatística (`estatistica.ftl`) e o Lançador (`lancador.ftl`). Os
outros seguem o plano do fim desta página.

## Onde ficam

```
idiomas/
  pt-BR/ensino.ftl   pt-BR/seis-sigma.ftl
  en/ensino.ftl      en/seis-sigma.ftl
  es/...  fr/...  it/...
```

O formato é o [Fluent](https://projectfluent.org/) (`.ftl`). Cada linha `chave = texto` é
uma entrada:

```ftl
aba_ajustes = Ajustes
ato_titulo = Ato { $numero }. { $titulo }
t-days_left = { $caso ->
        [hoje] vence hoje
        [um] falta 1 dia
        [futuro] faltam { $abs } dias
        [menos_um] atrasado 1 dia
       *[atraso] atrasado { $abs } dias
    }
```

Regras para quem traduz:

- Traduza só o que vem depois do `=`. A chave (antes do `=`) nunca muda.
- `{ $nome }` é um valor que o app preenche. Mantenha cada um, com o mesmo nome; a ordem na
  frase pode mudar.
- Numa escolha (`{ $n -> [one] ... *[other] ... }`) os casos entre colchetes ficam como
  estão; traduza o texto de cada caso. `[one]` e `[other]` seguem as regras de plural do
  idioma; `[0]`, `[1]` são números exatos.
- Um texto de várias linhas continua com quatro espaços no começo de cada linha.
- `{"{"}`, `{"}"}` e `{" "}` são chaves e espaços literais: deixe como estão.
- Linhas que começam com `#` são comentários; `##` separa os grupos de telas.
- Nomes de produtos (UTSim, Seis Sigma, Ensino) não se traduzem. Valores em dinheiro seguem
  em R$, porque a moeda é da empresa, não do idioma.

Números, datas e dinheiro saem no formato do idioma sem que o tradutor faça nada: vírgula
decimal em todos menos no inglês, datas `DD/MM/AAAA` (`AAAA-MM-DD` no inglês), milhar com
ponto (vírgula no inglês, espaço no francês).

## Testar sem compilar

Os arquivos vão para dentro do programa quando ele é compilado, então o app continua um único
`.exe`. Para ver uma tradução sem compilar, crie uma pasta `idiomas` ao lado do `.exe` (ou
aponte a variável `UTSIM_IDIOMAS` para outra pasta) com a mesma estrutura, por exemplo
`idiomas/fr/seis-sigma.ftl`. As entradas dessa pasta valem por cima das do programa; o resto
continua como veio. Feche e abra o app para ver a mudança.

O idioma se escolhe em Ajustes (Ensino) ou na tela inicial (Seis Sigma) e fica salvo na
configuração de cada app. Na primeira vez o app segue o idioma do Windows.

## Conferir o que falta

```bash
cargo run -p utsim-idiomas
```

lista, para cada idioma e app, as chaves que faltam (aparecem em português), as que sobram
(o pt-BR não tem mais), os valores `{ $... }` diferentes do pt-BR e os erros de sintaxe. Para
conferir uma pasta de teste: `cargo run -p utsim-idiomas -- caminho/da/pasta/idiomas`. O
programa compilado (`utsim-idiomas.exe`) faz o mesmo sem o Rust.

O teste `cargo test -p utsim-idiomas` falha nos mesmos casos, então nenhuma versão sai com um
idioma incompleto.

## Acrescentar um idioma

1. Crie `idiomas/<código>/` copiando os arquivos de `idiomas/en/` (ou de outro idioma
   próximo) e traduza.
2. Em `crates/utsim-idiomas/src/lib.rs`, acrescente o idioma ao `enum Idioma` (com `codigo`,
   `nome`, `de_codigo`, separadores decimal e de milhar) e ao `catalogo!`.
3. No Seis Sigma, acrescente-o também ao `Language` de `utsim-lss-store` (o valor salvo na
   configuração) e às duas conversões de `crates/utsim-lss-app/src/text.rs`.
4. Rode `cargo run -p utsim-idiomas` e `bash scripts/verificar.sh`.

## Acrescentar um texto (para quem programa)

- Ensino: declare a entrada em `crates/utsim-ensino-ui/src/texto/cat_*.rs` (`nome;` num
  bloco `textos!`, ou `nome(valor1, valor2);` num bloco `modelos!`) e escreva a chave em
  `idiomas/*/ensino.ftl`. As telas usam `texto::nome()` ou `texto::f(texto::nome(), &[&a, &b])`.
- Receitas de análise (`utsim-lss-core/src/analises.rs`) e abas dos nós
  (`utsim-edit/src/abas.rs`): o mesmo `tx`/`fx`, de `src/texto.rs` da crate, com as chaves em
  `idiomas/*/receitas.ftl` e `idiomas/*/abas.ftl`. Nas receitas, o rótulo de um valor
  (`o.put("média", ..)`) fica em português no resultado, porque é procurado pelo nome e
  comparado ao refazer a análise: um rótulo novo entra em `LINHAS` (ou `MODELOS`, quando leva
  um nome) de `utsim_lss_core::texto`, e quem mostra o resultado chama `texto::rotulo`. Nas
  abas, as escolhas e o `sim`/`não` dos interruptores são valores do arquivo e não se traduzem.
- Qualidade, Segurança, Estatística e Lançador: `tx("chave")` ou `fx("chave", &[("nome", vx(&valor))])` de
  `src/texto.rs` de cada app (`dx` para um número com casas fixas, já com a
  marca decimal da língua). As chaves começam pelo arquivo da tela (`cep-`, `custos-`...).
- Seis Sigma: um campo novo na tabela de `crates/utsim-lss-app/src/text*.rs`, preenchido com
  `tx("chave")` ou `|a| fx("chave", &[("a", a.into())])`, e a chave em
  `idiomas/*/seis-sigma.ftl`.
- Escreva o pt-BR primeiro. Nos outros idiomas, se não houver tradução ainda, copie o texto
  em português: o teste aceita, e a conferência mostra o que traduzir depois.

## Plano para os outros apps

Cada rodada migra um app, no mesmo molde: o app passa a depender de `utsim-idiomas`, os
textos saem do código para `idiomas/*/<app>.ftl` (pt-BR primeiro, os outros quatro idiomas
traduzidos), o seletor de idioma (`utsim_idiomas::seletor`) entra nos ajustes e o idioma fica
salvo na configuração do app. Cada rodada combina antes com a sessão dona do app.

| Rodada | App | Arquivo | Observação |
|---|---|---|---|
| 2 | UTSim Qualidade | `qualidade.ftl` | Feita: as telas próprias do app (Simulação, Custos, CEP, Planos, 8D, barra lateral). Início, Postos, Modelo e Ferramentas vêm de `utsim-posto-ui`, `utsim-edit` e `utsim-ferramentas`, e os nomes das disciplinas do 8D de `utsim-qualidade-core`: ficam para a rodada 6. A língua fica em `qualidade_seguranca.json`, comum ao Segurança. |
| 3 | UTSim Segurança | `seguranca.ftl` | Feita: as telas próprias do app (Inventário de riscos, Matriz de risco, Exposição, Ergonomia, Checklist, EPI, Incidentes, Relatório, barra lateral). Início, Postos, Modelo e Ferramentas ficam para a rodada 6, como no Qualidade. |
| 4 | UTSim Estatística | `estatistica.ftl` | Feita: menus, barra lateral, planilha, diálogos, formulários das análises, resultados, histórico, Modelo e relatórios; os números com a marca decimal da língua. Os rótulos que vêm das receitas de `utsim-lss-core` (comuns com o Seis Sigma) e os nomes das colunas das planilhas que o app cria seguem em português: ficam para a rodada 6. Os rótulos e as opções dos formulários guardam a chave e traduzem na hora de mostrar (`analise::Campo`, `doe::PLANOS`). |
| 5 | Lançador (`utsim-launcher`) | `lancador.ftl` | Feita: tela do código, barra, cartões e lista, menu "…", configurações, notas, desinstalar, rodapé, abertura e os erros do próprio lançador; datas e plurais pela língua. O seletor fica em Configurações › Aparência e a língua em `LauncherState::language`. Os erros que vêm de `utsim-launcher-core` (rede, assinatura, download) e o relato de erro (para o autor) seguem em português. O idioma escolhido aqui ainda não vira o padrão dos outros apps: cada app segue o sistema até alguém escolher nele. |
| 6 | `utsim-posto-ui`, `utsim-edit` (abas dos nós), `utsim-ferramentas`, receitas de `utsim-lss-core`, nomes do 8D, colunas do DOE e `utsim-ui-tema` | `receitas.ftl`, `abas.ftl`, `posto.ftl`, `tema.ftl` | Em andamento. Feitas: as receitas de análise (`receitas.ftl`: nome, conclusão, avisos, erros e, na tela do Seis Sigma, os rótulos dos valores por `texto::rotulo`) e as abas Qualidade, Segurança e Manutenção dos nós (`abas.ftl`). Ficam em português, por ora, as mensagens de `experimentos.rs` e `simulacao.rs` do `utsim-lss-core` (atributos `#[error]` e textos guardados no projeto). Parte 2 feita: as telas comuns de Qualidade e Segurança (`posto.ftl`: Início, Postos, Modelo e Ferramentas; os tipos de carta, as regras de sinal e os papéis das colunas guardam o valor gravado e traduzem o nome na tela), as ferramentas comuns (`ferramentas.ftl`: sensos do 5S, categorias do 6M por `ishikawa::category_name`, achados do poka-yoke e do QFD), os nomes das disciplinas do 8D (`qualidade.ftl`, `oito_d::disciplina`) e as colunas do plano do DOE (`estatistica.ftl`). A planilha do plano sai na língua de agora e se lê de volta em qualquer uma (`doe::e_coluna`, com `Catalogo::em_todas`). Seguem em português, por serem dados que o código lê pelo nome: as colunas das tabelas das ferramentas (`utsim-estudo`, ex.: "Ponto de venda" do QFD), a lista inicial do 5S e os tipos de nó do modelo. Rodada 7: o tema lê os textos de `tema.ftl` (`Rotulos::atual`, `Textos::atual`, `TextosData::default`, nomes dos filtros por `Filtro::nome_na_tela`; `Rotulos::PT` e `Textos::PT` continuam), os limites opcionais da tela Ferramentas viram chaves, e as mensagens de `experimentos.rs` e `simulacao.rs` do `utsim-lss-core` (erros, linhas do plano, conclusão e justificativa do ganho) vão para `receitas.ftl`; o que é gravado no projeto sai na língua de agora. Seguem em português as mensagens de erro do `utsim-doe`. |
| 7 | Editor do simulador (`utsim-viewer`) | `editor.ftl` | Em andamento, dividido por janela. Parte 1 feita (chamada de rodada 8 nas mensagens): a barra do topo com o seletor de língua (guardado em `%APPDATA%/UTSim/editor.json`), os diálogos Salvar como, Novo modelo, Atalhos e Não é possível simular, o painel da simulação e as estatísticas dos blocos e fluidos (`ui.rs`), as camadas Rede e Fluidos (`layers.rs`), a Biblioteca, a lista de objetos, a seleção múltipla e as ligações (`editor.rs`). `Layer::label` continua o nome em português que os roteiros e o estado gravado usam; a tela usa `layers::layer_name`. Um roteiro (`--roteiro`) roda sempre em português. Parte 2 feita: o painel de propriedades (`properties/`: campos de cada tipo, distribuições, rótulos, lote, saída, quebras, turnos e calendários, chegadas por período, nós e arestas da rede, nós de fluido, aparência). `spec::kind_name` continua em português (roteiros e seleção por tipo); a tela usa `spec::kind_label`. As listas fixas (`FAMILIES`, `RACK_PLACEMENTS`, `DISPATCH_RULES`, `MODES`) guardam chaves de `editor.ftl`. Um teste confere que toda chave escrita no código existe no arquivo. Parte 3 feita: a janela Resultados (`results_panel.rs`, `results.rs`: Resumo, Gráficos, Cenários e Otimização, as colunas e os tipos da tabela, os avisos dos formulários), com plural de verdade nas contagens ("1 replicação", "3 replicações"). Outro teste confere que cada `fx` passa exatamente as variáveis da mensagem em português. Parte 4 feita: fluxos (`flows*.rs`; `KINDS` guarda chaves), câmeras (`CameraKind::display` na tela; `label` continua em português para os roteiros), o seletor de visual dos modelos e os nomes dos estados dos blocos (`texto::estado`, no painel do bloco e nos gráficos de estados; `utsim_scene::style::state_label` continua em português para os roteiros e o Godot). O modo roteiro (`script.rs`, `roteiro.rs`) fica em português de propósito: ele acha os botões pelo rótulo em português. Faltam: os nomes da paleta e das categorias (`utsim-edit`) e os rótulos de fluido de `utsim-scene`. |
| 8 | Cliente 3D (Godot) | `godot.ftl` | Godot lê os textos pela ponte `utsim-godot` (mesma crate), sem um segundo sistema. Começado: `utsim_godot::texto::tx` e as chaves da janela Configurações e do botão `{ }` dos campos de distribuição; depois as abas Qualidade, Segurança e Manutenção dos nós e os painéis ao vivo (`ui_abas.rs`, `ui_ao_vivo.rs`), o conteúdo dos painéis ao vivo (`ao_vivo.rs`: as linhas `linha(chave, rótulo, valor, ajuda)` traduzem rótulo e ajuda, a chave continua o nome do nó), os fluxos de tokens (`ui_flows.rs`) e a janela Resultados (`ui_results.rs`: `TABS` e `CHARTS` guardam chaves). Os nomes de nó que os roteiros procuram não mudam. Faltam as outras janelas (`ui_props.rs`, `ui_class.rs`, `ui_pert.rs` e as demais). |
