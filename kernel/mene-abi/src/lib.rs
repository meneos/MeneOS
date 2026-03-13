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
    VmmMapPageTo = 505,
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
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    LocalEndpoint,
    SerialEndpoint,
    VmmEndpoint,
    Dynamic(usize),
}

impl Handle {
    pub const LOCAL_ENDPOINT: usize = 1;
    pub const SERIAL_ENDPOINT: usize = 2;
    pub const VMM_ENDPOINT: usize = 3;

    pub fn to_usize(&self) -> usize {
        match self {
            Handle::LocalEndpoint => Self::LOCAL_ENDPOINT,
            Handle::SerialEndpoint => Self::SERIAL_ENDPOINT,
            Handle::VmmEndpoint => Self::VMM_ENDPOINT,
            Handle::Dynamic(v) => *v,
        }
    }

    pub fn from_usize(val: usize) -> Self {
        match val {
            Self::LOCAL_ENDPOINT => Handle::LocalEndpoint,
            Self::SERIAL_ENDPOINT => Handle::SerialEndpoint,
            Self::VMM_ENDPOINT => Handle::VmmEndpoint,
            _ => Handle::Dynamic(val),
        }
    }
}
