//! Navegação no piso do UTSim: obstáculos, menor caminho livre, pontos de acesso e geração
//! de rede. Crate pura (sem interface, sem relógio, sem sorteio): a mesma entrada dá sempre a
//! mesma saída.
//!
//! - [`body`]: área ocupada de cada objeto pela forma e pelo tamanho (as mesmas caixas do
//!   desenho 3D), obstáculos da planta CAD, pontos de acesso por tipo e folga por papel do
//!   recurso (pessoa, empilhadeira, AGV);
//! - [`grid`]: grade de ocupação com os obstáculos inflados pela folga, A* com suavização
//!   por linha de visada (string pulling);
//! - [`router`]: os roteadores dos modos `Free` e `Network` (trait `utsim_core::Router`),
//!   com cache por par origem-destino;
//! - [`generate`]: rede de caminhos (`NetworkDef`) a partir do caminho livre.
//!
//! Decisão: grade + A* em vez de grafo de visibilidade. A grade cresce com a área e não com o
//! número de vértices (plantas com centenas de colunas ficam rápidas), trata paredes finas e
//! polígonos não convexos sem casos especiais, e a folga é exata (distância ao polígono, a
//! soma de Minkowski com um círculo). O preço é o comprimento: em volta de um canto o
//! caminho suavizado fica a menos de um lado de célula (0,25 m) do menor caminho exato.

pub mod body;
pub mod generate;
pub mod geom;
pub mod grid;
pub mod router;

pub use body::{
    Body, Obstacle, Role, Shape, cad_obstacles, default_clearance, half_extents, half_width,
    role_of_asset,
};
pub use generate::{Generated, generate_network};
pub use geom::Pt;
pub use grid::{GridOptions, NavGrid, NavPath};
pub use router::{FreeRouter, NetworkModeRouter};
