const NUMS_0: u64 = 2654435840;
const NUMS_1: u64 = 6364136223846793005;

/// A fast and compact PRNG.
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub fn new(seed: u32) -> Self {
        let seed = seed as u64;
        let state = seed.wrapping_sub(NUMS_0).wrapping_mul(NUMS_0);
        let inc = state.wrapping_add(NUMS_1);

        Self { state, inc }
    }

    pub fn generate(&mut self) -> u32 {
        let old_state = self.state;
        self.state = old_state.wrapping_mul(NUMS_1).wrapping_add(self.inc | 1);
        let xorshifted = ((old_state >> 18) ^ old_state) >> 27;
        let rot = old_state >> 59;
        ((xorshifted >> rot) | (xorshifted << ((rot.wrapping_neg()) & 31))) as u32
    }
}
