# Páginas de exemplo do Google Drive

Reconstruções das páginas HTML que o Drive devolve no lugar do arquivo, usadas pelos testes
de `drive::interpret` e do download (`tests/drive_paginas.rs`). Foram escritas a partir do
formato conhecido da página, sem acessar o Drive. Quando um download real mostrar uma página
diferente, salve-a aqui (sem cookies nem dados pessoais) e acrescente um caso ao teste.

- `drive_aviso_virus.html`: aviso atual (drive.usercontent.google.com, formulário
  `download-form` com `id`, `export`, `confirm` e `uuid`), em inglês.
- `drive_aviso_virus_pt.html`: o mesmo aviso em português, com aspas simples e campos extras.
- `drive_aviso_legado.html`: formato antigo (drive.google.com/uc, link com `confirm=<token>`).
- `drive_cota_excedida.html`: limite de downloads do arquivo atingido.
- `drive_sem_permissao.html`: arquivo não compartilhado (página de login).
