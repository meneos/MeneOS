pub mod protocol;

pub use protocol::{IpcHeader, IpcError, FLAG_REPLY_EXPECTED, FLAG_ERROR};
pub use mene_ipc::IpcManager;
