#![no_std]

pub use syscalls::Sysno;

pub mod blk {
    pub const REQ_PING: u16 = 1;
    pub const REQ_GET_INFO: u16 = 2;
    pub const REQ_READ: u16 = 3;
    pub const REQ_WRITE: u16 = 4;
    pub const REQ_FLUSH: u16 = 5;

    pub const RW_HDR_LEN: usize = 14; // opcode(2) + sector(8) + bytes(4)
    pub const MAX_IO_BYTES: usize = 4096;
}

pub mod fs {
    pub const REQ_PING: u16 = 0;
    pub const REQ_WRITE: u16 = 1;
    pub const REQ_READ: u16 = 2;
    pub const REQ_DELETE: u16 = 3;
    pub const REQ_EXEC: u16 = 4;

    pub const MAX_PATH: usize = 128;
    pub const MAX_DATA: usize = 512;
    pub const WRITE_HDR_LEN: usize = 8; // opcode(2) + path_len(2) + data_len(4)
    pub const PATH_HDR_LEN: usize = 4; // opcode(2) + path_len(2)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum MeneSysno {
    Spawn = 500,
    IpcSend = 501,
    IpcRecv = 502,
    ReadFile = 503,
    MapDevice = 504,
    VmmMapPageTo = 505,
    DmaAlloc = 506,
    PciCfgRead = 507,
    DmaDealloc = 508,
    VirtToPhys = 509,
    SleepMs = 510,
    SystemOff = 511,
    GetBootCfg = 512,
    SpawnElf = 513,
    MmapAnon = 514,
}

impl core::convert::TryFrom<usize> for MeneSysno {
    type Error = ();
    fn try_from(val: usize) -> Result<Self, Self::Error> {
        match val {
            500 => Ok(Self::Spawn),
            501 => Ok(Self::IpcSend),
            502 => Ok(Self::IpcRecv),
            503 => Ok(Self::ReadFile),
            504 => Ok(Self::MapDevice),
            505 => Ok(Self::VmmMapPageTo),
            506 => Ok(Self::DmaAlloc),
            507 => Ok(Self::PciCfgRead),
            508 => Ok(Self::DmaDealloc),
            509 => Ok(Self::VirtToPhys),
            510 => Ok(Self::SleepMs),
            511 => Ok(Self::SystemOff),
            512 => Ok(Self::GetBootCfg),
            513 => Ok(Self::SpawnElf),
            514 => Ok(Self::MmapAnon),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    LocalEndpoint,
    SerialEndpoint,
    VmmEndpoint,
    VirtioBlkEndpoint,
    FsEndpoint,
    Dynamic(usize),
}

impl Handle {
    pub const LOCAL_ENDPOINT: usize = 1;
    pub const SERIAL_ENDPOINT: usize = 2;
    pub const VMM_ENDPOINT: usize = 3;
    pub const VIRTIO_BLK_ENDPOINT: usize = 4;
    pub const FS_ENDPOINT: usize = 5;

    pub fn to_usize(&self) -> usize {
        match self {
            Handle::LocalEndpoint => Self::LOCAL_ENDPOINT,
            Handle::SerialEndpoint => Self::SERIAL_ENDPOINT,
            Handle::VmmEndpoint => Self::VMM_ENDPOINT,
            Handle::VirtioBlkEndpoint => Self::VIRTIO_BLK_ENDPOINT,
            Handle::FsEndpoint => Self::FS_ENDPOINT,
            Handle::Dynamic(v) => *v,
        }
    }

    pub fn from_usize(val: usize) -> Self {
        match val {
            Self::LOCAL_ENDPOINT => Handle::LocalEndpoint,
            Self::SERIAL_ENDPOINT => Handle::SerialEndpoint,
            Self::VMM_ENDPOINT => Handle::VmmEndpoint,
            Self::VIRTIO_BLK_ENDPOINT => Handle::VirtioBlkEndpoint,
            Self::FS_ENDPOINT => Handle::FsEndpoint,
            _ => Handle::Dynamic(val),
        }
    }
}
