use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::marker::PhantomData;

pub struct TransactionLock(RwLock<()>);
pub struct TransactionReadGuard<'a>(RwLockReadGuard<'a, ()>);
pub struct FakeTransactionReadGuard<'a>(PhantomData<&'a ()>);
pub struct TransactionWriteGuard<'a>(RwLockWriteGuard<'a, ()>);

pub trait QueryAccessGuard: Sized {
    fn destroy(self) {}
}

impl<'a> QueryAccessGuard for TransactionReadGuard<'a> {}
impl<'a> QueryAccessGuard for FakeTransactionReadGuard<'a> {}
impl<'a> QueryAccessGuard for TransactionWriteGuard<'a> {}

impl TransactionLock {
    pub fn shared<'a>(&'a self) -> TransactionReadGuard<'a> {
        TransactionReadGuard(self.0.read())
    }

    pub fn unique<'a>(&'a self) -> TransactionWriteGuard<'a> {
        TransactionWriteGuard(self.0.write())
    }
}

impl<'a> FakeTransactionReadGuard<'a> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}
