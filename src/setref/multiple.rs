use crate::mapref;
use core::hash::Hash;
use core::ops::Deref;

pub struct RefMulti<'a, K> {
    inner: mapref::multiple::RefMulti<'a, K, ()>,
}

impl<'a, K: Eq + Hash> RefMulti<'a, K> {
    pub(crate) fn new(inner: mapref::multiple::RefMulti<'a, K, ()>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> &'a K {
        self.inner.key()
    }
}

impl<'a, K: Eq + Hash> Deref for RefMulti<'a, K> {
    type Target = K;

    fn deref(&self) -> &'a K {
        self.key()
    }
}

#[cfg(test)]
mod tests {
    use crate::DashSet;

    #[test]
    #[allow(unused)]
    fn lifetime() {
        fn get_key<'a>(data: &'a DashSet<String>) -> impl Iterator<Item = &'a str> {
            data.iter().map(|item| item.key().as_str())
        }
    }
}
