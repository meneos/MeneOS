#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::arch::asm;
use core::panic::PanicInfo;
use buddy_system_allocator::LockedHeap;
use mene_abi::{Sysno, MeneSysno};

#[global_allocator]
static ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

pub fn syscall(sysno: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize) -> usize {
    let mut ret: usize;
    unsafe {
        asm!(
            "svc #0",
            in("x8") sysno,
            inout("x0") arg0 => ret,
            in("x1") arg1,
            in("x2") arg2,
            in("x3") arg3,
            options(nostack, preserves_flags)
        );
    }
    ret
}

pub fn sys_log(msg: &str) {
    // Send IPC to serial process (PID 2)
    sys_ipc_send(2, msg.as_bytes());
}

pub fn sys_spawn(path: &str) -> usize {
    syscall(MeneSysno::Spawn as usize, path.as_ptr() as usize, path.len(), 0, 0)
}

pub fn sys_read_file(path: &str, buf: &mut [u8]) -> usize {
    syscall(MeneSysno::ReadFile as usize, path.as_ptr() as usize, path.len(), buf.as_mut_ptr() as usize, buf.len())
}

pub fn sys_ipc_send(pid: usize, msg: &[u8]) {
    syscall(MeneSysno::IpcSend as usize, pid, msg.as_ptr() as usize, msg.len(), 0);
}

pub fn sys_ipc_recv(from_pid: &mut usize, buf: &mut [u8]) -> usize {
    syscall(MeneSysno::IpcRecv as usize, buf.as_mut_ptr() as usize, buf.len(), from_pid as *mut usize as usize, 0)
}

pub fn sys_exit(code: i32) -> ! {
    syscall(Sysno::exit as usize, code as usize, 0, 0, 0);
    loop {}
}

pub fn sys_mmap(length: usize) -> usize {
    let mut req = [0u8; 9];
    req[0] = 1; // Command 1: MMAP
    req[1..9].copy_from_slice(&length.to_le_bytes());
    sys_ipc_send(3, &req); // PID 3 is VMM
    
    let mut resp = [0u8; 8];
    let mut from_pid = 0;
    loop {
        let len = sys_ipc_recv(&mut from_pid, &mut resp);
        if len == 8 && from_pid == 3 {
            return usize::from_le_bytes(resp);
        }
    }
}

pub fn sys_map_device(paddr: usize, length: usize) -> usize {
    syscall(MeneSysno::MapDevice as usize, paddr, length, 0, 0)
}

pub fn sys_vmm_map_page_to(target_pid: usize, vaddr: usize, size: usize) -> usize {
    syscall(MeneSysno::VmmMapPageTo as usize, target_pid, vaddr, size, 0)
}

pub fn init_allocator() {
    let heap_size = 1024 * 1024; // 1MB
    let heap_start = sys_mmap(heap_size);
    if heap_start != !0 {
        unsafe {
            ALLOCATOR.lock().init(heap_start, heap_size);
        }
    } else {
        sys_log("Failed to initialize allocator: mmap failed");
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    sys_log("User panic!");
    sys_exit(-1);
}

#[alloc_error_handler]
fn alloc_error_handler(_layout: core::alloc::Layout) -> ! {
    sys_log("User allocation error!");
    sys_exit(-1);
}
