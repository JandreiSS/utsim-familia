//! Catalogue of the 2D view and the parts screens (vista2d.rs, pecas.rs). Entries start with `vista_`.

textos! {
    vista_continuar;
    vista_pausar;
    vista_do_inicio;
    vista_velocidade;
    vista_trabalhando;
    vista_ociosa;
    vista_bloqueada;
    vista_parada_ou_quebrada;
    vista_em_setup;
    vista_setup;
    vista_parada;
    vista_fora_do_turno;
    vista_sem_operador;
    vista_vazia;
    vista_fila;
    vista_chegadas;
    vista_saida;
    vista_balanc_ajuda;
    vista_tarefas_precedencias;
    vista_col_tarefa;
    vista_col_descricao;
    vista_col_tempo;
    vista_col_depois_de;
    vista_col_estacao;
    vista_arranjo_ajuda;
    vista_matriz_de_para;
    vista_col_de;
    vista_col_para;
    vista_col_viagens;
    vista_col_distancia;
    vista_col_momento;
}

modelos! {
    vista_tempo(t, unidade, stop_time);
    vista_balanc_resumo(takt, soma, estacoes_usadas, eficiencia);
    vista_estacao_n(s);
    vista_carga_de_takt(carga, takt);
    vista_tarefa_tempo(id, tempo);
    vista_depois_de_lista(depois_de);
    vista_arranjo_resumo(pos, celula);
    vista_celula_n(c);
}
