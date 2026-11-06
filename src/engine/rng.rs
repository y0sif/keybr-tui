/// Simple deterministic PRNG (xorshift32) — no external rand crate needed.
pub struct SimpleRng {
    state: u32,
}

impl Default for SimpleRng {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleRng {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u32;
        Self { state: seed | 1 } // ensure non-zero
    }

    #[cfg(test)]
    pub fn with_seed(seed: u32) -> Self {
        Self { state: seed | 1 }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    /// Return a random u32 in [0, bound).
    pub fn next_bounded(&mut self, bound: u32) -> u32 {
        self.next_u32() % bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_rng_produces_different_values() {
        let mut rng = SimpleRng::with_seed(42);
        let a = rng.next_u32();
        let b = rng.next_u32();
        let c = rng.next_u32();
        assert_ne!(a, b);
        assert_ne!(b, c);
    }
}
