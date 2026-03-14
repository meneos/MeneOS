use alloc::sync::Arc;
use axalloc::{UsageKind, global_allocator};
use axhal::paging::MappingFlags;
use axhal::mem::{phys_to_virt, virt_to_phys};
use axmm::AddrSpace;
use axsync::Mutex;
use memory_addr::{MemoryAddr, PhysAddr, VirtAddr};
use mene_config::*;

pub fn do_mmap(addr: usize, length: usize, aspace_arc: &Arc<Mutex<AddrSpace>>) -> usize {
    let mut aspace = aspace_arc.lock();
    let size = memory_addr::align_up_4k(length);
    let limit = memory_addr::VirtAddrRange::new(
        VirtAddr::from_usize(USER_SPACE_BASE),
        VirtAddr::from_usize(USER_STACK_TOP - USER_STACK_SIZE),
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

    let map_flags =
        MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE | MappingFlags::DEVICE;
    if aspace.map_linear(vaddr, pdaddr, size, map_flags).is_ok() {
        vaddr.as_usize()
    } else {
        !0 // MAP_FAILED
    }
}

pub fn do_dma_alloc(length: usize, aspace_arc: &Arc<Mutex<AddrSpace>>) -> Option<(usize, usize)> {
    if length == 0 {
        return None;
    }

    let size = memory_addr::align_up_4k(length);
    let num_pages = size / memory_addr::PAGE_SIZE_4K;

    let kern_vaddr = global_allocator()
        .alloc_pages(num_pages, memory_addr::PAGE_SIZE_4K, UsageKind::Dma)
        .ok()?;
    let paddr = virt_to_phys(VirtAddr::from_usize(kern_vaddr)).as_usize();

    let mut aspace = aspace_arc.lock();
    let limit = memory_addr::VirtAddrRange::new(
        VirtAddr::from_usize(USER_SPACE_BASE),
        VirtAddr::from_usize(USER_STACK_TOP - USER_STACK_SIZE),
    );
    let hints = VirtAddr::from_usize(0x4000_0000);

    let Some(user_vaddr) = aspace.find_free_area(hints, size, limit) else {
        global_allocator().dealloc_pages(kern_vaddr, num_pages, UsageKind::Dma);
        return None;
    };

    let map_flags = MappingFlags::USER | MappingFlags::READ | MappingFlags::WRITE;
    let phys = memory_addr::PhysAddr::from_usize(paddr);
    if aspace.map_linear(user_vaddr, phys, size, map_flags).is_err() {
        global_allocator().dealloc_pages(kern_vaddr, num_pages, UsageKind::Dma);
        return None;
    }

    Some((user_vaddr.as_usize(), paddr))
}

pub fn do_dma_dealloc(
    user_vaddr: usize,
    paddr: usize,
    pages: usize,
    aspace_arc: &Arc<Mutex<AddrSpace>>,
) -> bool {
    if pages == 0 {
        return true;
    }

    let size = pages.saturating_mul(memory_addr::PAGE_SIZE_4K);
    if size == 0 {
        return false;
    }

    let mut aspace = aspace_arc.lock();
    if aspace.unmap(VirtAddr::from_usize(user_vaddr), size).is_err() {
        return false;
    }

    let kern_vaddr = phys_to_virt(PhysAddr::from_usize(paddr)).as_usize();
    global_allocator().dealloc_pages(kern_vaddr, pages, UsageKind::Dma);
    true
}

pub fn do_virt_to_phys(user_vaddr: usize, aspace_arc: &Arc<Mutex<AddrSpace>>) -> Option<usize> {
    let aspace = aspace_arc.lock();
    let va = VirtAddr::from_usize(user_vaddr);
    let page_base = va.align_down_4k();
    let page_off = va.align_offset_4k();
    let (paddr, _, _) = aspace.page_table().query(page_base).ok()?;
    Some((paddr + page_off).as_usize())
}
