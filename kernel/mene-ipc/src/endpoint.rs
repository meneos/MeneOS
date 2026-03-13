use crate::capability::Capability;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use axsync::Mutex;
use axtask::WaitQueue;

pub struct IpcPayload {
    pub message: Vec<u8>,
    pub capabilities: VecDeque<Capability>,
    pub sender_id: u64,
}

impl IpcPayload {
    pub fn new(message: Vec<u8>, sender_id: u64) -> Self {
        Self {
            message,
            capabilities: VecDeque::new(),
            sender_id,
        }
    }
}

pub struct Endpoint {
    pub queue: Mutex<VecDeque<IpcPayload>>,
    pub wq: WaitQueue,
}

impl Endpoint {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            wq: WaitQueue::new(),
        }
    }

    pub fn push(&self, payload: IpcPayload) {
        self.queue.lock().push_back(payload);
        self.wq.notify_one(true);
    }

    pub fn pop(&self) -> Option<IpcPayload> {
        self.queue.lock().pop_front()
    }
}
