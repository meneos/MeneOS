use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use axsync::Mutex;
use crate::endpoint::{Endpoint, IpcPayload};
use axerrno::{AxResult, AxError};

static PROCESS_ENDPOINTS: Mutex<Option<BTreeMap<usize, Endpoint>>> = Mutex::new(None);

fn get_endpoints() -> &'static Mutex<Option<BTreeMap<usize, Endpoint>>> {
    &PROCESS_ENDPOINTS
}

pub struct IpcManager;

impl IpcManager {
    pub fn init_process(pid: usize) {
        let mut eps = get_endpoints().lock();
        if eps.is_none() {
            *eps = Some(BTreeMap::new());
        }
        if let Some(map) = eps.as_mut() {
            map.insert(pid, Endpoint::new());
        }
    }

    pub fn send(from_pid: usize, to_handle: usize, data: &[u8], _cap: usize) -> AxResult<()> {
        let payload = IpcPayload::new(data.to_vec(), from_pid as u64);

        let eps = get_endpoints().lock();
        if let Some(map) = eps.as_ref() {
            if let Some(ep) = map.get(&to_handle) {
                ep.push(payload);
                return Ok(());
            }
        }
        Err(AxError::NotFound)
    }

    pub fn recv(pid: usize, buf: &mut [u8], sender_pid: &mut usize, recv_cap: &mut usize) -> AxResult<usize> {
        let eps = get_endpoints().lock();
        if let Some(map) = eps.as_ref() {
            if let Some(ep) = map.get(&pid) {
                if let Some(payload) = ep.pop() {
                    let len = payload.message.len().min(buf.len());
                    buf[..len].copy_from_slice(&payload.message[..len]);
                    *sender_pid = payload.sender_id as usize;
                    *recv_cap = 0;
                    return Ok(len);
                }
            }
        }
        Err(AxError::WouldBlock)
    }

    pub fn recv_timeout(pid: usize, buf: &mut [u8], sender_pid: &mut usize, recv_cap: &mut usize, _timeout_ms: usize) -> AxResult<usize> {
        Self::recv(pid, buf, sender_pid, recv_cap)
    }

    pub fn cleanup_process(pid: usize) {
        let mut eps = get_endpoints().lock();
        if let Some(map) = eps.as_mut() {
            map.remove(&pid);
        }
    }
}
