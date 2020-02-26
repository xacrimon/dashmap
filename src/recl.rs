use std::sync::Mutex;

type GarbageList = Vec<Box<dyn FnOnce()>>;

pub struct Gc {
    state: Mutex<GcState>,
}

struct GcState {
    counter: usize,
    lists: [GarbageList; 3],
}
