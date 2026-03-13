#![no_std]
#![feature(bstr)]

extern crate alloc;
#[macro_use]
extern crate log;

pub mod mm;

pub fn microkernel_init() {
    info!("MeneOS Microkernel starting...");
    info!("Loading /boot/init from FAT32 Disk...");

    // In a microkernel, we mount the initial FS temporarily just to read the boot server.
    // For now we assume ArceOS handles axfs initialization through the axfs configuration.
    
    // Call the loader
    mm::loader::load_init_user();
}
