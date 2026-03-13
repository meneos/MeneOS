#![no_std]
use core::arch::asm;
use core::panic::PanicInfo;

pub fn syscall(sysno: usize, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let mut ret: usize;
    unsafe {
        asm!(
            "svc #0",
            in("x8") sysno as usize,
            inout("x0") arg0 => ret,
            in("x1") arg1,
            in("x2") arg2,
            options(nostack, preserves_flags)
        );
    }
    ret
}

pub fn sys_log(msg: &str) {
    syscall(1, msg.as_ptr() as usize, msg.len(), 0);
}

pub fn sys_spawn(path: &str) -> usize {
    syscall(2, path.as_ptr() as usize, path.len(), 0)
}

pub fn sys_read_file(path: &str, buf: &mut [u8]) -> usize {
    syscall(3, path.as_ptr() as usize, path.len(), buf.as_mut_ptr() as usize)
}

pub fn sys_ipc_send(pid: usize, msg: &[u8]) {
    syscall(4, pid, msg.as_ptr() as usize, msg.len());
}

pub fn sys_ipc_recv(buf: &mut [u8]) -> usize {
    syscall(5, buf.as_mut_ptr() as usize, buf.len(), 0)
}

pub fn sys_exit(code: i32) -> ! {
    syscall(6, code as usize, 0, 0);
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    sys_log("User panic!");
    sys_exit(-1);
}
