#![no_std]
extern crate alloc;
pub mod capability;
pub mod endpoint;
pub mod manager;

pub use manager::IpcManager;
