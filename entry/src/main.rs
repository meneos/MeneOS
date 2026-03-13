#![no_std]
#![no_main]

extern crate alloc;
extern crate axruntime;
#[macro_use]
extern crate axlog;

#[unsafe(no_mangle)]
fn main() {
    ax_println!("Starting MeneOS microkernel from entry...");

    // start mene-init
    mene_init::microkernel_init();

    // After this, axtask will schedule the user thread, we just loop here or exit
    ax_println!("Kernel initialization done, dropping to default task loop.");
    loop {
        axtask::yield_now();
    }
}
