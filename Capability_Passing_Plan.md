# MeneOS Capability Passing (句柄传递) 改造规划

为了将 MeneOS 升级为现代微内核架构（类似 seL4 / Zircon），彻底消除基于全局 PID 的寻址，转而使用面向对象的能力/句柄（Capability/Handle）机制，特制定以下四步重构计划：

## 第一步：引入 CSpace (句柄表) 和 Endpoint (通信端点)
- **目标**：在内核中摒弃全局的基于 PID 的消息队列，引入对象概念。
- **具体实现**：
  1. 定义 `Endpoint` 对象，内部包含用于该端点的消息队列。
  2. 定义 `Capability` 枚举，例如 `Capability::Endpoint(Arc<Endpoint>)`。
  3. 修改内核中的 `ProcessInfo` 结构体，为每个进程添加私有的句柄表 `cspace: Mutex<BTreeMap<usize, Capability>>`。

## 第二步：修改 IPC 系统调用（从 PID 升级为 Handle）
- **目标**：用户态通信不再"指名道姓"，只对本地句柄操作。
- **具体实现**：
  1. 重构 `mene-abi` 和 `mene-syscall`。
  2. 将 `sys_ipc_send(target_pid: usize, msg)` 修改为 `sys_ipc_send(handle: usize, msg)`。
  3. 内核处理时，根据当前进程的 `cspace` 检查传入的 `handle` 是否合法，若合法且为 `Endpoint`，则将消息压入该端点的队列中。

## 第三步：解决启动与发现问题（权能注入）
- **目标**：解决如果没有公共 Name Server 和固定 PID，新进程如何获取基础服务通信权限的问题。
- **具体实现**：
  1. `init` 祖宗进程在包办一切时预先创建好各服务的通讯端点（如 `ep_serial`, `ep_vmm`）。
  2. 启动 `serial`、`vmm` 服务时，将相应端点的**接收权**注入到它们的句柄表中。
  3. 启动应用进程（如 `helloworld`）时，强制将 `ep_serial` 和 `ep_vmm` 的**发送权**分配给其句柄表中的固定的本地句柄号（例如 handle 2 是 serial，handle 3 是 vmm）。

## 第四步：运行时能力传递 (Capability Passing)
- **目标**：实现进程间动态授予权限。
- **具体实现**：
  1. 扩展 IPC 系统调用，允许在发送消息的同时附带 Capability 句柄。
  2. 内核在投递消息时，自动将被发送的能力复制到接收进程的 `cspace` 中。
  3. 通过这一机制，天生实现极致的隔离性和安全性（无句柄则绝对无法访问某服务）。
