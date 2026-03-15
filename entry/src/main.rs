#![no_std]
#![no_main]

extern crate alloc;
extern crate axruntime;

#[unsafe(no_mangle)]
fn main() {
    // start mene-init
    mene_init::microkernel_init();

    // After this, axtask will schedule the user thread.
    loop {
        axtask::yield_now();
    }
}
