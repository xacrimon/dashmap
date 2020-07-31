pub struct CircularRange {
    end: usize,
    next: usize,
}

impl CircularRange {
    pub fn new(end: usize, next: usize) -> Self {
        Self { end, next }
    }
}

impl Iterator for CircularRange {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.next;
        self.next += 1;
        if self.next >= self.end {
            self.next &= self.end - 1;
        }
        Some(r)
    }
}
