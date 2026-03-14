#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use axfatfs::{
    FileSystem, FsOptions, LossyOemCpConverter, NullTimeProvider, Read, Seek, SeekFrom, Write,
};
use ulib::blk;
use ulib::fs::{
    MAX_DATA, MAX_PATH, PATH_HDR_LEN, REQ_DELETE, REQ_EXEC, REQ_PING, REQ_READ, REQ_WRITE,
    WRITE_HDR_LEN,
};

type FsType = FileSystem<VirtioBlkDisk, NullTimeProvider, LossyOemCpConverter>;

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
    if !ulib::sys_ipc_send_checked(
        ulib::Handle::VirtioBlkEndpoint,
        req,
        Some(ulib::Handle::LocalEndpoint),
    ) {
        return 0;
    }
    let mut from_pid = 0usize;
    let mut recv_cap = None;
    ulib::sys_ipc_recv(&mut from_pid, resp, &mut recv_cap)
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
    let mut chunk = [0u8; 512];
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

        let opcode = u16::from_le_bytes([req_buf[0], req_buf[1]]);
        match opcode {
            REQ_PING => {
                if let Some(cap) = reply_cap {
                    ulib::sys_ipc_send(cap, b"PONG", None);
                }
            }
            REQ_WRITE => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        ulib::sys_ipc_send(cap, b"EAGAIN", None);
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < WRITE_HDR_LEN {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    let data_len = u32::from_le_bytes([req_buf[4], req_buf[5], req_buf[6], req_buf[7]]) as usize;
                    if path_len == 0
                        || path_len > MAX_PATH
                        || data_len > MAX_DATA
                        || req_len < WRITE_HDR_LEN + path_len + data_len
                    {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_bytes = &req_buf[WRITE_HDR_LEN..WRITE_HDR_LEN + path_len];
                    let data = &req_buf[WRITE_HDR_LEN + path_len..WRITE_HDR_LEN + path_len + data_len];
                    match core::str::from_utf8(path_bytes) {
                        Ok(path) => {
                            let p = normalize_path(path);
                            let root = fs.root_dir();
                            let mut file = match root.open_file(p) {
                                Ok(f) => f,
                                Err(_) => match root.create_file(p) {
                                    Ok(f) => f,
                                    Err(_) => {
                                        ulib::sys_ipc_send(cap, b"EIO", None);
                                        continue;
                                    }
                                },
                            };
                            if file.seek(SeekFrom::Start(0)).is_err()
                                || file.truncate().is_err()
                                || file.write(data).is_err()
                                || file.flush().is_err()
                            {
                                ulib::sys_ipc_send(cap, b"EIO", None);
                            } else {
                                ulib::sys_ipc_send(cap, b"OK", None);
                            }
                        }
                        Err(_) => {
                            ulib::sys_ipc_send(cap, b"EINVAL", None)
                        }
                    }
                }
            }
            REQ_READ => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        ulib::sys_ipc_send(cap, b"EAGAIN", None);
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < PATH_HDR_LEN {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    if path_len == 0 || path_len > MAX_PATH || req_len < PATH_HDR_LEN + path_len {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_bytes = &req_buf[PATH_HDR_LEN..PATH_HDR_LEN + path_len];
                    match core::str::from_utf8(path_bytes) {
                        Ok(path) => {
                            let p = normalize_path(path);
                            let root = fs.root_dir();
                            let mut file = match root.open_file(p) {
                                Ok(f) => f,
                                Err(_) => {
                                    ulib::sys_ipc_send(cap, b"ENOENT", None);
                                    continue;
                                }
                            };

                            if file.seek(SeekFrom::Start(0)).is_err() {
                                ulib::sys_ipc_send(cap, b"EIO", None);
                                continue;
                            }

                            let n = match file.read(&mut file_buf) {
                                Ok(v) => v,
                                Err(_) => {
                                    ulib::sys_ipc_send(cap, b"EIO", None);
                                    continue;
                                }
                            };
                            if n == 0 {
                                ulib::sys_ipc_send(cap, b"ENOENT", None);
                            } else {
                                ulib::sys_ipc_send(cap, &file_buf[..n], None);
                            }
                        }
                        Err(_) => {
                            ulib::sys_ipc_send(cap, b"EINVAL", None)
                        }
                    }
                }
            }
            REQ_DELETE => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        ulib::sys_ipc_send(cap, b"EAGAIN", None);
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < PATH_HDR_LEN {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    if path_len == 0 || path_len > MAX_PATH || req_len < PATH_HDR_LEN + path_len {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_bytes = &req_buf[PATH_HDR_LEN..PATH_HDR_LEN + path_len];
                    match core::str::from_utf8(path_bytes) {
                        Ok(path) => {
                            let p = normalize_path(path);
                            let root = fs.root_dir();
                            if root.remove(p).is_ok() {
                                ulib::sys_ipc_send(cap, b"OK", None);
                            } else {
                                ulib::sys_ipc_send(cap, b"ENOENT", None);
                            }
                        }
                        Err(_) => ulib::sys_ipc_send(cap, b"EINVAL", None),
                    }
                }
            }
            REQ_EXEC => {
                if let Some(cap) = reply_cap {
                    if !ensure_fs(&mut fs) {
                        ulib::sys_ipc_send(cap, b"EAGAIN", None);
                        continue;
                    }
                    let fs = fs.as_ref().unwrap();

                    if req_len < PATH_HDR_LEN {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_len = u16::from_le_bytes([req_buf[2], req_buf[3]]) as usize;
                    if path_len == 0 || path_len > MAX_PATH || req_len < PATH_HDR_LEN + path_len {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    }

                    let path_bytes = &req_buf[PATH_HDR_LEN..PATH_HDR_LEN + path_len];
                    let Ok(path) = core::str::from_utf8(path_bytes) else {
                        ulib::sys_ipc_send(cap, b"EINVAL", None);
                        continue;
                    };

                    let Some(elf) = read_file_all(fs, path) else {
                        ulib::sys_ipc_send(cap, b"ENOENT", None);
                        continue;
                    };

                    let pid = ulib::sys_spawn_elf(path, &elf);
                    if pid != 0 {
                        ulib::sys_ipc_send(cap, b"OK", None);
                    } else {
                        ulib::sys_ipc_send(cap, b"EIO", None);
                    }
                }
            }
            _ => {
                if let Some(cap) = reply_cap {
                    ulib::sys_ipc_send(cap, b"EINVAL", None);
                }
            }
        }
    }
}
