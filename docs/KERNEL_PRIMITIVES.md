# 内核原语接口最小集合

本文档定义 MeneOS 微内核 Core 层应该暴露的最小原语接口集合。

## 设计原则

1. **最小化原则**：只提供机制，不提供策略
2. **正交性原则**：每个原语功能独立，不重叠
3. **组合性原则**：复杂功能通过组合原语实现
4. **稳定性原则**：原语接口应保持长期稳定

---

## 1. 线程调度原语

### 职责范围
提供线程创建、调度、上下文切换的基础能力。

### 核心接口
```rust
// 创建线程（内核内部使用）
fn create_thread(entry: VirtAddr, stack: VirtAddr, page_table_root: PhysAddr) -> ThreadId;

// 让出 CPU
fn thread_yield();

// 休眠指定时间
fn thread_sleep(duration: Duration);

// 退出当前线程
fn thread_exit(exit_code: i32) -> !;
```

### 不包含的功能
- 进程管理（由 Control Plane 负责）
- 线程优先级策略（由 Control Plane 负责）
- 线程组/进程组概念（由 Control Plane 负责）

---

## 2. IPC 原语

### 职责范围
提供端点创建、消息传递、能力传递的基础能力。

### 核心接口
```rust
// 创建 IPC 端点
fn ipc_create_endpoint() -> EndpointId;

// 发送消息（可选传递能力）
fn ipc_send(endpoint: EndpointId, data: &[u8], cap: Option<Capability>) -> Result<()>;

// 接收消息（可选接收能力）
fn ipc_recv(buffer: &mut [u8]) -> Result<(usize, Option<Capability>)>;

// 带超时的接收
fn ipc_recv_timeout(buffer: &mut [u8], timeout: Duration) -> Result<(usize, Option<Capability>)>;

// 销毁端点
fn ipc_destroy_endpoint(endpoint: EndpointId);
```

### 不包含的功能
- 服务发现（由 Control Plane 的 Service Registry 负责）
- 协议定义（由 User Services 层定义）
- 重试/超时策略（由上层封装）

---

## 3. 地址空间原语

### 职责范围
提供页表映射、解映射、地址空间切换的基础能力。

### 核心接口
```rust
// 创建新地址空间
fn aspace_create() -> AddrSpaceId;

// 映射物理页到虚拟地址
fn aspace_map(aspace: AddrSpaceId, vaddr: VirtAddr, paddr: PhysAddr,
              size: usize, flags: MappingFlags) -> Result<()>;

// 解映射虚拟地址
fn aspace_unmap(aspace: AddrSpaceId, vaddr: VirtAddr, size: usize) -> Result<()>;

// 分配并映射匿名页
fn aspace_map_alloc(aspace: AddrSpaceId, vaddr: VirtAddr, size: usize,
                    flags: MappingFlags) -> Result<()>;

// 查询虚拟地址映射
fn aspace_query(aspace: AddrSpaceId, vaddr: VirtAddr) -> Result<(PhysAddr, MappingFlags)>;

// 销毁地址空间
fn aspace_destroy(aspace: AddrSpaceId);
```

### 不包含的功能
- 内存布局策略（由 Control Plane 的 VMM Policy 负责）
- 内存配额管理（由 Control Plane 负责）
- 共享内存区域管理（由 Control Plane 负责）

---

## 4. 中断/陷入原语

### 职责范围
提供中断注册、陷入分发的基础能力。

### 核心接口
```rust
// 注册中断处理器
fn irq_register(irq_num: usize, handler: IrqHandler) -> Result<()>;

// 注销中断处理器
fn irq_unregister(irq_num: usize);

// 启用/禁用中断
fn irq_enable(irq_num: usize);
fn irq_disable(irq_num: usize);

// 系统调用分发（内部使用）
fn syscall_dispatch(uctx: &mut UserContext, pid: usize, aspace: &AddrSpace);
```

### 不包含的功能
- 中断路由策略（由 Control Plane 的 Device Manager 负责）
- 中断亲和性设置（由 Control Plane 负责）

---

## 5. 能力管理原语

### 职责范围
提供能力空间管理的基础能力。

### 核心接口
```rust
// 创建能力空间
fn cspace_create() -> CSpaceId;

// 插入能力到能力空间
fn cspace_insert(cspace: CSpaceId, handle: Handle, cap: Capability) -> Result<()>;

// 从能力空间查找能力
fn cspace_lookup(cspace: CSpaceId, handle: Handle) -> Result<Capability>;

// 从能力空间移除能力
fn cspace_remove(cspace: CSpaceId, handle: Handle) -> Result<Capability>;

// 销毁能力空间
fn cspace_destroy(cspace: CSpaceId);
```

### 不包含的功能
- 能力授权策略（由 Control Plane 负责）
- Bootstrap 能力注入（由 Control Plane 负责）

---

## 系统调用接口映射

### Mene 原生系统调用

| 系统调用 | 对应原语 | 层次 |
|---------|---------|------|
| `spawn` | 无直接对应 | Control Plane |
| `spawn_elf` | 无直接对应 | Control Plane |
| `ipc_send` | `ipc_send` | Core |
| `ipc_recv` | `ipc_recv` | Core |
| `ipc_recv_timeout` | `ipc_recv_timeout` | Core |
| `mmap_anon` | `aspace_map_alloc` | Core |
| `map_device` | `aspace_map` | Core |
| `vmm_map_page_to` | 无直接对应 | Control Plane |
| `dma_alloc` | `aspace_map_alloc` + 物理地址查询 | Core |
| `dma_dealloc` | `aspace_unmap` | Core |
| `virt_to_phys` | `aspace_query` | Core |
| `sleep_ms` | `thread_sleep` | Core |
| `exit` | `thread_exit` | Core |

### Linux 兼容系统调用

| 系统调用 | 对应原语 | 层次 |
|---------|---------|------|
| `getpid` | 查询当前 PID | Core |
| `exit` / `exit_group` | `thread_exit` | Core |
| `mmap` | `aspace_map_alloc` | Core |
| `munmap` | `aspace_unmap` | Core |
| `write` | IPC 到 serial 服务 | User Services |
| `writev` | IPC 到 serial 服务 | User Services |

---

## 接口稳定性保证

### 版本管理
- 原语接口使用语义化版本号
- 主版本号变更表示不兼容的接口变更
- 次版本号变更表示向后兼容的功能增加
- 修订版本号变更表示向后兼容的 bug 修复

### 废弃流程
1. 标记接口为 `#[deprecated]`
2. 至少保留 2 个主版本周期
3. 提供迁移指南
4. 在新主版本中移除

---

## 与 ArceOS 的边界

### MeneOS Core 依赖的 ArceOS 原语
- `axhal`: 硬件抽象层（页表、中断、上下文切换）
- `axmm`: 内存管理（物理页分配、地址空间）
- `axtask`: 任务调度（线程创建、调度器）
- `axsync`: 同步原语（Mutex、Spinlock）

### 封装原则
- MeneOS Core 不直接暴露 ArceOS 接口给上层
- 所有 ArceOS 功能通过 MeneOS 原语封装
- 保持 MeneOS 原语接口的稳定性，即使 ArceOS 变更

---

## 检查清单

在添加新原语前，确认：
- [ ] 该功能无法通过组合现有原语实现
- [ ] 该功能属于机制而非策略
- [ ] 该功能在 Core 层实现比在上层实现更高效
- [ ] 该接口设计足够通用，不绑定特定使用场景
- [ ] 已考虑接口的长期稳定性
- [ ] 已更新本文档
