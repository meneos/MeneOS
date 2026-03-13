//! 同步阻塞式 IPC 通信端点 (Synchronous Blocking IPC Endpoint)。
//!
//! 端点（Endpoint）是微内核进程间通信的核心锚点。系统服务或常规进程
//! 可以向特定端点发送消息，或者从端点处接收消息。
//!
//! 在同步阻塞模型中：
//! - **发送方**投递消息后会陷入阻塞状态，直到接收方准备好并真正取走该消息。
//! - **接收方**若发现当前端点暂无就绪的消息，也会陷入阻塞状态，直至有发送方进入。
//! 本模块主要提供 IPC 的数据投递载荷及端点的通用定义与接口约定。

use alloc::collections::VecDeque;

use crate::capability::Capability;
use crate::message::Message;

/// IPC 传输全尺寸载荷 (IPC Payload)。
///
/// 一次完整的 IPC 操作不仅能发送包含控制信号或共享缓冲区引用的数据消息，
/// 还能连带执行安全可靠的系统资源权能传递。
#[derive(Debug, Clone)]
pub struct IpcPayload {
    /// 被传输的核心数据消息实体（可为短消息，或长消息的内存索引）。
    pub message: Message,
    /// 伴随此消息一并下发的资源访问权能链表。若此通信不涉及系统权限调度，则列表为空。
    pub capabilities: VecDeque<Capability>,
    /// 负责此次发送的线程或进程在操作系统中的全局唯一标识符 (ID)。
    pub sender_id: u64,
}

impl IpcPayload {
    /// 构造一个新的、不附带任何权能转移的基础 IPC 载荷。
    pub fn new(message: Message, sender_id: u64) -> Self {
        Self {
            message,
            capabilities: VecDeque::new(),
            sender_id,
        }
    }

    /// 向当前发送载荷中追加一条待转移的系统访问权能。
    pub fn add_capability(&mut self, cap: Capability) {
        self.capabilities.push_back(cap);
    }
}

/// IPC 端点 (Endpoint)。
///
/// 作为通信信道的收发核心，微内核依赖调度器向端点存取 `IpcPayload` 并执行线程切换：
/// 如果通过端点调用 `send()`，而目标进程并未处于等待状态，发送方线程实体由于阻塞将挂起；
/// 只有当接收者端发起对应的 `recv()` 并拉取载荷后，发送方方能因应答苏醒。
///
/// *注意：端点内的阻塞队列管理与具体任务抢占调度策略（TCB block/wakeup）高度挂钩，此处仅提供基础业务逻辑承载。*
pub struct Endpoint {
    /// 为全局通信网络分配的端点 ID 标识。
    pub endpoint_id: u64,
    /// 等待被处理或者正处于阻塞排队状态的通信包缓冲队列。
    payload_queue: VecDeque<IpcPayload>,
}

impl Endpoint {
    /// 构造初始化一只全新且空闲的 IPC 端点容器。
    ///
    /// # 参数
    ///
    /// * `id`: 新分配的系统端点总编号。
    pub fn new(id: u64) -> Self {
        Self {
            endpoint_id: id,
            payload_queue: VecDeque::new(),
        }
    }

    /// 尝试向端口内同步投递一条载荷数据（发送端模型）。
    ///
    /// # 参数
    /// 
    /// * `payload`: 需要递送给对应端点背后持有服务者的完整载荷包裹。
    pub fn send(&mut self, payload: IpcPayload) {
        // 在严肃的微内核调度器中，若暂无对应接收方等待，发送线程应当由此引发休眠
        // 此抽象首先将信息压入通信队列中。
        self.payload_queue.push_back(payload);
    }

    /// 执行阻塞式提取的同步载荷（接收端模型）。
    ///
    /// 从当前端点缓冲顶部脱出首个通讯请求。无通信驻留时返回为空以使调用者执行实际的线程阻塞让出当前调度片。
    pub fn receive(&mut self) -> Option<IpcPayload> {
        self.payload_queue.pop_front()
    }
}
