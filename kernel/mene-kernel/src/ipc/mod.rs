use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use axtask::WaitQueue;
use axsync::Mutex;

lazy_static::lazy_static! {
    static ref IPC_MAILBOXES: Mutex<BTreeMap<usize, Arc<ProcessIpc>>> = Mutex::new(BTreeMap::new());
}

pub struct ProcessIpc {
    queue: Mutex<Vec<Vec<u8>>>,
    wq: WaitQueue,
}

impl ProcessIpc {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
            wq: WaitQueue::new(),
        }
    }
}

pub struct IpcManager;

impl IpcManager {
    pub fn init_process(pid: usize) {
        IPC_MAILBOXES.lock().insert(pid, Arc::new(ProcessIpc::new()));
    }
    
    pub fn cleanup_process(pid: usize) {
        IPC_MAILBOXES.lock().remove(&pid);
    }
    
    pub fn send(target_pid: usize, msg: &[u8]) -> isize {
        let map = IPC_MAILBOXES.lock();
        if let Some(ipc) = map.get(&target_pid) {
            ipc.queue.lock().push(msg.to_vec());
            ipc.wq.notify_one(true);
            0
        } else {
            -1 // Process not found
        }
    }
    
    pub fn recv(current_pid: usize, buf: &mut [u8]) -> usize {
        let ipc = {
            let map = IPC_MAILBOXES.lock();
            map.get(&current_pid).cloned()
        };
        
        if let Some(ipc) = ipc {
            axlog::info!("PID {} waiting for IPC...", current_pid);
            ipc.wq.wait_until(|| !ipc.queue.lock().is_empty());
            axlog::info!("PID {} Woke up!", current_pid);
            
            let mut q = ipc.queue.lock();
            let msg = q.remove(0);
            let bytes_to_copy = core::cmp::min(msg.len(), buf.len());
            buf[..bytes_to_copy].copy_from_slice(&msg[..bytes_to_copy]);
            
            bytes_to_copy
        } else {
            axlog::warn!("IPC recv failed, PID {} mailbox missing", current_pid);
            0
        }
    }
}