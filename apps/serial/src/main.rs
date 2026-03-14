#![no_std]
#![no_main]

use core::ptr::write_volatile;

// QEMU virt aarch64 PL011 UART MMIO address
const UART0: usize = 0x0900_0000;

fn pl011_putchar(c: u8) {
    let ptr = UART0 as *mut u8;
    unsafe {
        write_volatile(ptr, c);
    }
}

fn print_str(s: &str) {
    for byte in s.bytes() {
        pl011_putchar(byte);
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::sys_map_device(UART0, 0x1000);
    print_str("serial: PL011 UART driver initialized.\n");
    let mut buf = [0u8; 128];
    let mut from_pid = 0usize;
    loop {
        let mut recv_cap = None;
        let len = ulib::sys_ipc_recv(&mut from_pid, &mut buf, &mut recv_cap);
        if len > 0 {
            if let Ok(msg) = core::str::from_utf8(&buf[..len]) {
                print_str(msg);
                print_str("\n");
            }
        }
    }
}
