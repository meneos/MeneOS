#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::sys_log("vmm: Memory management service started.");

    let mut buf = [0u8; 128];
    let mut from_pid = 0usize;

    loop {
        let mut recv_cap = None;
        let len = ulib::sys_ipc_recv(&mut from_pid, &mut buf, &mut recv_cap);
        if len > 0 {
            if buf[0] == 1 && len == 9 {
                // Command 1 = MMAP, followed by 8 bytes of size
                let mut size_bytes = [0u8; 8];
                size_bytes.copy_from_slice(&buf[1..9]);
                let size = usize::from_le_bytes(size_bytes);

                ulib::sys_log("vmm: Received MMAP request");

                let vaddr = ulib::sys_vmm_map_page_to(from_pid, 0, size);

                // Send reply back
                let vaddr_bytes = vaddr.to_le_bytes();
                if let Some(target_cap) = recv_cap {
                    ulib::sys_ipc_send(target_cap, &vaddr_bytes, None);
                } else {
                    ulib::sys_log("vmm: no reply capability provided!");
                }
            }
        }
    }
}
