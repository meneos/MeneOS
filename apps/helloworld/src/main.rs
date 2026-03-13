#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;

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
    
    ulib::sys_log("helloworld: IPC sent. Exiting.");
    ulib::sys_exit(0);
}
