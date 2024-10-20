// A quadratic probe sequence.
#[derive(Default)]
pub struct Probe {
    // The current index in the probe sequence.
    pub i: usize,
    // The current length of the probe sequence.
    pub len: usize,
    // The current quadratic stride.
    stride: usize,
}

impl Probe {
    // Initialize the probe sequence.
    #[inline]
    pub fn start(hash: usize, mask: usize) -> Probe {
        Probe {
            i: hash & mask,
            len: 0,
            stride: 0,
        }
    }

    // Increment the probe sequence.
    #[inline]
    pub fn next(&mut self, mask: usize) {
        self.len += 1;
        self.stride += 1;
        self.i = (self.i + self.stride) & mask;
    }
}

// The maximum probe length for table operations.
//
// Estimating a load factor for the hash-table based on probe lengths allows
// the hash-table to avoid loading the length every insert, which is a source
// of contention.
pub fn limit(capacity: usize) -> usize {
    // 5 * log2(capacity): Testing shows this gives us a ~85% load factor.
    5 * ((usize::BITS as usize) - (capacity.leading_zeros() as usize) - 1)
}

// Returns an estimate of the number of entries needed to hold `capacity` elements.
pub fn entries_for(capacity: usize) -> usize {
    // We should rarely resize before 75%.
    let capacity = capacity.checked_mul(8).expect("capacity overflow") / 6;
    capacity.next_power_of_two()
}
