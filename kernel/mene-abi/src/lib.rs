#![no_std]

pub use syscalls::Sysno;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum MeneSysno {
    Spawn = 500,
    IpcSend = 501,
    IpcRecv = 502,
    ReadFile = 503,
    MapDevice = 504,
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
            _ => Err(()),
        }
    }
}
