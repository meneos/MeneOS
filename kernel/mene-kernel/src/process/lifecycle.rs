use alloc::collections::BTreeMap;
use axsync::Mutex;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::service::{ServiceRegistry, ServiceHandle, RegistryError};

static NEXT_PID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Created,
    Ready,
    Running,
    Exited,
    Faulted,
}

pub struct ProcessLifecycle {
    pid: usize,
    state: ProcessState,
    exit_code: Option<i32>,
}

impl ProcessLifecycle {
    fn new(pid: usize) -> Self {
        Self {
            pid,
            state: ProcessState::Created,
            exit_code: None,
        }
    }

    pub fn pid(&self) -> usize {
        self.pid
    }

    pub fn state(&self) -> ProcessState {
        self.state
    }

    pub fn transition_to(&mut self, new_state: ProcessState) {
        self.state = new_state;
    }

    pub fn mark_exited(&mut self, code: i32) {
        self.state = ProcessState::Exited;
        self.exit_code = Some(code);
    }

    pub fn mark_faulted(&mut self) {
        self.state = ProcessState::Faulted;
    }
}

lazy_static::lazy_static! {
    static ref LIFECYCLE_TABLE: Mutex<BTreeMap<usize, ProcessLifecycle>> = Mutex::new(BTreeMap::new());
    static ref SERVICE_REGISTRY: Mutex<ServiceRegistry> = Mutex::new(ServiceRegistry::new());
}

pub struct ProcessSupervisor;

impl ProcessSupervisor {
    pub fn allocate_pid() -> usize {
        NEXT_PID.fetch_add(1, Ordering::SeqCst)
    }

    pub fn register_process(pid: usize) {
        let lifecycle = ProcessLifecycle::new(pid);
        LIFECYCLE_TABLE.lock().insert(pid, lifecycle);
    }

    pub fn transition_state(pid: usize, new_state: ProcessState) {
        if let Some(lifecycle) = LIFECYCLE_TABLE.lock().get_mut(&pid) {
            lifecycle.transition_to(new_state);
        }
    }

    pub fn mark_process_exited(pid: usize, exit_code: i32) {
        if let Some(lifecycle) = LIFECYCLE_TABLE.lock().get_mut(&pid) {
            lifecycle.mark_exited(exit_code);
        }
    }

    pub fn mark_process_faulted(pid: usize) {
        if let Some(lifecycle) = LIFECYCLE_TABLE.lock().get_mut(&pid) {
            lifecycle.mark_faulted();
        }
    }

    pub fn get_state(pid: usize) -> Option<ProcessState> {
        LIFECYCLE_TABLE.lock().get(&pid).map(|l| l.state())
    }

    pub fn cleanup_process(pid: usize) {
        LIFECYCLE_TABLE.lock().remove(&pid);
    }

    pub fn register_service(name: &[u8], pid: usize, handle: ServiceHandle) -> Result<(), RegistryError> {
        SERVICE_REGISTRY.lock().register(name, pid, handle)
    }

    pub fn lookup_service(name: &[u8]) -> Result<ServiceHandle, RegistryError> {
        SERVICE_REGISTRY.lock().lookup(name)
    }

    pub fn unregister_service(name: &[u8]) -> Result<(), RegistryError> {
        SERVICE_REGISTRY.lock().unregister(name)
    }
}
