#![no_std]
#![feature(bstr)]

extern crate alloc;
#[macro_use]
extern crate axlog;

use axhal::uspace::UserContext;
use alloc::sync::Arc;
use axsync::Mutex;
use mene_abi::{Sysno, MeneSysno};

pub fn spawn_app(path: &str) -> usize {
    let pid = mene_kernel::process::generate_pid();
    mene_kernel::ipc::IpcManager::init_process(pid);
    
    // Pass execution down to the mene-task component
    mene_task::spawn_task(path, pid, handle_syscall)
}

pub fn handle_syscall(uctx: &mut UserContext, current_pid: usize, aspace_arc: &Arc<Mutex<axmm::AddrSpace>>) {
    let sysno_val = uctx.sysno();
    use core::convert::TryFrom;
    
    // Fallback space for mene specific calls (sysno_val == 500, 501, etc.)
    if let Ok(mene_sysno) = MeneSysno::try_from(sysno_val) {
        let result = match mene_sysno {
            MeneSysno::Spawn => {
                let ptr = uctx.arg0() as *const u8;
                let len = uctx.arg1();
                let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(path) = core::str::from_utf8(slice) {
                    spawn_app(path)
                } else {
                    0
                }
            }
            MeneSysno::IpcSend => {
                let target_pid = uctx.arg0();
                let ptr = uctx.arg1() as *const u8;
                let len = uctx.arg2();
                let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
                
                axlog::info!("PID {} sending IPC to PID {}", current_pid, target_pid);
                if mene_kernel::ipc::IpcManager::send(target_pid, slice) == 0 {
                    axlog::info!("IPC sent to PID {} success", target_pid);
                    0
                } else {
                    axlog::warn!("IPC send failed, PID {} not found", target_pid);
                    -1isize as usize
                }
            }
            MeneSysno::IpcRecv => {
                let buf_ptr = uctx.arg0() as *mut u8;
                let buf_max = uctx.arg1();
                let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, buf_max) };
                
                mene_kernel::ipc::IpcManager::recv(current_pid, buf_slice)
            }
            MeneSysno::ReadFile => {
                let path_ptr = uctx.arg0() as *const u8;
                let path_len = uctx.arg1();
                let buf_ptr = uctx.arg2() as *mut u8;
                let buf_len = uctx.arg3();
                
                let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
                if let Ok(path) = core::str::from_utf8(path_slice) {
                    if let Ok(data) = axfs::api::read(path) {
                        let bytes_to_copy = core::cmp::min(data.len(), buf_len);
                        let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, bytes_to_copy) };
                        buf_slice.copy_from_slice(&data[..bytes_to_copy]);
                        bytes_to_copy
                    } else {
                        0 // Read failed
                    }
                } else {
                    0 // Invalid path
                }
            }
        };
        uctx.set_retval(result);
        return;
    }
    
    // Check standard Linux syscalls mapped to microkernel operations
    if let Some(sysno) = Sysno::new(sysno_val) {
        if let Sysno::write = sysno {
            let _fd = uctx.arg0();
            let ptr = uctx.arg1() as *const u8;
            let len = uctx.arg2();
            let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
            if let Ok(s) = core::str::from_utf8(slice) {
                axlog::ax_println!("{}", s);
            }
            uctx.set_retval(len);
            return;
        }
        
        if let Sysno::exit = sysno {
            let code = uctx.arg0() as i32;
            mene_kernel::ipc::IpcManager::cleanup_process(current_pid);
            axtask::exit(code);
            // unreachable
        }
        
        if let Sysno::mmap = sysno {
            let addr = uctx.arg0();
            let length = uctx.arg1();
            let ret = mene_memory::mmap::do_mmap(addr, length, aspace_arc);
            uctx.set_retval(ret);
            return;
        }
    }

    warn!("Unimplemented syscall: {}", sysno_val);
    uctx.set_retval(-38isize as usize); // ENOSYS equivalents
}
