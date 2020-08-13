/// A very simple priority queue.
/// This is only ever accessed on thread setup and exit
/// and thus performance is mostly irrelevant.
pub struct PriorityQueue<T> {
    items: Vec<T>,
}

impl<T: Ord> PriorityQueue<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::with_capacity(8),
        }
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    /// Find the index of the least item and remove it from the list.
    pub fn pop(&mut self) -> Option<T> {
        (0..self.items.len())
            .min_by_key(|index| unsafe { self.items.get_unchecked(*index) })
            .map(|index| self.items.remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::PriorityQueue;

    #[test]
    fn pop_order() {
        let mut queue = PriorityQueue::new();
        queue.push(7);
        queue.push(9);
        queue.push(4);
        assert_eq!(queue.pop(), Some(4));
        assert_eq!(queue.pop(), Some(7));
        assert_eq!(queue.pop(), Some(9));
    }
}
