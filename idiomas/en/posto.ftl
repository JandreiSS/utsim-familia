
## abrir.rs

abrir-opcao_desconhecida = unknown option: { $opcao }
abrir-sem_valor = { $opcao } has no value
abrir-ferramenta_desconhecida = unknown tool: { $valor }
abrir-lie_nao_e_numero = LSL is not a number: { $valor }
abrir-lse_nao_e_numero = USL is not a number: { $valor }
abrir-subgrupo_invalido = invalid subgroup: { $valor }
abrir-mais_de_um_modelo_pedido = more than one model requested: the last one is kept
abrir-pedido = request: { $e }
abrir-ferramenta_desconhecida-2 = unknown tool: { $k }
abrir-pedido_sem_modelo_para_abrir = request has no model to open
abrir-este_app_nao_atende_a = this app does not handle the action { $outra }
abrir-o_estudo_do_pedido_nao = The study in the request was not open when it was closed: look for it in the unit.
abrir-o_arquivo_nao_tem_dados = { $nome }: the file has no data
abrir-coluna_que_nao_existe = column that does not exist: { $faltam }
abrir-sem_pasta_de_configuracao_para = No configuration folder for the standalone analysis unit.

## ferramentas.rs

ferramentas-a_caracteristica_nao_esta_no = The characteristic { $caracteristica } is not in the model.
ferramentas-analise_de = Analysis of { $nomes }
ferramentas-dados_de_escolha_a_ferramenta = Data of { $nomes }: choose the tool.
ferramentas-descartar = Discard
ferramentas-nao_consegui_gravar_o_estudo = could not save the study: { $err }
ferramentas-estudo_sem_titulo = Untitled study
ferramentas-estudo_criado = Study '{ $titulo }' created.
ferramentas-lie_e_lse_gravados_na = LSL and USL saved in the characteristic { $caracteristica } of node { $no }.
ferramentas-os_limites_nao_foram_para = The limits did not reach the node: { $e }
ferramentas-estudo_do_no = Study of node { $objeto }
ferramentas-nenhuma_unidade_aberta = No unit open.
ferramentas-ferramenta_nao_encontrada = Tool not found.
ferramentas-dados_puxados_da_simulacao = Data pulled from the simulation.
ferramentas-dados_puxados_da_simulacao_e = Data pulled from the simulation and saved in the study.
ferramentas-simulacao = Simulation: { $err }
ferramentas-abra_uma_unidade_na_tela = Open a unit on the home screen.
ferramentas-estudos = Studies
ferramentas-filtrar_por_titulo_ou_assunto = filter by title or subject
ferramentas-so_do_no = Only node { $objeto }
ferramentas-mostrar_todos = Show all
ferramentas-novo_estudo = New study
ferramentas-a_unidade_ainda_nao_tem = The unit has no studies yet.
ferramentas-nenhum_estudo_passa_pelo_filtro = No study matches the filter.
ferramentas-titulo = Title
ferramentas-problema_em_uma_frase = Problem in one sentence
ferramentas-posto = Workstation
ferramentas-nenhum = none
ferramentas-modelo = Model
ferramentas-dos_postos = of the workstations
ferramentas-no = Node
ferramentas-do_modelo = of the model
ferramentas-criar_estudo = Create study
ferramentas-cancelar = Cancel
ferramentas-ferramentas_da_qualidade_em_estudos = Quality tools in studies
ferramentas-um_estudo_aplica_as_ferramentas = A study applies the tools to a subject: a model node, a workstation or a problem. The data comes from the simulation, from a CSV or is typed in.
ferramentas-escolha_um_estudo_na_lista = Choose a study from the list or create a new one.
ferramentas-o_estudo_ainda_nao_tem = The study has no tools yet. Use "Add tool".
ferramentas-escolha_uma_ferramenta_do_estudo = Choose a tool of the study.
ferramentas-estudo_removido = Study removed.
ferramentas-titulo_do_estudo = study title
ferramentas-relatorio_do_estudo = Study report
ferramentas-confirmar_remocao = Confirm removal
ferramentas-nao = No
ferramentas-remover_estudo = Remove study
ferramentas-responsavel = owner { $autor }
ferramentas-aberto_em = opened on { $criado }
ferramentas-criado_no = created in { $nome }
ferramentas-problema = Problem
ferramentas-o_problema_em_uma_frase = the problem in one sentence
ferramentas-assunto = Subject
ferramentas-conclusao_do_estudo = Study conclusion
ferramentas-o_que_o_estudo_concluiu = what the study concluded and what will be done
ferramentas-ferramentas_no_relatorio_do_estudo = Tools in the study report
ferramentas-gravar_pdf_e_html_do = Save PDF and HTML of the study
ferramentas-relatorio_do_estudo_gravado = Study report saved.
ferramentas-gravado = Saved:
ferramentas-abrir = Open
ferramentas-nao_consegui_abrir = could not open { $h }: { $err }
ferramentas-ferramentas_do_estudo = Tools of the study:
ferramentas-adicionar_ferramenta = + Add tool
ferramentas-subir = Move up
ferramentas-antes_na_ordem_do_estudo = Earlier in the order of the study and the report
ferramentas-descer = Move down
ferramentas-remover = Remove
ferramentas-relatorio_desta_ferramenta_pdf_e = Report of this tool (PDF and HTML)
ferramentas-relatorio_da_ferramenta_gravado = Tool report saved.
ferramentas-dados = Data
ferramentas-leitura_da_equipe = Team's reading
ferramentas-o_que_o_resultado_mostra = what the result shows and what to do
ferramentas-resultado = Result
ferramentas-gravar_lie_e_lse_no = Save LSL and USL in the node
ferramentas-grava_estes_limites_na_caracteristica = Saves these limits in the characteristic { $caracteristica } of the model (Quality tab of node { $objeto }), with the model lock.
ferramentas-dados_editados_a_mao_a = Data edited by hand: the source is now Manual.
ferramentas-importar_e_exportar_dados = Import and export data
ferramentas-importar_dados = Import data
ferramentas-planilha_csv_xlsx = Spreadsheet (.csv, .xlsx)
ferramentas-aba = Sheet
ferramentas-importar_csv_ou_xlsx = Import CSV or xlsx
ferramentas-importado = { $nome } imported.
ferramentas-exportar_csv = Export CSV
ferramentas-tabela_gravada_em = Table saved in { $p }
ferramentas-puxar_da_simulacao = Pull from the simulation
ferramentas-o_estudo_nao_tem_no = The study has no model node. Choose one in "Subject".
ferramentas-a_simulacao_nao_oferece_leituras = The simulation offers no readings of this tool for the node.
ferramentas-leitura = Reading
ferramentas-replicacoes = Replications
ferramentas-rodar_e_preencher_a_tabela = Run and fill the table
ferramentas-simulando = simulating...
ferramentas-a_tabela_atual_e_trocada = The current table is replaced by the simulated result.
ferramentas-uma_linha_por_necessidade_do = One row per customer need, importance from 1 to 5. Each column after "Ponto de venda" is a technical requirement (+ column), with the relationship 0, 1, 3 or 9. Current, competitor and target go from 1 to 5, the sales point is 1, 1.2 or 1.5.
ferramentas-uma_linha_por_cota_o = One row per dimension. The coefficient is 1 for the dimension that adds and -1 for the one that subtracts. Without the upper tolerance, the dimension is symmetric. Distribution: normal (±3σ), normal 4 (±4σ), uniform or triangular. 100 thousand assemblies are simulated, with the same seed.
ferramentas-uma_linha_por_dispositivo_tipo = One row per device. Type: contact, fixed value or sequence. Function: control (lowers occurrence) or alert (lowers detection). Step and failure mode as in the FMEA, O and D ratings from 1 to 10 before and after.
ferramentas-pelo_nome_da_coluna_unidade = By column name: unit, direction (larger, smaller or target) and target value.
ferramentas-dois_requisitos_e_a_correlacao = Two requirements and the correlation: ++ and + when one helps the other, - and -- when one hinders the other.
ferramentas-cada_teste_do_dispositivo_data = Each test of the device: date (DD/MM/YYYY), whether it worked (1 or 0), who tested and a note.
ferramentas-nenhuma = none
ferramentas-especificacao = Specification
ferramentas-limites_do_resultado = Result limits
ferramentas-tamanho_do_subgrupo = Subgroup size
ferramentas-tipo_de_carta = Chart type
ferramentas-regras_de_sinal = Signal rules
ferramentas-tolerancia_lse_lie = Tolerance (USL − LSL)
ferramentas-outros_abaixo_de = Others below
ferramentas-severidade_critica_a_partir_de = Critical severity from
ferramentas-colunas = { $linhas ->
    [one] { $linhas } row
   *[other] { $linhas } rows
} · { $colunas ->
    [one] { $colunas } column
   *[other] { $colunas } columns
}
ferramentas-linha = + row
ferramentas-coluna = + column
ferramentas-remover_a_coluna = Remove the column
ferramentas-remover_a_linha = Remove the row
ferramentas-efeito = Effect
ferramentas-o_efeito = the effect
ferramentas-causa = + cause
ferramentas-remover_categoria = Remove category
ferramentas-causa-2 = cause
ferramentas-categoria = + category
ferramentas-nova_categoria = New category
ferramentas-o_problema = the problem
ferramentas-cadeia = Chain { $k }
ferramentas-por_que = + why
ferramentas-remover_cadeia = Remove chain
ferramentas-por_que-2 = Why { $j }
ferramentas-acao = Action
ferramentas-a_acao_sobre_a_causa = the action on the root cause
ferramentas-cadeia-2 = + chain
ferramentas-efeito-2 = Effect: { $efeito }
ferramentas-sem_causas = no causes
ferramentas-avisos = Warnings
ferramentas-acumulado_eixo_da_direita = cumulative % (right axis)
ferramentas-reta_de_minimos_quadrados = least-squares line
ferramentas-acumulado = cumulative
ferramentas-acumulado-2 = cumulative %
ferramentas-minimos_quadrados = least squares

## inicial.rs

inicial-a_unidade_e_uma_pasta = The unit is an ordinary folder on the computer (a factory, a sector or a laboratory). Everything you save here stays inside it. To work as a team, keep the folder inside a synced Google Drive folder: Qualidade, Segurança do Trabalho and Estatística open the same unit, and each one sees what the others saved.
inicial-escreva_seu_nome_acima_primeiro = Type your name above first: it signs what you save.
inicial-abra_ou_crie_uma_unidade = Open or create a unit first: the model is tied to it.
inicial-continuar_em = Continue in "{ $n }"
inicial-volte_ao_trabalho_na_unidade = Go back to work in the open unit.
inicial-abrir_outra_unidade = Open another unit
inicial-abrir_unidade = Open unit
inicial-escolha_a_pasta_de_uma = Choose the folder of a unit that already exists, for example the team's on Drive.
inicial-criar_unidade = Create unit
inicial-comece_uma_unidade_nova_voce = Start a new unit: you give it a name and choose where the folder goes.
inicial-abrir_modelo_do_utsim = Open UTSim model
inicial-unidades_recentes = Recent units
inicial-escolha_a_pasta_da_unidade = Choose the unit folder
inicial-nao_consegui_abrir_o_explorador = could not open the file explorer: { $e }
inicial-a_pasta_nao_e_uma = The folder { $p } is not a unit. Choose the folder that has the file unidade.json, or use "Create unit".
inicial-antes_de_comecar_diga_seu = Before you start, tell us your name
inicial-ele_vai_em_tudo_o = It goes on everything you save, so the team knows who changed what.
inicial-seu_nome = your name
inicial-pronto = Done
inicial-voce_esta_como = You are signed in as
inicial-trocar = Change
inicial-nome_da_unidade_a_fabrica = Unit name (the factory, sector or laboratory):
inicial-por_exemplo_fabrica_norte = for example: North Factory
inicial-onde_criar_a_pasta_para = Where to create the folder (for the team, choose a Google Drive folder):
inicial-onde_criar_a_pasta_da = Where to create the unit folder
inicial-sera_criada_a_pasta = The folder will be created: { $p }
inicial-escreva_o_nome_e_escolha = Type the name and choose where it goes.
inicial-cancelar = Cancel
inicial-unidade = Unit

## modelo.rs

modelo-disponibilidade = Availability (%)
modelo-oee = OEE (%)
modelo-falhas_por_replicacao = Failures per replication
modelo-mtbf_simulado = Simulated MTBF
modelo-mttr_simulado = Simulated MTTR
modelo-tempo_parado_em_corretiva = Downtime in corrective maintenance
modelo-tempo_em_preventiva = Time in preventive maintenance
modelo-preventivas_por_replicacao = Preventive tasks per replication
modelo-espera_media_pelo_tecnico = Average wait for the technician
modelo-custo_da_manutencao = Maintenance cost
modelo-replicacoes = Replications
modelo-em_edicao_por_no_desde = Being edited by { $pessoa } in { $app } since { $desde_ms }
modelo-modelo_aberto_para_edicao = Model open for editing.
modelo-o_no_nao_esta_no = { $antes } Node { $no } is not in the model.
modelo-a_tela_modelo_tem_mudanca = The Model screen has an unsaved change in this model. Save or undo it first.
modelo-edicao_solta_o_modelo_segue = Editing released. The model stays open read-only.
modelo-perdi_a_edicao = Editing was lost. { $av }
modelo-modelo_recarregado_do_disco = Model reloaded from disk.
modelo-o_modelo_esta_so_para = The model is read-only: someone else is editing.
modelo-nenhum_modelo_aberto = No model open.
modelo-gravado_no_modelo = Saved in the model.
modelo-o_modelo_esta_so_para-2 = The model is read-only.
modelo-abra_uma_unidade_na_tela = Open a unit on the home screen.
modelo-resumo_da_simulacao = Simulation summary: { $e }
modelo-resumo_de_manutencao = Maintenance summary: { $e }
modelo-nenhum_modelo_aberto-2 = No model open
modelo-escolha_o_ron_do_simulador = Choose the simulator .ron above and click Open.
modelo-modelo = MODEL
modelo-dos_postos = of the workstations
modelo-abrir = Open
modelo-fechar = Close
modelo-o_arquivo_mudou_no_disco = The file changed on disk and you have an unsaved change.
modelo-recarregar_perde_a_mudanca = Reload (loses the change)
modelo-manter_a_minha = Keep mine
modelo-gravar = Save
modelo-grava_no_modelo_ctrl_s = Saves in the model (Ctrl+S)
modelo-refazer = Redo
modelo-ctrl_y = Ctrl+Y
modelo-desfazer = Undo
modelo-ctrl_z = Ctrl+Z
modelo-editando = EDITING
modelo-voce_tem_a_trava_do = You hold the file lock. Click to release it and go read-only.
modelo-ninguem_edita_clique_para_editar = Nobody is editing. Click to edit.
modelo-so_leitura = READ-ONLY
modelo-clique_para_tentar_editar = { $dica } Click to try to edit.
modelo-mudanca_sem_gravar = unsaved change
modelo-recarregar_do_disco = Reload from disk
modelo-nos_do_modelo = Model nodes · { $nomes_todos }
modelo-filtrar_por_nome = Filter by name
modelo-nenhum_no_com_esse_nome = No node with that name.
modelo-no = Node
modelo-escolha_um_no_na_lista = Choose a node in the list on the left.
modelo-abrir_em_ferramentas = Open in Tools
modelo-nenhum_estudo = no study
modelo-1_estudo = 1 study
modelo-estudos = { $n } studies
modelo-manutencao_so_leitura_aqui_edite = Maintenance: read-only here. Edit it in UTSim.
modelo-nada_para_mostrar_neste_no = Nothing to show for this node.
modelo-resultados = Results
modelo-simulado = SIMULATED
modelo-resultado_do_modelo_nao_e = Model result, not a measurement from the factory.
modelo-do_modelo_nao_e_medicao = From the model, not a measurement from the factory.
modelo-simular_de_novo = Simulate again
modelo-no-2 = Node { $selecionado }
modelo-sem_resultado_simulado_para_este = No simulated result for this node.
modelo-modelo_inteiro = Whole model
modelo-manutencao_simulada = Simulated maintenance
modelo-media_das_replicacoes_tempos_na = Average of the replications. Times in the model's unit.
modelo-simulando = Simulating...
modelo-sem_resultado_de_manutencao_para = No maintenance result for this node.
modelo-adicionar = Add
modelo-mostrar = Show
modelo-esconder = Hide
modelo-remover = Remove
modelo-outro = other
modelo-chegada = Arrival
modelo-processo = Process
modelo-fila = Queue
modelo-saida = Exit
modelo-transportador = Conveyor
modelo-montagem = Assembly
modelo-separacao = Picking
modelo-transporte = Transport
modelo-estante = Rack
modelo-despacho = Dispatch

## postos.rs

postos-postos_sugeridos_pelo_modelo = { $n ->
    [one] { $n } workstation suggested by the model.
   *[other] { $n } workstations suggested by the model.
}
postos-abra_uma_unidade_na_tela = Open a unit on the home screen.
postos-postos_de_trabalho = Workstations
postos-atualizar = Refresh
postos-nada_novo_na_pasta = Nothing new in the folder.
postos-novo_posto = New workstation
postos-postos_na_unidade = { $n ->
    [one] { $n } workstation in the unit
   *[other] { $n } workstations in the unit
}
postos-escolha_um_posto_ou_crie = Choose a workstation or create a new one.
postos-editar_posto = Edit workstation
postos-nome = Name
postos-area = Area
postos-descricao = Description
postos-pessoas_expostas_por_turno = People exposed per shift
postos-epi_separados_por_virgula = PPE (comma-separated)
postos-modelo_ron = Model (.ron)
postos-objeto_do_modelo = Model object
postos-atividades_uma_por_linha = Activities (one per line)
postos-gravar = Save
postos-posto_gravado = Workstation saved.
postos-remover = Remove
postos-posto_removido = Workstation removed.
postos-fechar = Close
postos-importar_postos_do_modelo = Import workstations from the model
postos-modelo_ron-2 = Model (.ron):
postos-objeto_epi = { $nome } (object { $objeto }, PPE: { $epi })
postos-criar_os_postos_marcados = Create the selected workstations
postos-postos_criados = { $n ->
    [one] { $n } workstation created.
   *[other] { $n } workstations created.
}

## sessao.rs

sessao-nao_consegui_lembrar_a_configuracao = could not remember the configuration: { $e }
sessao-unidade_aberta = Unit '{ $nome }' opened.
sessao-unidade_criada = Unit '{ $nome }' created.
sessao-operacoes_novas_de_outras_pessoas = { $n ->
    [one] { $n } new operation from someone else.
   *[other] { $n } new operations from other people.
}
sessao-unidade_autor_operacoes = Unit: { $nome } · author: { $autor } · { $total_operacoes } operations
sessao-nenhuma_unidade_aberta = No unit open
sessao-autor = author: { $autor }
sessao-sem_unidade = No unit
sessao-modelo = model: { $nome }

## trabalho.rs

trabalho-a_tarefa_terminou_sem_resultado = the task ended without a result

## util.rs

util-arquivo_ron_escreva_ou_clique = .ron file: type or click Browse…
util-abre_o_explorador_de_arquivos = Opens the file explorer to choose the model.
util-escolha_o_modelo_do_utsim = Choose the UTSim model
util-data_dica = YYYY-MM-DD
util-data_invalida = Invalid date. Use YYYY-MM-DD (or DD/MM/YYYY).
util-data_antes = This date is before {"{"}limite{"}"}.

## Os papéis das colunas, os tipos de carta e as regras de sinal (ferramentas.rs)
ferramentas-coluna_do_papel = Column { $papel }
papel-categoria = category
papel-estrato = stratum
papel-valor = value
papel-quantidade = quantity
papel-custo_unitario = unit cost
papel-x = x
papel-y = y
papel-amostra = sample
papel-contagem = count
papel-tamanho = size
papel-subgrupo = subgroup
papel-peca = part
papel-operador = operator
carta-automatica = automatic by subgroup
carta-imr = I-MR (individuals)
carta-xbarr = X̄-R
carta-xbars = X̄-S
carta-p = p (proportion)
carta-np = np (defectives)
carta-c = c (defects)
carta-u = u (defects per unit)
regras-basicas = basic
regras-western = Western Electric
regras-nelson = Nelson

## Os limites opcionais da especificação (ferramentas.rs)
ferramentas-rotulo_lie = LSL
ferramentas-rotulo_lse = USL
ferramentas-rotulo_alvo = Target
ferramentas-rotulo_usar = use
