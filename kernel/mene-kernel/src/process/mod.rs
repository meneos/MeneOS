use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use axmm::AddrSpace;
use axsync::Mutex;
use mene_ipc::capability::Capability;

mod lifecycle;
pub use lifecycle::{ProcessState, ProcessSupervisor};

lazy_static::lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<BTreeMap<usize, ProcessInfo>> = Mutex::new(BTreeMap::new());
}

pub struct ProcessInfo {
    pub app_path: String,
    pub aspace: Arc<Mutex<AddrSpace>>,
    pub cspace: Mutex<BTreeMap<usize, Capability>>,
    pub local_endpoint: Arc<mene_ipc::endpoint::Endpoint>,
}

pub fn generate_pid() -> usize {
    ProcessSupervisor::allocate_pid()
}
