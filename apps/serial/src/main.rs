#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::sys_log("serial: Serial service listening for IPC...");
    let mut buf = [0u8; 128];
    loop {
        let len = ulib::sys_ipc_recv(&mut buf);
        if len > 0 {
            if let Ok(msg) = core::str::from_utf8(&buf[..len]) {
                ulib::sys_log("
==========================");
                ulib::sys_log("[SERIAL OUTPUT]");
                ulib::sys_log(msg);
                ulib::sys_log("==========================
");
            }
        }
    }
}
