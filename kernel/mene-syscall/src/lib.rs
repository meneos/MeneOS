#![no_std]
#![feature(bstr)]

extern crate alloc;
#[macro_use]
extern crate axlog;

use alloc::sync::Arc;
use axhal::uspace::UserContext;
use axsync::Mutex;
use core::time::Duration;
use mene_abi::{Handle, MeneSysno, Sysno};

pub fn preload_boot_assets() -> bool {
    mene_task::preload_boot_assets()
}

pub fn spawn_app(path: &str) -> usize {
    let pid = mene_kernel::process::generate_pid();
    mene_kernel::process::ProcessSupervisor::register_process(pid);
    mene_kernel::ipc::IpcManager::init_process(pid);

    let (spawned_pid, opt_aspace) = mene_task::spawn_task(path, pid, handle_syscall);

    if let Some(aspace) = opt_aspace {
        let mut cspace_map = alloc::collections::BTreeMap::new();
        let local_endpoint = alloc::sync::Arc::new(mene_ipc::endpoint::Endpoint::new());
        cspace_map.insert(
            1,
            mene_ipc::capability::Capability::Endpoint(local_endpoint.clone()),
        );

        let ptable = mene_kernel::process::PROCESS_TABLE.lock();
        mene_kernel::device::inject_bootstrap_capabilities(path, &mut cspace_map, &ptable);
        drop(ptable);

        mene_kernel::process::PROCESS_TABLE.lock().insert(
            spawned_pid,
            mene_kernel::process::ProcessInfo {
                app_path: alloc::string::String::from(path),
                cspace: axsync::Mutex::new(cspace_map),
                aspace,
                local_endpoint,
            },
        );

        mene_kernel::process::ProcessSupervisor::transition_state(
            spawned_pid,
            mene_kernel::process::ProcessState::Ready,
        );
    }

    spawned_pid
}

pub fn spawn_app_from_elf(path: &str, elf: &[u8]) -> usize {
    let pid = mene_kernel::process::generate_pid();
    mene_kernel::process::ProcessSupervisor::register_process(pid);
    mene_kernel::ipc::IpcManager::init_process(pid);

    let (spawned_pid, opt_aspace) =
        mene_task::spawn_task_from_bytes(path, elf, pid, handle_syscall);

    if let Some(aspace) = opt_aspace {
        let mut cspace_map = alloc::collections::BTreeMap::new();
        let local_endpoint = alloc::sync::Arc::new(mene_ipc::endpoint::Endpoint::new());
        cspace_map.insert(
            1,
            mene_ipc::capability::Capability::Endpoint(local_endpoint.clone()),
        );

        let ptable = mene_kernel::process::PROCESS_TABLE.lock();
        mene_kernel::device::inject_bootstrap_capabilities(path, &mut cspace_map, &ptable);
        drop(ptable);

        mene_kernel::process::PROCESS_TABLE.lock().insert(
            spawned_pid,
            mene_kernel::process::ProcessInfo {
                app_path: alloc::string::String::from(path),
                cspace: axsync::Mutex::new(cspace_map),
                aspace,
                local_endpoint,
            },
        );

        mene_kernel::process::ProcessSupervisor::transition_state(
            spawned_pid,
            mene_kernel::process::ProcessState::Ready,
        );
    }

    spawned_pid
}

fn linux_write_serial(current_pid: usize, bytes: &[u8]) -> axerrno::AxResult<usize> {
    mene_kernel::ipc::IpcManager::send(current_pid, Handle::SERIAL_ENDPOINT, bytes, 0)
        .map(|_| bytes.len())
}

pub fn handle_syscall(
    uctx: &mut UserContext,
    current_pid: usize,
    aspace_arc: &Arc<Mutex<axmm::AddrSpace>>,
) {
    let sysno_val = uctx.sysno();
    use core::convert::TryFrom;

    // Fallback space for mene specific calls (sysno_val == 500, 501, etc.)
    if let Ok(mene_sysno) = MeneSysno::try_from(sysno_val) {
        let result: axerrno::AxResult<usize> = match mene_sysno {
            MeneSysno::Spawn => {
                let ptr = uctx.arg0() as *const u8;
                let len = uctx.arg1();
                let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(path) = core::str::from_utf8(slice) {
                    Ok(spawn_app(path))
                } else {
                    Err(axerrno::AxError::InvalidInput)
                }
            }
            MeneSysno::SpawnElf => {
                let is_fs_caller = {
                    let ptable = mene_kernel::process::PROCESS_TABLE.lock();
                    ptable
                        .get(&current_pid)
                        .is_some_and(|p| p.app_path == "/boot/fs")
                };
                if !is_fs_caller {
                    Err(axerrno::AxError::PermissionDenied)
                } else {
                    let path_ptr = uctx.arg0() as *const u8;
                    let path_len = uctx.arg1();
                    let elf_ptr = uctx.arg2() as *const u8;
                    let elf_len = uctx.arg3();

                    if path_ptr.is_null() || elf_ptr.is_null() || path_len == 0 || elf_len == 0 {
                        Err(axerrno::AxError::InvalidInput)
                    } else {
                        let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
                        let elf_slice = unsafe { core::slice::from_raw_parts(elf_ptr, elf_len) };
                        if let Ok(path) = core::str::from_utf8(path_slice) {
                            Ok(spawn_app_from_elf(path, elf_slice))
                        } else {
                            Err(axerrno::AxError::InvalidInput)
                        }
                    }
                }
            }
            MeneSysno::IpcSend => {
                let handle = uctx.arg0();
                let ptr = uctx.arg1() as *const u8;
                let len = uctx.arg2();
                // Optional capability to pass
                let passed_cap = uctx.arg3();
                let slice = unsafe { core::slice::from_raw_parts(ptr, len) };

                mene_kernel::ipc::IpcManager::send(current_pid, handle, slice, passed_cap)
                    .map(|_| 0)
            }
            MeneSysno::IpcRecv => {
                let buf_ptr = uctx.arg0() as *mut u8;
                let buf_max = uctx.arg1();
                let sender_pid_ptr = uctx.arg2() as *mut usize;
                // Optional arg to receive capability
                let recv_cap_ptr = uctx.arg3() as *mut usize;

                let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, buf_max) };

                let mut sender_pid = 0;
                let mut recv_cap = 0;
                match mene_kernel::ipc::IpcManager::recv(
                    current_pid,
                    buf_slice,
                    &mut sender_pid,
                    &mut recv_cap,
                ) {
                    Ok(copied) => {
                        if !sender_pid_ptr.is_null() {
                            unsafe {
                                *sender_pid_ptr = sender_pid;
                            }
                        }
                        if !recv_cap_ptr.is_null() {
                            unsafe {
                                *recv_cap_ptr = recv_cap;
                            }
                        }
                        Ok(copied)
                    }
                    Err(e) => {
                        axlog::warn!("IPC recv failed, PID {}, error {:?}", current_pid, e);
                        Err(e)
                    }
                }
            }
            MeneSysno::IpcRecvTimeout => {
                let buf_ptr = uctx.arg0() as *mut u8;
                let buf_max = uctx.arg1();
                let meta_ptr = uctx.arg2() as *mut usize;
                let timeout_ms = uctx.arg3();

                if buf_ptr.is_null() || meta_ptr.is_null() {
                    Err(axerrno::AxError::InvalidInput)
                } else {
                    let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, buf_max) };

                    let mut sender_pid = 0usize;
                    let mut recv_cap = 0usize;
                    match mene_kernel::ipc::IpcManager::recv_timeout(
                        current_pid,
                        buf_slice,
                        &mut sender_pid,
                        &mut recv_cap,
                        Duration::from_millis(timeout_ms as u64),
                    ) {
                        Ok(copied) => {
                            unsafe {
                                *meta_ptr = sender_pid;
                                *meta_ptr.add(1) = recv_cap;
                            }
                            Ok(copied)
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            MeneSysno::UptimeMs => {
                let now_ns = axhal::time::monotonic_time_nanos();
                Ok((now_ns / 1_000_000) as usize)
            }
            MeneSysno::ReadFile => {
                // Temporary bootstrap ability: only init process can use kernel-fs read.
                if current_pid != 1 {
                    Err(axerrno::AxError::PermissionDenied)
                } else {
                    let path_ptr = uctx.arg0() as *const u8;
                    let path_len = uctx.arg1();
                    let buf_ptr = uctx.arg2() as *mut u8;
                    let buf_len = uctx.arg3();

                    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
                    if let Ok(path) = core::str::from_utf8(path_slice) {
                        if let Ok(data) = axfs::api::read(path) {
                            let bytes_to_copy = core::cmp::min(data.len(), buf_len);
                            let buf_slice =
                                unsafe { core::slice::from_raw_parts_mut(buf_ptr, bytes_to_copy) };
                            buf_slice.copy_from_slice(&data[..bytes_to_copy]);
                            Ok(bytes_to_copy)
                        } else {
                            Err(axerrno::AxError::NotFound)
                        }
                    } else {
                        Err(axerrno::AxError::InvalidInput)
                    }
                }
            }
            MeneSysno::MapDevice => {
                let paddr = uctx.arg0();
                let length = uctx.arg1();
                Ok(mene_memory::mmap::do_map_device(paddr, length, aspace_arc))
            }
            MeneSysno::MmapAnon => {
                let length = uctx.arg0();
                let addr = mene_memory::mmap::do_mmap(0, length, aspace_arc);
                if addr == !0 {
                    Err(axerrno::AxError::NoMemory)
                } else {
                    Ok(addr)
                }
            }
            MeneSysno::VmmMapPageTo => {
                let target_pid = uctx.arg0();
                let vaddr = uctx.arg1();
                let length = uctx.arg2();

                // Note: Only PID 3 (VMM) should be allowed to call this in a secure system
                if current_pid != 3 {
                    return; // Denied
                }

                let map = mene_kernel::process::PROCESS_TABLE.lock();
                if let Some(info) = map.get(&target_pid) {
                    let addr = mene_memory::mmap::do_mmap(vaddr, length, &info.aspace);
                    if addr == !0 {
                        Err(axerrno::AxError::NoMemory)
                    } else {
                        Ok(addr)
                    }
                } else {
                    Err(axerrno::AxError::NoSuchProcess)
                }
            }
            MeneSysno::DmaAlloc => {
                let length = uctx.arg0();
                let paddr_out = uctx.arg1() as *mut usize;

                if paddr_out.is_null() || length == 0 {
                    Err(axerrno::AxError::InvalidInput)
                } else if let Some((vaddr, paddr)) =
                    mene_memory::mmap::do_dma_alloc(length, aspace_arc)
                {
                    unsafe {
                        *paddr_out = paddr;
                    }
                    Ok(vaddr)
                } else {
                    Err(axerrno::AxError::NoMemory)
                }
            }
            MeneSysno::PciCfgRead => {
                let bus = uctx.arg0();
                let device = uctx.arg1();
                let function = uctx.arg2();
                let offset = uctx.arg3();

                if device >= 32 || function >= 8 || offset > 0xffc || (offset & 0x3) != 0 {
                    Err(axerrno::AxError::InvalidInput)
                } else {
                    let ecam_base = mene_kernel::device::pci_ecam_base()
                        .unwrap_or(axconfig::devices::PCI_ECAM_BASE);
                    let cfg_addr =
                        ecam_base + (bus << 20) + (device << 15) + (function << 12) + offset;
                    let vaddr =
                        axhal::mem::phys_to_virt(memory_addr::PhysAddr::from_usize(cfg_addr));
                    let val = unsafe { core::ptr::read_volatile(vaddr.as_ptr() as *const u32) };
                    Ok(val as usize)
                }
            }
            MeneSysno::DmaDealloc => {
                let user_vaddr = uctx.arg0();
                let paddr = uctx.arg1();
                let pages = uctx.arg2();

                if pages == 0 {
                    Ok(0)
                } else if mene_memory::mmap::do_dma_dealloc(user_vaddr, paddr, pages, aspace_arc) {
                    Ok(0)
                } else {
                    Err(axerrno::AxError::InvalidInput)
                }
            }
            MeneSysno::VirtToPhys => {
                let user_vaddr = uctx.arg0();
                match mene_memory::mmap::do_virt_to_phys(user_vaddr, aspace_arc) {
                    Some(paddr) => Ok(paddr),
                    None => Err(axerrno::AxError::BadAddress),
                }
            }
            MeneSysno::SleepMs => {
                let ms = uctx.arg0();
                axtask::sleep(Duration::from_millis(ms as u64));
                Ok(0)
            }
            MeneSysno::SystemOff => axhal::power::system_off(),
            MeneSysno::GetBootCfg => {
                let buf_ptr = uctx.arg0() as *mut u8;
                let buf_len = uctx.arg1();
                if buf_ptr.is_null() || buf_len == 0 {
                    Err(axerrno::AxError::InvalidInput)
                } else {
                    let out = unsafe { core::slice::from_raw_parts_mut(buf_ptr, buf_len) };
                    Ok(mene_task::copy_boot_cfg_to(out))
                }
            }
        };
        let retval = match result {
            Ok(val) => val,
            Err(e) => e.code() as isize as usize,
        };
        uctx.set_retval(retval);
        return;
    }

    // Check standard Linux syscalls mapped to microkernel operations.
    if let Some(sysno) = Sysno::new(sysno_val) {
        match sysno {
            Sysno::set_tid_address => {
                uctx.set_retval(current_pid);
                return;
            }
            Sysno::ppoll
            | Sysno::tkill
            | Sysno::tgkill
            | Sysno::rt_sigaction
            | Sysno::rt_sigprocmask => {
                // Minimal startup compatibility for Linux-musl user programs.
                uctx.set_retval(0);
                return;
            }
            Sysno::sigaltstack => {
                uctx.set_retval(0);
                return;
            }
            Sysno::munmap | Sysno::mprotect => {
                uctx.set_retval(0);
                return;
            }
            Sysno::mmap => {
                let length = uctx.arg1();
                let addr = mene_memory::mmap::do_mmap(0, length, aspace_arc);
                if addr == !0 {
                    uctx.set_retval(axerrno::AxError::NoMemory.code() as isize as usize);
                } else {
                    uctx.set_retval(addr);
                }
                return;
            }
            Sysno::getpid => {
                uctx.set_retval(current_pid);
                return;
            }
            Sysno::brk => {
                // Keep a simple compatibility behavior for libc probes.
                // Returning 0 means "no brk heap available" and lets many runtimes fallback to mmap.
                uctx.set_retval(0);
                return;
            }
            Sysno::write => {
                let ptr = uctx.arg1() as *const u8;
                let len = uctx.arg2();
                let result = if ptr.is_null() && len != 0 {
                    Err(axerrno::AxError::InvalidInput)
                } else {
                    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
                    linux_write_serial(current_pid, slice)
                };
                let retval = match result {
                    Ok(n) => n,
                    Err(e) => e.code() as isize as usize,
                };
                uctx.set_retval(retval);
                return;
            }
            Sysno::writev => {
                let iov_ptr = uctx.arg1() as *const usize;
                let iov_cnt = uctx.arg2();
                if iov_ptr.is_null() && iov_cnt != 0 {
                    uctx.set_retval(axerrno::AxError::InvalidInput.code() as isize as usize);
                    return;
                }

                let mut total = 0usize;
                let mut i = 0usize;
                while i < iov_cnt {
                    let base_ptr = unsafe { *iov_ptr.add(i * 2) } as *const u8;
                    let base_len = unsafe { *iov_ptr.add(i * 2 + 1) };
                    if base_ptr.is_null() && base_len != 0 {
                        uctx.set_retval(axerrno::AxError::InvalidInput.code() as isize as usize);
                        return;
                    }
                    if base_len != 0 {
                        let slice = unsafe { core::slice::from_raw_parts(base_ptr, base_len) };
                        if let Err(e) = linux_write_serial(current_pid, slice) {
                            uctx.set_retval(e.code() as isize as usize);
                            return;
                        }
                        total = total.saturating_add(base_len);
                    }
                    i += 1;
                }

                uctx.set_retval(total);
                return;
            }
            Sysno::exit | Sysno::exit_group => {
                let code = uctx.arg0() as i32;
                mene_kernel::process::ProcessSupervisor::mark_process_exited(current_pid, code);
                mene_kernel::ipc::IpcManager::cleanup_process(current_pid);
                mene_kernel::process::PROCESS_TABLE
                    .lock()
                    .remove(&current_pid);
                mene_kernel::process::ProcessSupervisor::cleanup_process(current_pid);
                axtask::exit(code);
            }
            _ => {}
        }
    }

    warn!("Unimplemented syscall: {}", sysno_val);
    uctx.set_retval(-38isize as usize); // ENOSYS equivalents
}
