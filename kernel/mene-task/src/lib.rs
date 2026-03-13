#![no_std]

extern crate alloc;
extern crate axlog;

pub mod task;
pub mod loader;

pub use task::{spawn_task, SyscallHandler};
