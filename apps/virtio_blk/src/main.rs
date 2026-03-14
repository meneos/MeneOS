#![no_std]
#![no_main]

use core::ptr::NonNull;
use ulib::blk::{
    MAX_IO_BYTES, REQ_FLUSH, REQ_GET_INFO, REQ_PING, REQ_READ, REQ_WRITE, RW_HDR_LEN,
    RW_TAGGED_HDR_LEN, TAGGED_HDR_LEN,
};
use virtio_drivers::device::blk::{SECTOR_SIZE, VirtIOBlk};
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::transport::pci::bus::{Cam, Command, HeaderType, PciRoot};
use virtio_drivers::transport::pci::PciTransport;
use virtio_drivers::transport::{DeviceType, Transport};
use virtio_drivers::{BufferDirection, Hal, PAGE_SIZE, PhysAddr};

const PAGE_MASK: usize = PAGE_SIZE - 1;
const MOCK_BLOCK_SIZE: usize = SECTOR_SIZE;
const MOCK_BLOCK_COUNT: usize = 256;

static mut MOCK_DISK: [u8; MOCK_BLOCK_SIZE * MOCK_BLOCK_COUNT] = [0; MOCK_BLOCK_SIZE * MOCK_BLOCK_COUNT];
const VIRTIO_BLK_MMIO_PADDR: usize = 0x0a00_0000;
const VIRTIO_BLK_MMIO_SIZE: usize = 0x1000;
const PCI_ECAM_BASE: usize = 0x4010_000000;
const PCI_ECAM_SIZE: usize = 0x1000_0000;
const PCI_VENDOR_VIRTIO: usize = 0x1af4;
const PCI_DEVICE_VIRTIO_BLK_LEGACY: usize = 0x1001;
const PCI_DEVICE_VIRTIO_BLK_MODERN: usize = 0x1042;

struct UserVirtHal;

unsafe impl Hal for UserVirtHal {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        if pages == 0 {
            return (0, NonNull::dangling());
        }

        let bytes = pages.saturating_mul(PAGE_SIZE);
        let mut paddr = 0usize;
        let vaddr = ulib::sys_dma_alloc(bytes, &mut paddr);
        if vaddr == !0 || paddr == 0 {
            (0, NonNull::dangling())
        } else {
            let ptr = NonNull::new(vaddr as *mut u8).unwrap_or(NonNull::dangling());
            (paddr, ptr)
        }
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, vaddr: NonNull<u8>, pages: usize) -> i32 {
        let ret = ulib::sys_dma_dealloc(vaddr.as_ptr() as usize, paddr, pages);
        if ret == 0 { 0 } else { -1 }
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, size: usize) -> NonNull<u8> {
        let pa = paddr as usize;
        let base = pa & !PAGE_MASK;
        let off = pa & PAGE_MASK;
        let need = off.saturating_add(size.max(1));
        let map_size = (need + PAGE_MASK) & !PAGE_MASK;
        let mapped = ulib::sys_map_device(base, map_size);
        if mapped == !0 {
            return NonNull::dangling();
        }
        NonNull::new((mapped + off) as *mut u8).unwrap_or(NonNull::dangling())
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        let slice_ptr = buffer.as_ptr();
        let vaddr = (*slice_ptr).as_ptr() as usize;
        let len = (&*slice_ptr).len();
        if len == 0 {
            return 0;
        }

        let first_paddr = ulib::sys_virt_to_phys(vaddr);
        if first_paddr == !0 {
            return 0;
        }

        let first_page_rem = PAGE_SIZE - (vaddr & PAGE_MASK);
        let mut covered = len.min(first_page_rem);
        while covered < len {
            let cur_v = vaddr + covered;
            let expected = (first_paddr + covered) & !PAGE_MASK;
            let cur_p = ulib::sys_virt_to_phys(cur_v);
            if cur_p == !0 || (cur_p & !PAGE_MASK) != expected {
                return 0;
            }
            let step = (len - covered).min(PAGE_SIZE);
            covered += step;
        }

        first_paddr
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {}
}

enum Backend {
    RealMmio(VirtIOBlk<UserVirtHal, MmioTransport>),
    RealPci(VirtIOBlk<UserVirtHal, PciTransport>),
    Mock,
}

impl Backend {
    fn info(&mut self) -> (u64, u64) {
        match self {
            Backend::RealMmio(dev) => (SECTOR_SIZE as u64, dev.capacity()),
            Backend::RealPci(dev) => (SECTOR_SIZE as u64, dev.capacity()),
            Backend::Mock => (MOCK_BLOCK_SIZE as u64, MOCK_BLOCK_COUNT as u64),
        }
    }

    fn read_blocks(&mut self, sector: usize, out: &mut [u8]) -> bool {
        if out.is_empty() || (out.len() % SECTOR_SIZE) != 0 {
            return false;
        }

        match self {
            Backend::RealMmio(dev) => dev.read_blocks(sector, out).is_ok(),
            Backend::RealPci(dev) => dev.read_blocks(sector, out).is_ok(),
            Backend::Mock => {
                let start = sector.saturating_mul(MOCK_BLOCK_SIZE);
                let end = start.saturating_add(out.len());
                if end > MOCK_BLOCK_SIZE * MOCK_BLOCK_COUNT {
                    return false;
                }
                // SAFETY: single-process mock backend storage and checked bounds.
                unsafe {
                    out.copy_from_slice(&MOCK_DISK[start..end]);
                }
                true
            }
        }
    }

    fn write_blocks(&mut self, sector: usize, data: &[u8]) -> bool {
        if data.is_empty() || (data.len() % SECTOR_SIZE) != 0 {
            return false;
        }

        match self {
            Backend::RealMmio(dev) => dev.write_blocks(sector, data).is_ok(),
            Backend::RealPci(dev) => dev.write_blocks(sector, data).is_ok(),
            Backend::Mock => {
                let start = sector.saturating_mul(MOCK_BLOCK_SIZE);
                let end = start.saturating_add(data.len());
                if end > MOCK_BLOCK_SIZE * MOCK_BLOCK_COUNT {
                    return false;
                }
                // SAFETY: single-process mock backend storage and checked bounds.
                unsafe {
                    MOCK_DISK[start..end].copy_from_slice(data);
                }
                true
            }
        }
    }

    fn flush(&mut self) -> bool {
        match self {
            Backend::RealMmio(dev) => dev.flush().is_ok(),
            Backend::RealPci(dev) => dev.flush().is_ok(),
            Backend::Mock => true,
        }
    }
}

fn init_backend() -> Backend {
    if let Some(dev) = try_init_virtio_pci() {
        ulib::sys_log("virtio-blk: using PCI transport backend");
        return Backend::RealPci(dev);
    }
    ulib::sys_log("virtio-blk: PCI transport init failed, trying fixed MMIO path");

    if let Some(dev) = try_init_virtio_at(VIRTIO_BLK_MMIO_PADDR, VIRTIO_BLK_MMIO_SIZE) {
        ulib::sys_log("virtio-blk: using fixed MMIO backend");
        return Backend::RealMmio(dev);
    }

    ulib::sys_log("virtio-blk: transport init failed, fallback to mock backend");
    Backend::Mock
}

fn try_init_virtio_at(mmio_paddr: usize, mmio_size: usize) -> Option<VirtIOBlk<UserVirtHal, MmioTransport>> {
    let mapped = ulib::sys_map_device(mmio_paddr, mmio_size);
    if mapped == !0 {
        return None;
    }

    let header = NonNull::new(mapped as *mut VirtIOHeader)?;
    // SAFETY: MMIO region is mapped into user address space by `sys_map_device`.
    let transport = unsafe { MmioTransport::new(header) }.ok()?;
    if transport.device_type() != DeviceType::Block {
        return None;
    }

    VirtIOBlk::<UserVirtHal, _>::new(transport).ok()
}

fn try_init_virtio_pci() -> Option<VirtIOBlk<UserVirtHal, PciTransport>> {
    let ecam_vaddr = ulib::sys_map_device(PCI_ECAM_BASE, PCI_ECAM_SIZE);
    if ecam_vaddr == !0 {
        ulib::sys_log("virtio-blk: map PCI ECAM failed");
        return None;
    }

    let mut root = unsafe { PciRoot::new(ecam_vaddr as *mut u8, Cam::Ecam) };
    for bus in 0u8..=7 {
        for (bdf, dev_info) in root.enumerate_bus(bus) {
            if dev_info.header_type != HeaderType::Standard {
                continue;
            }
            if dev_info.vendor_id as usize != PCI_VENDOR_VIRTIO {
                continue;
            }
            if dev_info.device_id as usize != PCI_DEVICE_VIRTIO_BLK_LEGACY
                && dev_info.device_id as usize != PCI_DEVICE_VIRTIO_BLK_MODERN
            {
                continue;
            }

            let (_status, cmd) = root.get_status_command(bdf);
            root.set_command(
                bdf,
                cmd | Command::IO_SPACE | Command::MEMORY_SPACE | Command::BUS_MASTER,
            );

            let transport = match PciTransport::new::<UserVirtHal>(&mut root, bdf) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if transport.device_type() != DeviceType::Block {
                continue;
            }
            if let Ok(dev) = VirtIOBlk::<UserVirtHal, _>::new(transport) {
                return Some(dev);
            }
        }
    }
    None
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Keep early logs through serial service.
    ulib::sys_log("virtio-blk: service started");
    ulib::init_allocator();

    let mut backend = init_backend();

    let mut req_buf = [0u8; 2 + 8 + 4 + MAX_IO_BYTES];
    let mut io_buf = [0u8; MAX_IO_BYTES];
    let mut from_pid = 0usize;

    loop {
        let mut reply_cap = None;
        let req_len = ulib::sys_ipc_recv(&mut from_pid, &mut req_buf, &mut reply_cap);
        if req_len < 2 {
            continue;
        }

        let opcode = u16::from_le_bytes([req_buf[0], req_buf[1]]);
        let tagged = req_len >= TAGGED_HDR_LEN;
        let req_tag = if tagged {
            u32::from_le_bytes([req_buf[2], req_buf[3], req_buf[4], req_buf[5]])
        } else {
            0
        };
        let rw_hdr_len = if tagged { RW_TAGGED_HDR_LEN } else { RW_HDR_LEN };
        let payload_off = if tagged { TAGGED_HDR_LEN } else { 2 };

        let send_reply = |cap: ulib::Handle, payload: &[u8]| {
            if tagged {
                let mut out = [0u8; MAX_IO_BYTES + 16];
                let n = payload.len();
                out[0..4].copy_from_slice(&req_tag.to_le_bytes());
                out[4..4 + n].copy_from_slice(payload);
                ulib::sys_ipc_send(cap, &out[..4 + n], None);
            } else {
                ulib::sys_ipc_send(cap, payload, None);
            }
        };

        match opcode {
            REQ_PING => {
                if let Some(cap) = reply_cap {
                    // Echo a small payload to prove request/reply IPC is alive.
                    send_reply(cap, b"PONG");
                }
            }
            REQ_GET_INFO => {
                if let Some(cap) = reply_cap {
                    let (block_size, block_count) = backend.info();

                    let mut resp = [0u8; 16];
                    resp[0..8].copy_from_slice(&block_size.to_le_bytes());
                    resp[8..16].copy_from_slice(&block_count.to_le_bytes());
                    send_reply(cap, &resp);
                }
            }
            REQ_READ => {
                if let Some(cap) = reply_cap {
                    if req_len < rw_hdr_len {
                        send_reply(cap, b"EINVAL");
                        continue;
                    }

                    let mut sec = [0u8; 8];
                    sec.copy_from_slice(&req_buf[payload_off..payload_off + 8]);
                    let sector = u64::from_le_bytes(sec) as usize;

                    let mut n = [0u8; 4];
                    n.copy_from_slice(&req_buf[payload_off + 8..payload_off + 12]);
                    let bytes = u32::from_le_bytes(n) as usize;

                    if bytes == 0 || bytes > MAX_IO_BYTES || (bytes % SECTOR_SIZE) != 0 {
                        send_reply(cap, b"EINVAL");
                        continue;
                    }

                    if backend.read_blocks(sector, &mut io_buf[..bytes]) {
                        send_reply(cap, &io_buf[..bytes]);
                    } else {
                        send_reply(cap, b"EIO");
                    }
                }
            }
            REQ_WRITE => {
                if let Some(cap) = reply_cap {
                    if req_len < rw_hdr_len {
                        send_reply(cap, b"EINVAL");
                        continue;
                    }

                    let mut sec = [0u8; 8];
                    sec.copy_from_slice(&req_buf[payload_off..payload_off + 8]);
                    let sector = u64::from_le_bytes(sec) as usize;

                    let mut n = [0u8; 4];
                    n.copy_from_slice(&req_buf[payload_off + 8..payload_off + 12]);
                    let bytes = u32::from_le_bytes(n) as usize;

                    if bytes == 0
                        || bytes > MAX_IO_BYTES
                        || (bytes % SECTOR_SIZE) != 0
                        || req_len < rw_hdr_len + bytes
                    {
                        send_reply(cap, b"EINVAL");
                        continue;
                    }

                    if backend.write_blocks(sector, &req_buf[rw_hdr_len..rw_hdr_len + bytes]) {
                        send_reply(cap, b"OK");
                    } else {
                        send_reply(cap, b"EIO");
                    }
                }
            }
            REQ_FLUSH => {
                if let Some(cap) = reply_cap {
                    if backend.flush() {
                        send_reply(cap, b"OK");
                    } else {
                        send_reply(cap, b"EIO");
                    }
                }
            }
            _ => {
                if let Some(cap) = reply_cap {
                    send_reply(cap, b"EINVAL");
                }
            }
        }
    }
}
