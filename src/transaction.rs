use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::rc::Rc;

pub struct TransactionLock(RwLock<()>);
pub struct TransactionReadGuard<'a>(RwLockReadGuard<'a, ()>);
pub struct TransactionWriteGuard<'a>(RwLockWriteGuard<'a, ()>);

pub enum QueryAccessGuard<'a> {
    Normal(Option<TransactionReadGuard<'a>>),
    Special(Option<Rc<TransactionWriteGuard<'a>>>),
}

impl<'a> QueryAccessGuard<'a> {
    pub fn try_clone(&self) -> Self {
        match self {
            QueryAccessGuard::Normal(_) => unreachable!(),
            QueryAccessGuard::Special(ref t) => QueryAccessGuard::Special(Some(t.as_ref().unwrap().clone()))
        }
    }
    
    pub fn destroy(&mut self) {
        match self {
            QueryAccessGuard::Normal(ref mut t) => drop(t.take()),
            QueryAccessGuard::Special(ref mut t) => drop(t.take()),
        }
    }
}


impl TransactionLock {
    pub fn new() -> Self {
        Self(RwLock::new(()))
    }

    pub fn shared<'a>(&'a self) -> QueryAccessGuard<'a> {
        QueryAccessGuard::Normal(Some(TransactionReadGuard(self.0.read())))
    }

    pub fn unique<'a>(&'a self) -> QueryAccessGuard<'a> {
        QueryAccessGuard::Special(Some(Rc::new(TransactionWriteGuard(self.0.write()))))
    }
}
