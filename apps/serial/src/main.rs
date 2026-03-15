#![no_std]
#![no_main]

use core::ptr::write_volatile;

// QEMU virt aarch64 PL011 UART MMIO address
const UART0: usize = 0x0900_0000;
const IPC_BUF_CAP: usize = 4096;

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

fn print_usize(mut n: usize) {
    let mut buf = [0u8; 20];
    if n == 0 {
        pl011_putchar(b'0');
        return;
    }

    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    while i > 0 {
        i -= 1;
        pl011_putchar(buf[i]);
    }
}

fn print_prefix(now_ms: usize) {
    let secs = now_ms / 1000;
    let millis = now_ms % 1000;

    print_str("[");
    print_usize(secs);
    print_str(".");
    pl011_putchar(b'0' + ((millis / 100) % 10) as u8);
    pl011_putchar(b'0' + ((millis / 10) % 10) as u8);
    pl011_putchar(b'0' + (millis % 10) as u8);
    print_str("] ");
}

fn print_record(now_ms: usize, msg: &[u8]) {
    print_prefix(now_ms);
    for &b in msg {
        // Keep one-line output per IPC record for readability.
        if b == b'\n' || b == b'\r' {
            pl011_putchar(b' ');
        } else {
            pl011_putchar(b);
        }
    }
    pl011_putchar(b'\n');
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::sys_map_device(UART0, 0x1000);
    print_record(ulib::sys_uptime_ms(), b"PL011 UART driver initialized.");
    let _ = ulib::ctl_register_service("serial");
    let mut buf = [0u8; IPC_BUF_CAP];
    let mut from_pid = 0usize;
    loop {
        let mut recv_cap = None;
        let len = ulib::sys_ipc_recv_timeout(&mut from_pid, &mut buf, &mut recv_cap, 50);
        if len <= 0 {
            continue;
        }

        let len = len as usize;
        let now_ms = ulib::sys_uptime_ms();
        print_record(now_ms, &buf[..len]);
    }
}
