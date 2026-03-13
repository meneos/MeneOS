use alloc::sync::Arc;
use axsync::Mutex;
use axmm::AddrSpace;
use memory_addr::VirtAddr;
use axhal::paging::MappingFlags;
use mene_config::*;

pub fn do_mmap(addr: usize, length: usize, aspace_arc: &Arc<Mutex<AddrSpace>>) -> usize {
    let mut aspace = aspace_arc.lock();
    let size = memory_addr::align_up_4k(length);
    let limit = memory_addr::VirtAddrRange::new(
        VirtAddr::from_usize(USER_SPACE_BASE),
        VirtAddr::from_usize(USER_STACK_TOP - USER_STACK_SIZE)
    );
    
    let hints = VirtAddr::from_usize(addr.max(0x4000_0000));
    
    if let Some(vaddr) = aspace.find_free_area(hints, size, limit) {
        let map_flags = MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE;
        if aspace.map_alloc(vaddr, size, map_flags, true).is_ok() {
            vaddr.as_usize()
        } else {
            !0 // MAP_FAILED
        }
    } else {
        !0
    }
}

pub fn do_map_device(paddr: usize, length: usize, aspace_arc: &Arc<Mutex<AddrSpace>>) -> usize {
    let mut aspace = aspace_arc.lock();
    let size = memory_addr::align_up_4k(length);
    // Identity map logical == physical
    let vaddr = VirtAddr::from_usize(paddr);
    let pdaddr = memory_addr::PhysAddr::from_usize(paddr);
    
    let map_flags = MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE;
    if aspace.map_linear(vaddr, pdaddr, size, map_flags).is_ok() {
        vaddr.as_usize()
    } else {
        !0 // MAP_FAILED
    }
}