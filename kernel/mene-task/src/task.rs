use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use axhal::paging::MappingFlags;
use axhal::uspace::{ReturnReason, UserContext};
use axlog::{error, warn};
use axmm::AddrSpace;
use axsync::Mutex;
use axtask::{TaskInner, spawn_task as ax_spawn_task};
use memory_addr::{MemoryAddr, VirtAddr};
use mene_config::*;
use xmas_elf::{ElfFile, program::Type};

pub type SyscallHandler = fn(&mut UserContext, usize, &Arc<Mutex<AddrSpace>>);

lazy_static::lazy_static! {
    static ref PRELOADED_ELFS: Mutex<BTreeMap<String, Vec<u8>>> = Mutex::new(BTreeMap::new());
    static ref PRELOADED_BOOT_CFG: Mutex<String> = Mutex::new(String::new());
}

pub fn preload_boot_assets() -> bool {
    let boot_cfg_bytes = match axfs::api::read("/boot/boot.cfg") {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to read /boot/boot.cfg during preload: {:?}", e);
            return false;
        }
    };

    let boot_cfg = match core::str::from_utf8(&boot_cfg_bytes) {
        Ok(s) => s,
        Err(_) => {
            error!("/boot/boot.cfg is not valid UTF-8");
            return false;
        }
    };

    {
        let mut cfg = PRELOADED_BOOT_CFG.lock();
        *cfg = boot_cfg.to_string();
    }

    // Minimal bootstrap set: later apps are loaded by user-space fs.
    let paths = [
        "/boot/init",
        "/boot/serial",
        "/boot/virtio_blk",
        "/boot/fs",
    ];

    let mut cache = PRELOADED_ELFS.lock();
    cache.clear();
    for path in paths {
        if cache.contains_key(path) {
            continue;
        }
        match axfs::api::read(path) {
            Ok(data) => {
                warn!("Preloaded app {} ({} bytes)", path, data.len());
                cache.insert(String::from(path), data);
            }
            Err(e) => {
                warn!("Preload skipped {}: {:?}", path, e);
            }
        }
    }

    cache.contains_key("/boot/init")
}

pub fn copy_boot_cfg_to(buf: &mut [u8]) -> usize {
    let cfg = PRELOADED_BOOT_CFG.lock();
    let bytes = cfg.as_bytes();
    let n = core::cmp::min(bytes.len(), buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
    n
}

fn get_preloaded_elf(path: &str) -> Option<Vec<u8>> {
    PRELOADED_ELFS.lock().get(path).cloned()
}

fn spawn_task_from_elf(
    path: &str,
    pid: usize,
    handler: SyscallHandler,
    file_data: &[u8],
) -> (usize, Option<Arc<Mutex<AddrSpace>>>) {
    warn!("Spawning app: {} with PID {}", path, pid);

    let elf = ElfFile::new(file_data).expect("Invalid ELF");
    let entry_point = elf.header.pt2.entry_point();

    let mut aspace = axmm::new_user_aspace(VirtAddr::from_usize(USER_SPACE_BASE), USER_SPACE_SIZE)
        .expect("Failed to create aspace");

    for p in elf.program_iter() {
        if p.get_type().unwrap() == Type::Load {
            let start = p.virtual_addr();
            let end = start + p.mem_size();
            let vaddr = VirtAddr::from_usize(start as usize).align_down_4k();
            let vend = VirtAddr::from_usize(end as usize).align_up_4k();

            let flags = MappingFlags::USER
                | MappingFlags::READ
                | MappingFlags::WRITE
                | MappingFlags::EXECUTE;
            let size = vend.as_usize() - vaddr.as_usize();

            aspace.map_alloc(vaddr, size, flags, true).unwrap();

            let file_start = p.offset() as usize;
            let file_end = file_start + p.file_size() as usize;
            let segment_data = &file_data[file_start..file_end];

            aspace
                .write(VirtAddr::from_usize(start as usize), segment_data)
                .unwrap();
        }
    }

    let stack_bottom = VirtAddr::from_usize(USER_STACK_TOP - USER_STACK_SIZE);
    let stack_size = USER_STACK_SIZE;
    let stack_top = VirtAddr::from_usize(USER_STACK_TOP);

    aspace
        .map_alloc(
            stack_bottom,
            stack_size,
            MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE,
            true,
        )
        .unwrap();

    let uctx = UserContext::new((entry_point as usize).into(), stack_top.into(), 0);
    let aspace_arc = Arc::new(Mutex::new(aspace));
    let aspace_clone = aspace_arc.clone();

    let path_clone = String::from(path);
    let mut task_inner = TaskInner::new(
        move || {
            let _keep = aspace_clone.clone();
            let mut uctx_run = uctx;
            warn!("Entering user mode for {} (PID {})", path_clone, pid);
            loop {
                let reason = uctx_run.run();
                match reason {
                    ReturnReason::Syscall => {
                        handler(&mut uctx_run, pid, &aspace_clone);
                    }
                    ReturnReason::PageFault(addr, ref flags) => {
                        error!("Fatal PageFault in {}: {:#x} {:?}", path_clone, addr, flags);
                        break;
                    }
                    ReturnReason::Interrupt => {}
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

    task_inner
        .ctx_mut()
        .set_page_table_root(aspace_arc.lock().page_table_root());
    ax_spawn_task(task_inner);

    (pid, Some(aspace_arc))
}

pub fn spawn_task(
    path: &str,
    pid: usize,
    handler: SyscallHandler,
) -> (usize, Option<Arc<Mutex<AddrSpace>>>) {
    let file_data = match get_preloaded_elf(path) {
        Some(data) => data,
        None => {
            error!("{} not found in preload cache", path);
            return (0, None); // Return 0 as error pid or indicator
        }
    };
    warn!("Loaded ELF {} ({} bytes)", path, file_data.len());
    spawn_task_from_elf(path, pid, handler, &file_data)
}

pub fn spawn_task_from_bytes(
    path: &str,
    file_data: &[u8],
    pid: usize,
    handler: SyscallHandler,
) -> (usize, Option<Arc<Mutex<AddrSpace>>>) {
    spawn_task_from_elf(path, pid, handler, file_data)
}
