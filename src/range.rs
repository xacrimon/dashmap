pub struct CircularRange {
    wrap: usize,
    stop: usize,
    next: usize,
}

impl CircularRange {
    pub fn new(start: usize, wrap: usize) -> Self {
        Self {
            wrap,
            stop: start,
            next: start,
        }
    }
}

impl Iterator for CircularRange {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let n = self.next;
        self.next += 1;

        if self.next == self.wrap {
            self.next = 0;
        } else if self.next == self.stop {
            return None;
        }

        Some(n)
    }
}
