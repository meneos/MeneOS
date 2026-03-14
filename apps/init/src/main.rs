#![no_std]
#![no_main]

use ulib::fs;

fn fs_exec(path: &str) -> bool {
    let p = path.as_bytes();
    if p.is_empty() || p.len() > fs::MAX_PATH {
        return false;
    }

    let mut req = [0u8; fs::PATH_HDR_LEN + fs::MAX_PATH];
    req[0..2].copy_from_slice(&fs::REQ_EXEC.to_le_bytes());
    req[2..4].copy_from_slice(&(p.len() as u16).to_le_bytes());
    req[4..4 + p.len()].copy_from_slice(p);

    if !ulib::sys_ipc_send_checked(
        ulib::Handle::FsEndpoint,
        &req[..4 + p.len()],
        Some(ulib::Handle::LocalEndpoint),
    ) {
        return false;
    }

    let mut resp = [0u8; 16];
    let mut from_pid = 0usize;
    let mut recv_cap = None;
    let len = ulib::sys_ipc_recv(&mut from_pid, &mut resp, &mut recv_cap);
    len == 2 && &resp[..2] == b"OK"
}

fn wait_fs_ready() -> bool {
    let ping = fs::REQ_PING.to_le_bytes();
    let mut resp = [0u8; 8];
    let mut from_pid = 0usize;
    let mut recv_cap = None;

    let mut i = 0;
    while i < 80 {
        if ulib::sys_ipc_send_checked(
            ulib::Handle::FsEndpoint,
            &ping,
            Some(ulib::Handle::LocalEndpoint),
        ) {
            let len = ulib::sys_ipc_recv(&mut from_pid, &mut resp, &mut recv_cap);
            if len == 4 && &resp[..4] == b"PONG" {
                return true;
            }
        }
        ulib::sys_sleep_ms(10);
        i += 1;
    }
    false
}

fn fs_exec_retry(path: &str, retries: usize) -> bool {
    let mut i = 0;
    while i < retries {
        if fs_exec(path) {
            return true;
        }
        ulib::sys_sleep_ms(10);
        i += 1;
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    BootPre,
    BootPost,
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::sys_log("init: Microkernel init service started.");

    let mut buf = [0u8; 512];
    let len = ulib::sys_get_boot_cfg(&mut buf);
    if len > 0 {
        if let Ok(config) = core::str::from_utf8(&buf[..len]) {
            let mut section = Section::None;
            let mut fs_started = false;

            // Phase 1: spawn boot_pre entries strictly in listed order.
            for line in config.split('\n') {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "[boot_pre]" {
                    section = Section::BootPre;
                    continue;
                }
                if line == "[boot_post]" {
                    section = Section::BootPost;
                    continue;
                }
                if section != Section::BootPre {
                    continue;
                }

                let path = line.trim_end_matches('\0');
                if path.is_empty() {
                    continue;
                }

                let _ = ulib::sys_spawn(path);
                if path == "/boot/fs" {
                    fs_started = true;
                }
            }

            if fs_started && !wait_fs_ready() {
                ulib::sys_log("init: fs not ready in bootstrap window");
            }

            section = Section::None;

            // Phase 2: execute boot_post entries from filesystem in listed order.
            for line in config.split('\n') {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "[boot_pre]" {
                    section = Section::BootPre;
                    continue;
                }
                if line == "[boot_post]" {
                    section = Section::BootPost;
                    continue;
                }
                if section != Section::BootPost {
                    continue;
                }

                let path = line.trim_end_matches('\0');
                if path.is_empty() {
                    continue;
                }

                if !fs_exec_retry(path, 20) {
                    ulib::sys_log("init: fs exec failed");
                }
            }
        } else {
            ulib::sys_log("init: Failed to parse preloaded boot cfg");
        }
    } else {
        ulib::sys_log("init: Failed to get preloaded boot cfg");
    }

    ulib::sys_log("init: Boot sequence completed.");
    ulib::sys_exit(0);
}
