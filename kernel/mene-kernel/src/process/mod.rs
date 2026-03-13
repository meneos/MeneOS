use core::sync::atomic::{AtomicUsize, Ordering};

pub struct ProcessManager;

static NEXT_PID: AtomicUsize = AtomicUsize::new(1); // PID allocations

pub fn generate_pid() -> usize {
    NEXT_PID.fetch_add(1, Ordering::SeqCst)
}
