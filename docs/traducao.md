# Tradução dos apps UTSim

Todos os textos que os apps mostram ficam em arquivos de texto, um por app e por idioma, que
um tradutor edita sem programar. O português do Brasil é a fonte: todo texto nasce nele, e um
texto que falta em outro idioma aparece em português.

Idiomas de hoje: `pt-BR` (fonte), `en`, `es`, `fr` e `it`. Os arquivos `fr` e `it` foram
traduzidos por máquina e aguardam revisão humana (o cabeçalho de cada um avisa).

Apps já nos arquivos: UTSim Ensino (`ensino.ftl`), UTSim Seis Sigma (`seis-sigma.ftl`) e as
telas próprias do UTSim Qualidade (`qualidade.ftl`) e do UTSim Segurança do Trabalho
(`seguranca.ftl`). Os outros seguem o plano do fim desta
página.

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
- Qualidade e Segurança: `tx("chave")` ou `fx("chave", &[("nome", vx(&valor))])` de
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
| 4 | UTSim Estatística | `estatistica.ftl` | Nomes de testes e saídas numéricas com o formato do idioma. |
| 5 | Lançador (`utsim-launcher`) | `lancador.ftl` | Poucos textos; o idioma escolhido aqui pode virar o padrão dos outros apps. |
| 6 | `utsim-posto-ui`, `utsim-edit` (abas dos nós), `utsim-ferramentas` e `utsim-ui-tema` | `posto.ftl`, `tema.ftl` | O tema hoje recebe os rótulos prontos (`Rotulos`, `Textos`); passa a lê-los de `tema.ftl`. |
| 7 | Editor do simulador | `editor.ftl` | O maior volume; dividir por janela. |
| 8 | Cliente 3D (Godot) | `godot.ftl` | Godot lê os textos pela ponte `utsim-godot` (mesma crate), sem um segundo sistema. |
