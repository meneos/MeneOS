use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use axmm::AddrSpace;
use axsync::Mutex;
use core::sync::atomic::{AtomicUsize, Ordering};
use mene_ipc::capability::Capability;

lazy_static::lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<BTreeMap<usize, ProcessInfo>> = Mutex::new(BTreeMap::new());
}

pub struct ProcessInfo {
    pub aspace: Arc<Mutex<AddrSpace>>,
    pub cspace: Mutex<BTreeMap<usize, Capability>>,
    pub local_endpoint: Arc<mene_ipc::endpoint::Endpoint>,
}

pub struct ProcessManager;

static NEXT_PID: AtomicUsize = AtomicUsize::new(1); // PID allocations

pub fn generate_pid() -> usize {
    NEXT_PID.fetch_add(1, Ordering::SeqCst)
}
