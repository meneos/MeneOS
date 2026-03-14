#![no_std]

extern crate alloc;
extern crate axlog;

pub mod loader;
pub mod task;

pub use task::{
	SyscallHandler,
	copy_boot_cfg_to,
	preload_boot_assets,
	spawn_task,
	spawn_task_from_bytes,
};
