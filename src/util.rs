//! Small deterministic helpers shared across modules.

/// FNV-1a: a hash that never changes between runs or Rust versions, so the
/// same workload always gets the same look in the city.
pub fn stable_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Mixes numbers into one well-spread hash (splitmix64 steps), for stable
/// per-object variation such as which windows are lit.
pub fn mix(values: &[u64]) -> u64 {
    values.iter().fold(0x9e37_79b9_7f4a_7c15, |hash, &value| {
        let mut z = (hash ^ value).wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

/// A hash as a number in `0.0..1.0`.
pub fn unit(hash: u64) -> f32 {
    (hash >> 40) as f32 / (1u64 << 24) as f32
}

/// xorshift64*: a tiny seeded random number generator, enough for the demo.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // xorshift gets stuck on a zero state.
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A number in `0..bound`; `bound` must not be zero.
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    /// A random element; `items` must not be empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_hash_matches_the_fnv1a_reference() {
        assert_eq!(stable_hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(stable_hash("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn mix_is_stable_and_order_sensitive() {
        assert_eq!(mix(&[1, 2]), mix(&[1, 2]));
        assert_ne!(mix(&[1, 2]), mix(&[2, 1]));
        assert!((0..1000).all(|i| (0.0..1.0).contains(&unit(mix(&[i])))));
    }

    #[test]
    fn rng_is_deterministic_and_respects_bounds() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            let value = a.below(7);
            assert_eq!(value, b.below(7));
            assert!(value < 7);
        }
    }
}
