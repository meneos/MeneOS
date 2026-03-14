use axerrno::{AxError, AxResult};
use alloc::sync::Arc;

use crate::process::PROCESS_TABLE;
use mene_ipc::capability::Capability;
use mene_ipc::endpoint::Endpoint;

fn service_path_by_handle(handle: usize) -> Option<&'static str> {
    match handle {
        2 => Some("/boot/serial"),
        3 => Some("/boot/vmm"),
        4 => Some("/boot/virtio_blk"),
        5 => Some("/boot/fs"),
        _ => None,
    }
}

fn resolve_system_endpoint(
    handle: usize,
    ptable: &alloc::collections::BTreeMap<usize, crate::process::ProcessInfo>,
) -> Option<Arc<Endpoint>> {
    let path = service_path_by_handle(handle)?;
    ptable
        .values()
        .find(|p| p.app_path == path)
        .map(|p| p.local_endpoint.clone())
}

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
                _ => {
                    if let Some(ep) = resolve_system_endpoint(handle, &ptable) {
                        axlog::warn!(
                            "ipc: dynamic handle {} resolved for sender pid {}",
                            handle,
                            sender_pid
                        );
                        ep
                    } else {
                        axlog::warn!(
                            "ipc: dynamic handle {} unresolved for sender pid {}",
                            handle,
                            sender_pid
                        );
                        return Err(AxError::BadFileDescriptor);
                    }
                }
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

        ep.wq.wait_until(|| ep.has_pending());
        let mut payload = loop {
            if let Some(p) = ep.pop() {
                break p;
            }
            ep.wq.wait_until(|| ep.has_pending());
        };

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
