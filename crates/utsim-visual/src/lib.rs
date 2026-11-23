//! As ferramentas da qualidade desenhadas na tela e editadas em cima do desenho, comuns ao
//! UTSim Qualidade, ao UTSim Segurança do Trabalho e ao UTSim LSS.
//!
//! Cada componente recebe uma vista dos dados (o que desenhar) e devolve as ações da pessoa
//! (o que mudou). Quem chama aplica as ações nos seus dados: os formatos de arquivo de cada
//! app continuam os mesmos. A disposição (onde fica cada osso, barra ou célula) é uma
//! função pura, testada sem janela, e os testes em `tests/` clicam e arrastam de verdade
//! num quadro sem janela.
//!
//! - [`espinha`]: o Ishikawa como espinha de peixe, com as causas em cartões arrastáveis;
//! - [`arvore`]: uma árvore da esquerda para a direita (os 5 porquês);
//! - [`pareto`]: o gráfico de Pareto vivo, com a barra escolhida e a linha de 80 %;
//! - [`matriz`]: a matriz severidade × ocorrência da FMEA;
//! - [`casa`]: a casa da qualidade do QFD, com a matriz de relações e o telhado;
//! - [`cores`]: as cores de risco (baixo, médio e alto) e as de apoio.

pub mod arvore;
pub mod casa;
pub mod cores;
pub mod espinha;
pub mod matriz;
pub mod pareto;
pub mod texto;

mod edicao;
mod util;

pub use cores::Nivel;
