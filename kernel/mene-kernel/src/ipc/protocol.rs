#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IpcHeader {
    pub opcode: u16,
    pub flags: u16,
    pub payload_len: u32,
}

impl IpcHeader {
    pub const SIZE: usize = 8;

    pub const fn new(opcode: u16, payload_len: u32) -> Self {
        Self {
            opcode,
            flags: 0,
            payload_len,
        }
    }

    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            opcode: u16::from_le_bytes([buf[0], buf[1]]),
            flags: u16::from_le_bytes([buf[2], buf[3]]),
            payload_len: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..2].copy_from_slice(&self.opcode.to_le_bytes());
        buf[2..4].copy_from_slice(&self.flags.to_le_bytes());
        buf[4..8].copy_from_slice(&self.payload_len.to_le_bytes());
        buf
    }
}

pub const FLAG_REPLY_EXPECTED: u16 = 0x0001;
pub const FLAG_ERROR: u16 = 0x0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    InvalidHeader,
    PayloadTooLarge,
    InvalidOpcode,
    NotFound,
    PermissionDenied,
}
