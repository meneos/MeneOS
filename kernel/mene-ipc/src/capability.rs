//! 权能传递 (Capability Transfer) 相关定义。
//! 
//! 在微内核中，IPC 不仅可以用于传递数据，还可以用于安全地传递对系统资源
//! （如内存页、设备 I/O 端口、系统对象等）的访问权限。这种基于授权的访问控制模型称为权能（Capability）。

/// 定义不同种类的系统核心资源权能。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityType {
    /// 内存页映射或授权，允许访问特定的物理或虚拟内存页。
    MemoryPage,
    /// 硬件设备硬件中断或 I/O 端口权限。
    IoPort,
    /// 内核对象句柄的使用权（如进程、线程或 IPC 端点的句柄）。
    ObjectHandle,
}

/// 权能结构体，描述某项特定系统资源的具体访问授权。
#[derive(Debug, Clone)]
pub struct Capability {
    /// 此权能所归属的资源种类。
    pub cap_type: CapabilityType,
    /// 资源关联的目标标识符（如物理地址起址或系统分配的对象 ID）。
    pub identifier: u64,
    /// 该资源附带的读写执行等权限掩码，具体位定义根据上下文决定。
    pub permissions: u32,
    /// 授权资源的规模属性（例如代表内存时表示被授权的页面数量）。
    pub size: usize,
}

impl Capability {
    /// 构造一个新的权能传递对象。
    ///
    /// # 参数
    ///
    /// * `cap_type`: 权能代表的基础资源种类类型。
    /// * `identifier`: 目标系统资源的对象标识号或地址。
    /// * `permissions`: 赋予使用者的权限控制位。
    /// * `size`: 授权的延伸规模或数量。
    #[inline]
    pub const fn new(
        cap_type: CapabilityType,
        identifier: u64,
        permissions: u32,
        size: usize,
    ) -> Self {
        Self {
            cap_type,
            identifier,
            permissions,
            size,
        }
    }
}
