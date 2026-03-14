#![no_std]
#![feature(bstr)]

extern crate alloc;
#[macro_use]
extern crate log;

pub fn microkernel_init() {
    info!("MeneOS Microkernel starting...");
    mene_kernel::device::init_device_model();
    if !mene_syscall::preload_boot_assets() {
        warn!("boot preload failed; /boot/init may not be available");
    }
    // Call the loader
    mene_syscall::spawn_app("/boot/init");
}
