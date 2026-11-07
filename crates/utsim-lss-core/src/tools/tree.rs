//! Forests of items linked by `pai`, robust to the cycles two people can close at once.
//! The rules live in `utsim_ferramentas::arvore` (decision G3); this module keeps the
//! Seis Sigma names with the project's [`Id`].

use crate::id::Id;

pub use utsim_ferramentas::arvore::{forest, paths};

/// A node in depth-first order.
pub type Node = utsim_ferramentas::arvore::Node<Id>;
