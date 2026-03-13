#![no_std]
#![feature(bstr)]

extern crate alloc;
#[macro_use]
extern crate log;

pub mod mm;

pub fn microkernel_init() {
    info!("MeneOS Microkernel starting...");
    // Call the loader
    mm::loader::load_init_user();
}
