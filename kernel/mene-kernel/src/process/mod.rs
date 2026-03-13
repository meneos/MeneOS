use core::sync::atomic::{AtomicUsize, Ordering};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use axsync::Mutex;
use axmm::AddrSpace;

lazy_static::lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<BTreeMap<usize, ProcessInfo>> = Mutex::new(BTreeMap::new());
}

pub struct ProcessInfo {
    pub aspace: Arc<Mutex<AddrSpace>>,
}

pub struct ProcessManager;

static NEXT_PID: AtomicUsize = AtomicUsize::new(1); // PID allocations

pub fn generate_pid() -> usize {
    NEXT_PID.fetch_add(1, Ordering::SeqCst)
}
