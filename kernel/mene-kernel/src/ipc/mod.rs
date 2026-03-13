use axerrno::{AxError, AxResult};

use crate::process::PROCESS_TABLE;
use mene_ipc::capability::Capability;

pub struct IpcManager;

impl IpcManager {
    pub fn init_process(_pid: usize) {
        // Initialization is handled when creating ProcessInfo
    }

    pub fn cleanup_process(_pid: usize) {
        // Handled by ProcessInfo drop
    }

    pub fn send(sender_pid: usize, handle: usize, msg: &[u8], passed_cap: usize) -> AxResult<()> {
        let ep = {
            let ptable = PROCESS_TABLE.lock();
            let process = ptable.get(&sender_pid).ok_or(AxError::NoSuchProcess)?;
            let cspace = process.cspace.lock();
            match cspace.get(&handle) {
                Some(Capability::Endpoint(ep)) => ep.clone(),
                _ => return Err(AxError::BadFileDescriptor), // invalid handle
            }
        };

        let mut payload = mene_ipc::endpoint::IpcPayload::new(msg.to_vec(), sender_pid as u64);

        if passed_cap != 0 {
            let ptable = PROCESS_TABLE.lock();
            let p = ptable.get(&sender_pid).ok_or(AxError::NoSuchProcess)?;
            let cspace = p.cspace.lock();
            if let Some(cap) = cspace.get(&passed_cap) {
                payload.capabilities.push_back(cap.clone());
            } else {
                return Err(AxError::BadFileDescriptor); // invalid passed_cap
            }
        }

        ep.push(payload);
        Ok(())
    }

    pub fn recv(
        current_pid: usize,
        buf: &mut [u8],
        from_pid: &mut usize,
        recv_cap: &mut usize,
    ) -> AxResult<usize> {
        let ep = {
            let ptable = PROCESS_TABLE.lock();
            let process = ptable.get(&current_pid).ok_or(AxError::NoSuchProcess)?;
            process.local_endpoint.clone()
        };

        axlog::info!("PID {} waiting for IPC...", current_pid);
        let mut payload = loop {
            if let Some(p) = ep.pop() {
                break p;
            }
            ep.wq.wait();
        };
        axlog::info!("PID {} Woke up!", current_pid);

        let msg = payload.message;
        let bytes_to_copy = core::cmp::min(msg.len(), buf.len());
        buf[..bytes_to_copy].copy_from_slice(&msg[..bytes_to_copy]);
        *from_pid = payload.sender_id as usize;

        if let Some(cap) = payload.capabilities.pop_front() {
            let ptable = PROCESS_TABLE.lock();
            if let Some(p) = ptable.get(&current_pid) {
                let mut cspace = p.cspace.lock();
                let mut new_handle = 10;
                while cspace.contains_key(&new_handle) {
                    new_handle += 1;
                }
                cspace.insert(new_handle, cap);
                *recv_cap = new_handle;
            } else {
                *recv_cap = 0;
            }
        } else {
            *recv_cap = 0;
        }

        Ok(bytes_to_copy)
    }
}
