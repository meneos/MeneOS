#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::sys_log("init: Microkernel init service started.");
    
    let mut buf = [0u8; 256];
    let len = ulib::sys_read_file("/boot/boot.cfg", &mut buf);
    if len > 0 {
        if let Ok(config) = core::str::from_utf8(&buf[..len]) {
            for line in config.split('\n') {
                let line = line.trim();
                let without_null = line.trim_end_matches('\0');
                if !without_null.is_empty() {
                    ulib::sys_spawn(without_null);
                }
            }
        } else {
            ulib::sys_log("init: Failed to parse boot.cfg");
        }
    } else {
        ulib::sys_log("init: Failed to read /boot/boot.cfg");
    }

    ulib::sys_log("init: Boot sequence completed.");
    ulib::sys_exit(0);
}
