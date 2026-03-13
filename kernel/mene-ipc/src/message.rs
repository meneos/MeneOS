//! IPC 消息相关的数据结构定义。
//!
//! 在微内核设计中，IPC 消息分为两种基本类型：
//! - **短消息 (Short Message)**：用于传递少量控制信息（如系统调用号或少量参数），
//!   在底层实现中通常直接通过 CPU 寄存器传递，以获得极致的性能。
//! - **长消息 (Long Message)**：用于传递大数据（如文件缓冲、网络包等），
//!   通常通过预先协商的共享内存 (Shared Memory) 页进行传输，避免频繁的内存拷贝。

/// 短消息结构体。
///
/// 模拟通过 CPU 寄存器直接传递的小型数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortMessage {
    /// 消息类型或系统调用号。
    pub msg_type: u64,
    /// 固定数量的参数（对应寄存器数量）。
    pub args: [u64; 4],
}

impl ShortMessage {
    /// 创建一条新的短消息。
    ///
    /// # 参数
    ///
    /// * `msg_type`: 消息类型标识符。
    /// * `args`: 最多包含 4 个 64 位整型的参数组。
    #[inline]
    pub const fn new(msg_type: u64, args: [u64; 4]) -> Self {
        Self { msg_type, args }
    }
}

/// 长消息结构体。
///
/// 用于描述通过共享内存传递的消息引用。通常传递的是页地址或缓冲区的描述信息。
#[derive(Debug, Clone)]
pub struct LongMessage {
    /// 指向共享内存缓冲区起始地址的指针/偏移量。
    pub buffer_addr: usize,
    /// 缓冲区大小（以字节为单位）。
    pub buffer_len: usize,
}

impl LongMessage {
    /// 创建一条长消息描述符。
    ///
    /// # 参数
    ///
    /// * `buffer_addr`: 共享内存缓冲区的起始地址。
    /// * `buffer_len`: 缓冲区的字节长度。
    #[inline]
    pub const fn new(buffer_addr: usize, buffer_len: usize) -> Self {
        Self {
            buffer_addr,
            buffer_len,
        }
    }
}

/// 综合的 IPC 消息枚举。
///
/// 封装了短消息和长消息的变体，便于在发送和接收端口中进行统一处理。
#[derive(Debug, Clone)]
pub enum Message {
    /// 包含短消息数据。
    Short(ShortMessage),
    /// 包含长消息引用。
    Long(LongMessage),
}

impl From<ShortMessage> for Message {
    #[inline]
    fn from(msg: ShortMessage) -> Self {
        Message::Short(msg)
    }
}

impl From<LongMessage> for Message {
    #[inline]
    fn from(msg: LongMessage) -> Self {
        Message::Long(msg)
    }
}
