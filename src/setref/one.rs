use crate::mapref;
use core::hash::Hash;
use core::ops::Deref;

pub struct Ref<'a, K> {
    inner: mapref::one::Ref<'a, K, ()>,
}

impl<'a, K: Eq + Hash> Ref<'a, K> {
    pub(crate) fn new(inner: mapref::one::Ref<'a, K, ()>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> &'a K {
        self.inner.key()
    }
}

impl<'a, K: Eq + Hash> Deref for Ref<'a, K> {
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
        fn get_key<'a>(data: &'a DashSet<String>) -> Option<&'a str> {
            data.get("key").map(|item| item.key().as_str())
        }
    }
}
