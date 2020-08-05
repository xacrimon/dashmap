pub struct CircularRange {
    wrap: usize,
    stop: usize,
    next: usize,
    first: bool,
}

impl CircularRange {
    pub fn new(start: usize, wrap: usize) -> Self {
        Self {
            wrap,
            stop: start + 1,
            next: start,
            first: true,
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
        } else if self.next == self.stop && !self.first {
            return None;
        }

        self.first = false;
        Some(n)
    }
}

#[cfg(test)]
mod tests {
    use super::CircularRange;

    #[test]
    fn check_set() {
        let set: Vec<usize> = CircularRange::new(5, 10).collect();
        let expected = [5, 6, 7, 8, 9, 0, 1, 2, 3, 4];

        assert_eq!(&set, &expected);
    }
}
