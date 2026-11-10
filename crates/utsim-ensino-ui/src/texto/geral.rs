//! Catalogue of the frame of the app: identity, top bar, map, packages, glossary, settings,
//! the window and the command line.

/// The name of the app (the same in every language).
pub const NOME_APP: &str = "UTSim Ensino";

textos! {
    subtitulo;
    identidade_titulo;
    identidade_texto;
    prever_antes;
    sem_identidade;
    medir;
    simular;
    cancelar;
    areia_texto;
    limite_cola;
    prova_texto;
    desafio_texto;
    programa_texto;
    outros_pacotes;
    outros_pacotes_texto;
    ok;
    aba_campanha;
    aba_glossario;
    aba_turma;
    aba_professor;
    aba_ajustes;
    nome;
    registro_academico;
    apelido_opcional;
    comecar;
    campanha_nao_carregou;
    bloqueada_ato;
    bloqueada_anterior;
    caixa_de_areia;
    pacotes;
    abrir_pacote;
    voltar_campanha;
    glossario_texto;
    tamanho_letra;
    idioma;
    idioma_texto;
    modo_professor;
    trocar_aluno;
    prova_sem_dicas;
    sem_pasta_usuario;
    sem_perfil;
    voltar_3d;
    faltam_argumentos;
    missao_nova;
    ajuda_cli;
}

modelos! {
    requisitos_aviso(lista);
    pacote_nao_abriu_voltei(c, e);
    config_nao_salva(e);
    ato_titulo(numero, titulo);
    melhor_nota(nota, tentativas);
    pacote_nao_abriu(e);
    glossario_missoes(usado);
    glossario_manual(m);
    pacote_em_outra_lingua(nome);
    chave_professor_falhou(e);
    pasta_perfil(dir);
    nucleo_versao(utsim_ensino_core);
    pacote_versao(id, versao, hash);
    prova_linha(fuso_min, fuso_min2, tentativas);
    prova_tempo(duracao_min);
    prova_restam(minutos);
    prova_nao_da(e);
    erro(e);
    erro_janela(e);
    pacote_gravado(p);
}
