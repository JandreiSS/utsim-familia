//! Reproducible random number streams.
//!
//! Every block gets its own stream derived from `(seed, replication, block index)`,
//! so adding a random draw in one block never shifts the numbers seen by another.

use rand_pcg::Pcg64;

pub type SimRng = Pcg64;

pub fn stream(seed: u64, replication: u32, stream_id: u64) -> SimRng {
    let hi = splitmix64(seed);
    let lo = splitmix64(seed ^ splitmix64(u64::from(replication).wrapping_add(1)));
    let state = (u128::from(hi) << 64) | u128::from(lo ^ splitmix64(stream_id));
    Pcg64::new(state, u128::from(stream_id))
}

fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    fn draws(mut rng: SimRng) -> Vec<u64> {
        (0..4).map(|_| rng.random()).collect()
    }

    #[test]
    fn streams_are_reproducible_and_distinct() {
        assert_eq!(draws(stream(1, 0, 0)), draws(stream(1, 0, 0)));
        assert_ne!(draws(stream(1, 0, 0)), draws(stream(1, 0, 1)));
        assert_ne!(draws(stream(1, 0, 0)), draws(stream(1, 1, 0)));
    }
}
