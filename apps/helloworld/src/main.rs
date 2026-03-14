#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use ulib::fs;

const FS_RETRY_MAX: usize = 20;
const FS_RETRY_SLEEP_MS: usize = 20;

fn fs_call(req: &[u8], resp: &mut [u8]) -> usize {
    if !ulib::sys_ipc_send_checked(
        ulib::Handle::FsEndpoint,
        req,
        Some(ulib::Handle::LocalEndpoint),
    ) {
        return 0;
    }

    let mut from_pid = 0usize;
    let mut recv_cap = None;
    ulib::sys_ipc_recv(&mut from_pid, resp, &mut recv_cap)
}

fn fs_call_retry(req: &[u8], resp: &mut [u8]) -> usize {
    let mut i = 0;
    while i < FS_RETRY_MAX {
        let len = fs_call(req, resp);
        if !(len == 6 && &resp[..6] == b"EAGAIN") {
            return len;
        }
        ulib::sys_sleep_ms(FS_RETRY_SLEEP_MS);
        i += 1;
    }
    0
}

fn fs_write(path: &str, data: &[u8], resp: &mut [u8]) -> bool {
    let p = path.as_bytes();
    let mut req = Vec::with_capacity(fs::WRITE_HDR_LEN + p.len() + data.len());
    req.extend_from_slice(&fs::REQ_WRITE.to_le_bytes());
    req.extend_from_slice(&(p.len() as u16).to_le_bytes());
    req.extend_from_slice(&(data.len() as u32).to_le_bytes());
    req.extend_from_slice(p);
    req.extend_from_slice(data);
    let len = fs_call_retry(&req, resp);
    len == 2 && &resp[..2] == b"OK"
}

fn fs_read(path: &str, resp: &mut [u8]) -> usize {
    let p = path.as_bytes();
    let mut req = Vec::with_capacity(fs::PATH_HDR_LEN + p.len());
    req.extend_from_slice(&fs::REQ_READ.to_le_bytes());
    req.extend_from_slice(&(p.len() as u16).to_le_bytes());
    req.extend_from_slice(p);
    fs_call_retry(&req, resp)
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::init_allocator();
    
    ulib::sys_log("helloworld: Service started, sending IPC to serial...");
    
    // Testing dynamic allocation
    let mut vec = Vec::new();
    vec.push(b'H');
    vec.push(b'e');
    vec.push(b'l');
    vec.push(b'l');
    vec.push(b'o');
    vec.push(b' ');
    vec.push(b'W');
    vec.push(b'o');
    vec.push(b'r');
    vec.push(b'l');
    vec.push(b'd');
    vec.push(b' ');
    vec.push(b'f');
    vec.push(b'r');
    vec.push(b'o');
    vec.push(b'm');
    vec.push(b' ');
    vec.push(b'm');
    vec.push(b'm');
    vec.push(b'a');
    vec.push(b'p');
    vec.push(b' ');
    vec.push(b'V');
    vec.push(b'e');
    vec.push(b'c');
    vec.push(b'!');
    vec.push(b'!');
    
    // Send IPC to serial service (PID 2 based on startup sequence)
    ulib::sys_ipc_send(ulib::Handle::SerialEndpoint, &vec, None);

    // fs write/read via user-space fs service.
    let path = "hello.txt";
    let content = b"mene-fs-content";
    let mut fs_resp = [0u8; 256];

    ulib::sys_log("helloworld: fs write begin");
    let write_ok = fs_write(path, content, &mut fs_resp);
    ulib::sys_log("helloworld: fs write done");

    ulib::sys_log("helloworld: fs read begin");
    let rd = fs_read(path, &mut fs_resp);
    let read_ok = rd == content.len() && &fs_resp[..rd] == content;
    ulib::sys_log("helloworld: fs read done");

    if write_ok && read_ok {
        ulib::sys_log("helloworld: fs write/read test passed");
    } else {
        ulib::sys_log("helloworld: fs write/read test failed");
    }
    
    ulib::sys_log("helloworld: IPC sent. Powering off in 2s...");
    ulib::sys_sleep_ms(2000);
    ulib::sys_system_off();
}
