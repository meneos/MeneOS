#![no_std]

extern crate alloc;

pub mod ipc;
pub mod memory;
pub mod process;
pub mod trap;
pub mod device;

// Re-exports for convenience
pub use ipc::IpcManager;
pub use process::ProcessManager;
