#![no_std]
#![feature(bstr)]

extern crate alloc;
#[macro_use]
extern crate log;

pub fn microkernel_init() {
    info!("MeneOS Microkernel starting...");
    // Call the loader
    mene_syscall::spawn_app("/boot/init");
}
