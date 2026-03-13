#![no_std]
#![feature(bstr)]

extern crate alloc;
#[macro_use]
extern crate axlog;

use axhal::uspace::{UserContext, ReturnReason};

use syscalls::Sysno;
use alloc::sync::Arc;
use alloc::string::String;
use alloc::vec::Vec;
use axhal::paging::MappingFlags;
use axsync::Mutex;
use axtask::{spawn_task, TaskInner};
use memory_addr::{MemoryAddr, PAGE_SIZE_4K, VirtAddr};
use xmas_elf::{ElfFile, program::Type};

lazy_static::lazy_static! {
    static ref IPC_MAILBOX: axsync::Mutex<Vec<u8>> = axsync::Mutex::new(Vec::new());
}

pub fn spawn_app(path: &str) {
    info!("Spawning app: {}", path);
    // 1. Read ELF
    let file_data = match axfs::api::read(path) {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to read {}: {:?}", path, e);
            return;
        }
    };
    
    let elf = ElfFile::new(&file_data).expect("Invalid ELF");
    let entry_point = elf.header.pt2.entry_point();

    let aspace_base = VirtAddr::from(0x200000);
    let aspace_size = 0x40000000; 
    let mut aspace = axmm::new_user_aspace(aspace_base, aspace_size).expect("Failed to create aspace");

    for p in elf.program_iter() {
        if p.get_type().unwrap() == Type::Load {
            let start = p.virtual_addr();
            let end = start + p.mem_size();
            let vaddr = VirtAddr::from_usize(start as usize).align_down_4k();
            let vend = VirtAddr::from_usize(end as usize).align_up_4k();
            
            let flags = MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE;
            let size = vend.as_usize() - vaddr.as_usize();
            
            aspace.map_alloc(vaddr, size, flags, true).unwrap();
            
            let file_start = p.offset() as usize;
            let file_end = file_start + p.file_size() as usize;
            let segment_data = &file_data[file_start..file_end];
            
            aspace.write(VirtAddr::from_usize(start as usize), segment_data).unwrap();
        }
    }

    let stack_bottom = VirtAddr::from(0x4000_0000);
    let stack_size = PAGE_SIZE_4K * 16;
    let stack_top = stack_bottom + stack_size;
    aspace.map_alloc(
        stack_bottom,
        stack_size,
        MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE,
        true,
    ).unwrap();

    let uctx = UserContext::new((entry_point as usize).into(), stack_top.into(), 0);
    let aspace_arc = Arc::new(Mutex::new(aspace));
    let _aspace_keep = aspace_arc.clone();

    let path_clone = String::from(path);
    let mut task_inner = TaskInner::new(
        move || {
            let _keep = _aspace_keep;
            let mut uctx_run = uctx;
            info!("Entering user mode for {}!", path_clone);
            loop {
                let reason = uctx_run.run();
                match reason {
                    ReturnReason::Syscall => {
                        handle_syscall(&mut uctx_run);
                    }
                    ReturnReason::PageFault(addr, ref flags) => {
                        error!("Fatal PageFault in {}: {:#x} {:?}", path_clone, addr, flags);
                        break;
                    }
                    ReturnReason::Interrupt => {
                    }
                    _ => {
                        error!("Unexpected exit reason: {:?}", reason);
                        break;
                    }
                }
            }
        },
        String::from(path).into(),
        8192 * 4,
    );

    task_inner.ctx_mut().set_page_table_root(aspace_arc.lock().page_table_root());
    spawn_task(task_inner);
}

pub fn handle_syscall(uctx: &mut UserContext) {
    let sysno = uctx.sysno();
    
    // Quick microkernel routing
    let result = match sysno {
        // sys_log
        1 => {
            let ptr = uctx.arg0() as *const u8;
            let len = uctx.arg1();
            let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
            if let Ok(s) = core::str::from_utf8(slice) {
                // info!("App output: {}", s);
                axlog::ax_println!("{}", s);
            }
            0
        }
        // sys_spawn
        2 => {
            let ptr = uctx.arg0() as *const u8;
            let len = uctx.arg1();
            let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
            if let Ok(path) = core::str::from_utf8(slice) {
                spawn_app(path);
            }
            0
        }
        // sys_read_file
        3 => {
            let path_ptr = uctx.arg0() as *const u8;
            let path_len = uctx.arg1();
            let buf_ptr = uctx.arg2() as *mut u8;
            
            let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
            let path = core::str::from_utf8(path_slice).unwrap_or("");
            
            if let Ok(data) = axfs::api::read(path) {
                let bytes_to_copy = core::cmp::min(data.len(), 256); // quick arbitrary buffer size from init
                let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, bytes_to_copy) };
                buf_slice.copy_from_slice(&data[..bytes_to_copy]);
                bytes_to_copy
            } else {
                0
            }
        }
        // sys_ipc_send
        4 => {
            let _pid = uctx.arg0();
            let ptr = uctx.arg1() as *const u8;
            let len = uctx.arg2();
            let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
            IPC_MAILBOX.lock().extend_from_slice(slice);
            0
        }
        // sys_ipc_recv
        5 => {
            let buf_ptr = uctx.arg0() as *mut u8;
            let buf_max = uctx.arg1();
            
            let mut mb = IPC_MAILBOX.lock();
            if !mb.is_empty() {
                let bytes_to_copy = core::cmp::min(mb.len(), buf_max);
                let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr, bytes_to_copy) };
                buf_slice.copy_from_slice(&mb[..bytes_to_copy]);
                mb.drain(..bytes_to_copy);
                bytes_to_copy
            } else {
                drop(mb);
                axtask::yield_now(); // very slow polling approach for demo
                0
            }
        }
        // sys_exit
        6 => {
            let code = uctx.arg0() as i32;
            axtask::exit(code);
            // unreachable
        }
        _ => {
            warn!("Unimplemented syscall: {}", sysno);
            -38isize as usize // ENOSYS equivalents
        }
    };
    
    uctx.set_retval(result as usize);
}
