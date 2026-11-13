//! Rampas: quem anda ou dirige numa rampa vai mais devagar.
//!
//! Só contam os trechos que são rampas de verdade, as arestas da rede de corredores com
//! desnível ([`Ramps`]): a subida até a ponta de uma esteira (a altura onde a peça é
//! deixada) não é uma rampa.
//!
//! Regras (a inclinação `g` é a subida ÷ a distância em planta, com sinal):
//! - **A pé** ([`walk_factor`]): a função de Tobler para caminhada,
//!   `v(g) = 6·e^(−3,5·|g + 0,05|)` km/h, dividida pela velocidade no plano. A descida
//!   suave (até 10 %) não acelera ninguém numa fábrica: o fator fica no máximo 1. Subindo
//!   uma rampa de 1:12 (8,33 %), 0,75; descendo 20 %, 0,70.
//! - **Veículo** ([`vehicle_factor`]): empilhadeira, AGV e carrinho reduzem a velocidade
//!   em rampa por segurança, subindo ou descendo: `limitar(1 − 4·|g|, 0,3, 1)`. Em 10 %,
//!   0,6.
//!
//! O tempo de um trajeto é o de andar o comprimento equivalente `Σ Lᵢ ÷ fᵢ` na velocidade
//! do recurso; o transportador usa a velocidade média equivalente `v·L ÷ Σ (Lᵢ ÷ fᵢ)` no
//! trajeto inteiro (com a aceleração dele).

use std::collections::HashSet;

use utsim_core::task::{distance, path_length};

/// Fator de velocidade a pé numa inclinação `g` (com sinal).
pub fn walk_factor(g: f64) -> f64 {
    (-3.5 * ((g + 0.05).abs() - 0.05)).exp().min(1.0)
}

/// Fator de velocidade de um veículo numa inclinação `g` (com sinal).
pub fn vehicle_factor(g: f64) -> f64 {
    (1.0 - 4.0 * g.abs()).clamp(0.3, 1.0)
}

fn key(p: [f64; 3]) -> [u64; 3] {
    p.map(f64::to_bits)
}

/// Os trechos de rampa da rede (pares de pontas, nos dois sentidos) e a regra do
/// recurso.
#[derive(Clone, Debug, Default)]
pub struct Ramps {
    edges: HashSet<([u64; 3], [u64; 3])>,
    vehicle: bool,
}

impl Ramps {
    /// `edges`: as pontas de cada trecho de rampa; `vehicle`: a regra de veículo.
    pub fn new(edges: &[([f64; 3], [f64; 3])], vehicle: bool) -> Self {
        let mut set = HashSet::new();
        for (a, b) in edges {
            set.insert((key(*a), key(*b)));
            set.insert((key(*b), key(*a)));
        }
        Ramps {
            edges: set,
            vehicle,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    fn factor(&self, a: [f64; 3], b: [f64; 3]) -> f64 {
        let run = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        if run <= 0.0 {
            return 1.0;
        }
        let g = (b[2] - a[2]) / run;
        if self.vehicle {
            vehicle_factor(g)
        } else {
            walk_factor(g)
        }
    }

    /// Comprimento equivalente do trajeto: cada trecho de rampa conta `L ÷ f`.
    pub fn equivalent_length(&self, path: &[[f64; 3]]) -> f64 {
        let mut slow = false;
        let mut total = 0.0;
        for w in path.windows(2) {
            let d = distance(w[0], w[1]);
            if self.edges.contains(&(key(w[0]), key(w[1]))) {
                slow = true;
                total += d / self.factor(w[0], w[1]);
            } else {
                total += d;
            }
        }
        // No ramp on the way: exactly the length (bit for bit).
        if slow { total } else { path_length(path) }
    }

    /// Velocidade média equivalente no trajeto: `speed` sem rampa.
    pub fn speed_on(&self, path: &[[f64; 3]], speed: f64) -> f64 {
        if self.edges.is_empty() {
            return speed;
        }
        let eq = self.equivalent_length(path);
        let len = path_length(path);
        if eq > len && eq > 0.0 {
            speed * len / eq
        } else {
            speed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_up_a_1_in_12_ramp_is_three_quarters_of_the_speed() {
        assert!((walk_factor(0.0) - 1.0).abs() < 1e-12);
        assert!((walk_factor(1.0 / 12.0) - 0.7470).abs() < 1e-3);
        assert_eq!(walk_factor(-0.08), 1.0);
        assert!((walk_factor(-0.2) - 0.7047).abs() < 1e-3);
        assert!((vehicle_factor(0.1) - 0.6).abs() < 1e-12);
        assert_eq!(vehicle_factor(0.5), 0.3);
    }

    #[test]
    fn only_the_ramp_edges_slow_down() {
        let a = [0.0, 0.0, 0.0];
        let b = [12.0, 0.0, 1.0];
        let c = [12.0, 5.0, 1.0];
        let r = Ramps::new(&[(a, b)], false);
        let path = [a, b, c];
        let eq = r.equivalent_length(&path);
        let ramp = distance(a, b);
        assert!((eq - (ramp / walk_factor(1.0 / 12.0) + 5.0)).abs() < 1e-9);
        // The same climb not on a ramp edge (to a belt end) counts as is.
        assert_eq!(r.equivalent_length(&[c, a]), path_length(&[c, a]));
        assert!(r.speed_on(&path, 1.0) < 1.0);
    }
}
