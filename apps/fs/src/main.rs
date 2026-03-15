#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use axfatfs::{
    FileSystem, FsOptions, LossyOemCpConverter, NullTimeProvider, Read, Seek, SeekFrom, Write,
};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use ulib::blk;
use ulib::fs::{
    FLAG_REQID, MAX_DATA, MAX_PATH, PATH_HDR_LEN, PATH_HDR_LEN_V2, REQ_DELETE, REQ_EXEC,
    REQ_PING, REQ_READ, REQ_WRITE, RESP_REQID_LEN, WRITE_HDR_LEN, WRITE_HDR_LEN_V2,
};

type FsType = FileSystem<VirtioBlkDisk, NullTimeProvider, LossyOemCpConverter>;

static BLK_REQ_ID: AtomicU32 = AtomicU32::new(1);
static BLK_SERVICE_HANDLE: AtomicUsize = AtomicUsize::new(0);

fn next_blk_req_id() -> u32 {
    BLK_REQ_ID.fetch_add(1, Ordering::Relaxed)
}

fn blk_service_handle() -> ulib::Handle {
    let cached = BLK_SERVICE_HANDLE.load(Ordering::Relaxed);
    if cached != 0 {
        return ulib::Handle::from_usize(cached);
    }

    let resolved = ulib::ctl_lookup_service("virtio_blk", 100).unwrap_or(ulib::Handle::VirtioBlkEndpoint);
    BLK_SERVICE_HANDLE.store(resolved.to_usize(), Ordering::Relaxed);
    resolved
}

struct VirtioBlkDisk {
    pos: u64,
    size: u64,
    block_size: usize,
}

impl VirtioBlkDisk {
    fn new() -> Option<Self> {
        let mut req = [0u8; 2];
        req.copy_from_slice(&blk::REQ_GET_INFO.to_le_bytes());
        let mut resp = [0u8; 16];
        let len = blk_call(&req, &mut resp);
        if len != 16 {
            return None;
        }

        let mut bs = [0u8; 8];
        let mut bc = [0u8; 8];
        bs.copy_from_slice(&resp[0..8]);
        bc.copy_from_slice(&resp[8..16]);
        let block_size = u64::from_le_bytes(bs) as usize;
        let block_count = u64::from_le_bytes(bc);
        if block_size == 0 || block_size > blk::MAX_IO_BYTES {
            return None;
        }

        Some(Self {
            pos: 0,
            size: block_count.saturating_mul(block_size as u64),
            block_size,
        })
    }

    fn read_sector(&self, sector: u64, out: &mut [u8]) -> bool {
        let mut req = [0u8; blk::RW_HDR_LEN];
        req[0..2].copy_from_slice(&blk::REQ_READ.to_le_bytes());
        req[2..10].copy_from_slice(&sector.to_le_bytes());
        req[10..14].copy_from_slice(&(out.len() as u32).to_le_bytes());
        let mut resp = [0u8; blk::MAX_IO_BYTES];
        let len = blk_call(&req, &mut resp);
        if len != out.len() {
            return false;
        }
        out.copy_from_slice(&resp[..len]);
        true
    }

    fn write_sector(&self, sector: u64, data: &[u8]) -> bool {
        let mut req = Vec::with_capacity(blk::RW_HDR_LEN + data.len());
        req.extend_from_slice(&blk::REQ_WRITE.to_le_bytes());
        req.extend_from_slice(&sector.to_le_bytes());
        req.extend_from_slice(&(data.len() as u32).to_le_bytes());
        req.extend_from_slice(data);
        let mut resp = [0u8; 8];
        let len = blk_call(&req, &mut resp);
        len == 2 && &resp[..2] == b"OK"
    }
}

impl axfatfs::IoBase for VirtioBlkDisk {
    type Error = ();
}

impl axfatfs::Read for VirtioBlkDisk {
    fn read(&mut self, mut buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut done = 0usize;
        let mut sector = [0u8; blk::MAX_IO_BYTES];
        while !buf.is_empty() && self.pos < self.size {
            let lba = self.pos / self.block_size as u64;
            let off = (self.pos as usize) % self.block_size;
            if !self.read_sector(lba, &mut sector[..self.block_size]) {
                return Err(());
            }

            let available_in_sector = self.block_size - off;
            let available_in_disk = (self.size - self.pos) as usize;
            let n = available_in_sector.min(buf.len()).min(available_in_disk);
            buf[..n].copy_from_slice(&sector[off..off + n]);

            self.pos += n as u64;
            done += n;
            let tmp = buf;
            buf = &mut tmp[n..];
        }
        Ok(done)
    }
}

impl axfatfs::Write for VirtioBlkDisk {
    fn write(&mut self, mut buf: &[u8]) -> Result<usize, Self::Error> {
        let mut done = 0usize;
        let mut sector = [0u8; blk::MAX_IO_BYTES];
        while !buf.is_empty() && self.pos < self.size {
            let lba = self.pos / self.block_size as u64;
            let off = (self.pos as usize) % self.block_size;
            if !self.read_sector(lba, &mut sector[..self.block_size]) {
                return Err(());
            }

            let available_in_sector = self.block_size - off;
            let available_in_disk = (self.size - self.pos) as usize;
            let n = available_in_sector.min(buf.len()).min(available_in_disk);
            sector[off..off + n].copy_from_slice(&buf[..n]);

            if !self.write_sector(lba, &sector[..self.block_size]) {
                return Err(());
            }

            self.pos += n as u64;
            done += n;
            buf = &buf[n..];
        }
        Ok(done)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        let req = blk::REQ_FLUSH.to_le_bytes();
        let mut resp = [0u8; 8];
        let len = blk_call(&req, &mut resp);
        if len == 2 && &resp[..2] == b"OK" {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl axfatfs::Seek for VirtioBlkDisk {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        let new_pos = match pos {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::Current(off) => self.pos.checked_add_signed(off),
            SeekFrom::End(off) => self.size.checked_add_signed(off),
        }
        .ok_or(())?;

        self.pos = new_pos.min(self.size);
        Ok(self.pos)
    }
}

fn blk_call(req: &[u8], resp: &mut [u8]) -> usize {
    let req_id = next_blk_req_id();
    let mut tagged = [0u8; blk::MAX_IO_BYTES + 32];
    if req.len() + blk::TAG_LEN > tagged.len() {
        return 0;
    }
    tagged[0..2].copy_from_slice(&req[0..2]);
    tagged[2..6].copy_from_slice(&req_id.to_le_bytes());
    tagged[6..6 + (req.len() - 2)].copy_from_slice(&req[2..]);
    let tagged_len = req.len() + blk::TAG_LEN;

    if !ulib::sys_ipc_send_checked(blk_service_handle(), &tagged[..tagged_len], Some(ulib::Handle::LocalEndpoint)) {
        return 0;
    }

    let mut inbox = [0u8; blk::MAX_IO_BYTES + 32];
    let mut from_pid = 0usize;
    let mut recv_cap = None;
    let mut tries = 0;
    while tries < 250 {
        let n = ulib::sys_ipc_recv_timeout(&mut from_pid, &mut inbox, &mut recv_cap, 20);
        if n < 0 {
            tries += 1;
            continue;
        }
        let n = n as usize;

        if let Some(cap) = recv_cap {
            // Request channel and blk reply channel share one local endpoint.
            // Push backpressure for unrelated client requests while waiting blk reply.
            ulib::sys_ipc_send(cap, b"EAGAIN", None);
            tries += 1;
            continue;
        }

        if n < blk::TAG_LEN {
            tries += 1;
            continue;
        }
        let tag = u32::from_le_bytes([inbox[0], inbox[1], inbox[2], inbox[3]]);
        if tag != req_id {
            tries += 1;
            continue;
        }

        let payload_len = n - blk::TAG_LEN;
        let copy = core::cmp::min(payload_len, resp.len());
        resp[..copy].copy_from_slice(&inbox[blk::TAG_LEN..blk::TAG_LEN + copy]);
        return copy;
    }
    0
}

fn normalize_path(path: &str) -> &str {
    let p = path.trim_start_matches('/');
    if p.is_empty() { "_" } else { p }
}

fn read_file_all(
    fs: &FileSystem<VirtioBlkDisk, NullTimeProvider, LossyOemCpConverter>,
    path: &str,
) -> Option<Vec<u8>> {
    let p = normalize_path(path);
    let root = fs.root_dir();
    let mut file = root.open_file(p).ok()?;
    let size = file.seek(SeekFrom::End(0)).ok()? as usize;
    file.seek(SeekFrom::Start(0)).ok()?;

    let mut out = Vec::with_capacity(size);
    let mut chunk = [0u8; blk::MAX_IO_BYTES];
    loop {
        let n = file.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
    }
    Some(out)
}

fn ensure_fs(fs: &mut Option<FsType>) -> bool {
    if fs.is_some() {
        return true;
    }
    let Some(disk) = VirtioBlkDisk::new() else {
        return false;
    };
    let Ok(inst) = FileSystem::<VirtioBlkDisk, NullTimeProvider, LossyOemCpConverter>::new(
        disk,
        FsOptions::new(),
    ) else {
        return false;
    };
    *fs = Some(inst);
    true
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::init_allocator();
    let _ = ulib::ctl_register_service("fs");
    ulib::sys_log("fs: service started (axfatfs)");
    let mut fs: Option<FsType> = None;

    let mut req_buf = [0u8; WRITE_HDR_LEN + MAX_PATH + MAX_DATA];
    let mut file_buf = [0u8; MAX_DATA];
    let mut from_pid = 0usize;

    loop {
        let mut reply_cap = None;
        let req_len = ulib::sys_ipc_recv(&mut from_pid, &mut req_buf, &mut reply_cap);
        if req_len < 2 {
            continue;
        }

        let raw_opcode = u16::from_le_bytes([req_buf[0], req_buf[1]]);
        let reqid_mode = (raw_opcode & FLAG_REQID) != 0;
        let opcode = raw_opcode & !FLAG_REQID;
        let req_id = if reqid_mode {
            if opcode == REQ_WRITE && req_len >= WRITE_HDR_LEN_V2 {
                Some(u32::from_le_bytes([req_buf[8], req_buf[9], req_buf[10], req_buf[11]]))
            } else if req_len >= PATH_HDR_LEN_V2 {
                Some(u32::from_le_bytes([req_buf[4], req_buf[5], req_buf[6], req_buf[7]]))
            } else {
                None
            }
        } else {
            None
        };

        let reply = |cap: ulib::Handle, payload: &[u8]| {
            if let Some(id) = req_id {
                let mut out = [0u8; RESP_REQID_LEN + MAX_DATA + 16];
                out[0..4].copy_from_slice(&id.to_le_bytes());
                let n = payload.len();
                out[4..4 + n].copy_from_slice(payload);
                ulib::sys_ipc_send(cap, &out[..4 + n], None);
            } else {
                ulib::sys_ipc_send(cap, payload, None);
            }
        };

        let path_hdr_len = if reqid_mode { PATH_HDR_LEN_V2 } else { PATH_HDR_LEN };
        let write_hdr_len = if reqid_mode {
            WRITE_HDR_LEN_V2
        } else {
            WRITE_HDR_LEN
        };
        match opcode {
            REQ_PING => {
                if let Some(cap) = reply_cap {
                    reply(cap, b"PONG");
                }
            }
            REQ_WRITE => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        reply(cap, b"EAGAIN");
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < write_hdr_len {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    let data_len = u32::from_le_bytes([req_buf[4], req_buf[5], req_buf[6], req_buf[7]]) as usize;
                    if path_len == 0
                        || path_len > MAX_PATH
                        || data_len > MAX_DATA
                        || req_len < write_hdr_len + path_len + data_len
                    {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_bytes = &req_buf[write_hdr_len..write_hdr_len + path_len];
                    let data = &req_buf[write_hdr_len + path_len..write_hdr_len + path_len + data_len];
                    match core::str::from_utf8(path_bytes) {
                        Ok(path) => {
                            let p = normalize_path(path);
                            let root = fs.root_dir();
                            let mut file = match root.open_file(p) {
                                Ok(f) => f,
                                Err(_) => match root.create_file(p) {
                                    Ok(f) => f,
                                    Err(_) => {
                                        reply(cap, b"EIO");
                                        continue;
                                    }
                                },
                            };
                            if file.seek(SeekFrom::Start(0)).is_err()
                                || file.truncate().is_err()
                                || file.write(data).is_err()
                                || file.flush().is_err()
                            {
                                reply(cap, b"EIO");
                            } else {
                                reply(cap, b"OK");
                            }
                        }
                        Err(_) => {
                            reply(cap, b"EINVAL")
                        }
                    }
                }
            }
            REQ_READ => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        reply(cap, b"EAGAIN");
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < path_hdr_len {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    if path_len == 0 || path_len > MAX_PATH || req_len < path_hdr_len + path_len {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_bytes = &req_buf[path_hdr_len..path_hdr_len + path_len];
                    match core::str::from_utf8(path_bytes) {
                        Ok(path) => {
                            let p = normalize_path(path);
                            let root = fs.root_dir();
                            let mut file = match root.open_file(p) {
                                Ok(f) => f,
                                Err(_) => {
                                    reply(cap, b"ENOENT");
                                    continue;
                                }
                            };

                            if file.seek(SeekFrom::Start(0)).is_err() {
                                reply(cap, b"EIO");
                                continue;
                            }

                            let n = match file.read(&mut file_buf) {
                                Ok(v) => v,
                                Err(_) => {
                                    reply(cap, b"EIO");
                                    continue;
                                }
                            };
                            if n == 0 {
                                reply(cap, b"ENOENT");
                            } else {
                                reply(cap, &file_buf[..n]);
                            }
                        }
                        Err(_) => {
                            reply(cap, b"EINVAL")
                        }
                    }
                }
            }
            REQ_DELETE => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        reply(cap, b"EAGAIN");
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < path_hdr_len {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    if path_len == 0 || path_len > MAX_PATH || req_len < path_hdr_len + path_len {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_bytes = &req_buf[path_hdr_len..path_hdr_len + path_len];
                    match core::str::from_utf8(path_bytes) {
                        Ok(path) => {
                            let p = normalize_path(path);
                            let root = fs.root_dir();
                            if root.remove(p).is_ok() {
                                reply(cap, b"OK");
                            } else {
                                reply(cap, b"ENOENT");
                            }
                        }
                        Err(_) => reply(cap, b"EINVAL"),
                    }
                }
            }
            REQ_EXEC => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        reply(cap, b"EAGAIN");
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < path_hdr_len {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    if path_len == 0 || path_len > MAX_PATH || req_len < path_hdr_len + path_len {
                        reply(cap, b"EINVAL");
                        continue;
                    }

                    let path_bytes = &req_buf[path_hdr_len..path_hdr_len + path_len];
                    let Ok(path) = core::str::from_utf8(path_bytes) else {
                        reply(cap, b"EINVAL");
                        continue;
                    };

                    let Some(elf) = read_file_all(fs, path) else {
                        reply(cap, b"ENOENT");
                        continue;
                    };

                    let pid = ulib::sys_spawn_elf(path, &elf);
                    if pid != 0 {
                        reply(cap, b"OK");
                    } else {
                        reply(cap, b"EIO");
                    }
                }
            }
            _ => {
                if let Some(cap) = reply_cap {
                    reply(cap, b"EINVAL");
                }
            }
        }
    }
}
