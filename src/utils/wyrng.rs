const NUMS_0: u64 = 2654435840;
const NUMS_1: u64 = 6364136223846793005;
const NUMS_2: u64 = 0xa0761d6478bd642f;
const NUMS_3: u64 = 0xe7037ed1a0b428db;

/// A fast and compact PRNG.
/// This is not cryptographically secure although it does perform very well in statistical tests
/// This is however quite irrelevant as it is only used for reducing the
/// frequency of certain code paths within the garbage collector and
/// this only requires a roughly uniform distribution of the output to work well.
pub struct WyRng {
    state: u64,
}

impl WyRng {
    pub fn new(seed: u32) -> Self {
        let seed = seed as u64;
        let state = seed
            .wrapping_sub(NUMS_0)
            .rotate_right(7)
            .wrapping_mul(NUMS_1);

        Self { state }
    }

    pub fn generate(&mut self) -> u32 {
        self.state = self.state.wrapping_add(NUMS_2);
        let t = self.state as u128 * (self.state * NUMS_3) as u128;
        return ((t >> 64) ^ t) as u32;
    }
}
