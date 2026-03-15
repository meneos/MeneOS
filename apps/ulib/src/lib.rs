#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use buddy_system_allocator::LockedHeap;
use core::arch::asm;
use core::panic::PanicInfo;
use mene_abi::{MeneSysno, Sysno};
pub use mene_abi::blk;
pub use mene_abi::control;
pub use mene_abi::fs;
pub use mene_abi::Handle;

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
    // Send IPC to logger/serial capability
    sys_ipc_send(Handle::SerialEndpoint, msg.as_bytes(), None);
}

pub fn sys_spawn(path: &str) -> usize {
    syscall(
        MeneSysno::Spawn as usize,
        path.as_ptr() as usize,
        path.len(),
        0,
        0,
    )
}

pub fn sys_spawn_elf(path: &str, elf: &[u8]) -> usize {
    syscall(
        MeneSysno::SpawnElf as usize,
        path.as_ptr() as usize,
        path.len(),
        elf.as_ptr() as usize,
        elf.len(),
    )
}

pub fn sys_read_file(path: &str, buf: &mut [u8]) -> usize {
    syscall(
        MeneSysno::ReadFile as usize,
        path.as_ptr() as usize,
        path.len(),
        buf.as_mut_ptr() as usize,
        buf.len(),
    )
}

pub fn sys_get_boot_cfg(buf: &mut [u8]) -> usize {
    syscall(
        MeneSysno::GetBootCfg as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
        0,
        0,
    )
}

pub fn sys_ipc_send(handle: Handle, msg: &[u8], passed_cap: Option<Handle>) {
    let passed_cap_usize = passed_cap.map_or(0, |h| h.to_usize());
    let _ = syscall(
        MeneSysno::IpcSend as usize,
        handle.to_usize(),
        msg.as_ptr() as usize,
        msg.len(),
        passed_cap_usize,
    );
}

pub fn sys_ipc_send_checked(handle: Handle, msg: &[u8], passed_cap: Option<Handle>) -> bool {
    let passed_cap_usize = passed_cap.map_or(0, |h| h.to_usize());
    let ret = syscall(
        MeneSysno::IpcSend as usize,
        handle.to_usize(),
        msg.as_ptr() as usize,
        msg.len(),
        passed_cap_usize,
    );
    (ret as isize) >= 0
}

pub fn sys_ipc_recv(from_pid: &mut usize, buf: &mut [u8], recv_cap: &mut Option<Handle>) -> usize {
    let mut sender_pid = 0;
    let mut recv_usize = 0;
    let res = syscall(
        MeneSysno::IpcRecv as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
        &mut sender_pid as *mut usize as usize,
        &mut recv_usize as *mut usize as usize,
    );
    *from_pid = sender_pid;
    if recv_usize != 0 {
        *recv_cap = Some(Handle::from_usize(recv_usize));
    } else {
        *recv_cap = None;
    }
    res
}

pub fn sys_ipc_recv_timeout(
    from_pid: &mut usize,
    buf: &mut [u8],
    recv_cap: &mut Option<Handle>,
    timeout_ms: usize,
) -> isize {
    let mut meta = [0usize; 2];
    *from_pid = 0;
    *recv_cap = None;
    let res = syscall(
        MeneSysno::IpcRecvTimeout as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
        meta.as_mut_ptr() as usize,
        timeout_ms,
    ) as isize;

    if res >= 0 && (res as usize) <= buf.len() && (res == 0 || meta[0] != 0) {
        *from_pid = meta[0];
        if meta[1] != 0 {
            *recv_cap = Some(Handle::from_usize(meta[1]));
        } else {
            *recv_cap = None;
        }
        return res;
    }

    // Kernel-side errno encoding may be platform-dependent. If the return value is
    // out of message length range, treat it as an error and do not consume stale data.
    -1
}

pub fn sys_exit(code: i32) -> ! {
    syscall(Sysno::exit as usize, code as usize, 0, 0, 0);
    loop {}
}

pub fn sys_mmap(length: usize) -> usize {
    syscall(MeneSysno::MmapAnon as usize, length, 0, 0, 0)
}

pub fn sys_map_device(paddr: usize, length: usize) -> usize {
    syscall(MeneSysno::MapDevice as usize, paddr, length, 0, 0)
}

pub fn sys_vmm_map_page_to(target_pid: usize, vaddr: usize, size: usize) -> usize {
    syscall(MeneSysno::VmmMapPageTo as usize, target_pid, vaddr, size, 0)
}

pub fn sys_dma_alloc(length: usize, paddr_out: &mut usize) -> usize {
    syscall(
        MeneSysno::DmaAlloc as usize,
        length,
        paddr_out as *mut usize as usize,
        0,
        0,
    )
}

pub fn sys_dma_dealloc(vaddr: usize, paddr: usize, pages: usize) -> usize {
    syscall(MeneSysno::DmaDealloc as usize, vaddr, paddr, pages, 0)
}

pub fn sys_virt_to_phys(vaddr: usize) -> usize {
    syscall(MeneSysno::VirtToPhys as usize, vaddr, 0, 0, 0)
}

pub fn sys_pci_cfg_read(bus: usize, device: usize, function: usize, offset: usize) -> usize {
    syscall(
        MeneSysno::PciCfgRead as usize,
        bus,
        device,
        function,
        offset,
    )
}

pub fn sys_sleep_ms(ms: usize) {
    let _ = syscall(MeneSysno::SleepMs as usize, ms, 0, 0, 0);
}

pub fn sys_uptime_ms() -> usize {
    syscall(MeneSysno::UptimeMs as usize, 0, 0, 0, 0)
}

pub fn sys_system_off() -> ! {
    let _ = syscall(MeneSysno::SystemOff as usize, 0, 0, 0, 0);
    loop {}
}

pub fn ctl_register_service(name: &str) -> bool {
    let n = name.as_bytes();
    if n.is_empty() || n.len() > control::MAX_SERVICE_NAME {
        return false;
    }

    let mut req = [0u8; control::HDR_LEN + control::MAX_SERVICE_NAME];
    req[0..2].copy_from_slice(&control::REQ_REGISTER_SERVICE.to_le_bytes());
    req[2..4].copy_from_slice(&(n.len() as u16).to_le_bytes());
    req[4..4 + n.len()].copy_from_slice(n);

    // Pass our local endpoint capability to the registry.
    sys_ipc_send_checked(
        Handle::InitEndpoint,
        &req[..control::HDR_LEN + n.len()],
        Some(Handle::LocalEndpoint),
    )
}

pub fn ctl_lookup_service(name: &str, timeout_ms: usize) -> Option<Handle> {
    let n = name.as_bytes();
    if n.is_empty() || n.len() > control::MAX_SERVICE_NAME {
        return None;
    }

    let mut req = [0u8; control::HDR_LEN + control::MAX_SERVICE_NAME];
    req[0..2].copy_from_slice(&control::REQ_LOOKUP_SERVICE.to_le_bytes());
    req[2..4].copy_from_slice(&(n.len() as u16).to_le_bytes());
    req[4..4 + n.len()].copy_from_slice(n);

    // Pass our local endpoint as reply capability.
    if !sys_ipc_send_checked(
        Handle::InitEndpoint,
        &req[..control::HDR_LEN + n.len()],
        Some(Handle::LocalEndpoint),
    ) {
        return None;
    }

    let mut from_pid = 0usize;
    let mut recv_cap = None;
    let mut resp = [0u8; 16];
    let n = sys_ipc_recv_timeout(&mut from_pid, &mut resp, &mut recv_cap, timeout_ms);
    if n < 0 {
        return None;
    }
    if (n as usize) >= 2 && &resp[..2] == b"OK" {
        return recv_cap;
    }
    None
}

pub fn ctl_device_query_usize(key: &str, timeout_ms: usize) -> Option<usize> {
    let n = key.as_bytes();
    if n.is_empty() || n.len() > control::MAX_SERVICE_NAME {
        return None;
    }

    let mut req = [0u8; control::HDR_LEN + control::MAX_SERVICE_NAME];
    req[0..2].copy_from_slice(&control::REQ_DEVICE_QUERY.to_le_bytes());
    req[2..4].copy_from_slice(&(n.len() as u16).to_le_bytes());
    req[4..4 + n.len()].copy_from_slice(n);

    if !sys_ipc_send_checked(
        Handle::InitEndpoint,
        &req[..control::HDR_LEN + n.len()],
        Some(Handle::LocalEndpoint),
    ) {
        return None;
    }

    let mut from_pid = 0usize;
    let mut recv_cap = None;
    let mut resp = [0u8; 16];
    let n = sys_ipc_recv_timeout(&mut from_pid, &mut resp, &mut recv_cap, timeout_ms);
    if n < 0 {
        return None;
    }
    let n = n as usize;
    if n < 10 || &resp[..2] != b"OK" {
        return None;
    }
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&resp[2..10]);
    Some(u64::from_le_bytes(raw) as usize)
}

pub fn init_allocator() {
    let heap_size = 4 * 1024 * 1024; // 4MB
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
