# 微内核核心设计 (The Kernel)

在微内核架构中，内核的设计必须遵循**极简（Minimalism）**原则。内核运行在最高特权级（Ring 0 / EL1），其核心职能是提供**机制（Mechanism）**，而将所有的**策略（Policy）**（如文件系统、网络协议、设备驱动）下放至用户态（Ring 3 / EL0）以独立服务进程的形式运行。

## 核心设计理念

1. **极小化内核态 (Minimalism)**：内核仅提供地址空间管理、线程调度、IPC 进程间通信和权能（Capability）机制，绝不包含 POSIX 等高层抽象。
2. **万物皆 IPC**：所有的系统服务（如 VFS、网络、设备驱动）均运行在用户态，用户程序通过端点（Endpoint）与这些服务进程通信。
3. **基于权能的安全机制 (Capability-based)**：任何对内核对象（Task, IPC Endpoint, Memory Page）的操作都需要持有相应的 Capability。

---

## 模块架构与职责

我们将系统切分为以下核心 crate：

### 1. \`mene-ipc\`: 进程间通信模块
IPC 是微内核的灵魂。
* **设计模式**：同步阻塞式 IPC（Synchronous Blocking IPC）。
* **结构抽象**：
  * **ShortMessage**: 通过 CPU 寄存器传递少量控制信息（如服务号、简单参数）。
  * **LongMessage**: 共享内存页大数据传输。
  * **Capability**: 访问控制的权限凭证。

### 2. \`mene-init\`: 内核初始化与系统自举
负责最基础的硬件环境搭建，并手动加载出第一个用户态进程 \`init\`。这是微内核的入口。

> 🔍 **【参考梳理】\`mene-init\` 阶段如何借鉴 \`Macro kernel\`：**
> 
> 在自举与微内核重构阶段，我们需要抽取 \`Macro kernel\` 的基础硬件原语和启动逻辑，剔除其宏内核特性。请在代码重构时严格参考以下具体文件：
> 
> 1. **内核入口点与早期配置 (Early Entry)**
>    * **参考文件**: \`Macro kernel/src/entry.rs\` 和 \`Macro kernel/src/config/*\`
>    * **提取功能**: 获取进入内核的第一段汇编与 Rust 环境自举。包括页表早期初始化映射所需的宏配置。
> 2. **ELF 文件加载与解析 (Loader)**
>    * **参考文件**: \`Macro kernel/src/mm/loader.rs\`
>    * **提取功能**: 重点复用该文件中解析 ELF Header 与 Program Header 的逻辑（例如 \`load_elf\` 或者各段映射的函数）。我们需要这部分代码来确定 \`boot/init\` 的 BSS 段如何清零、Text/Data 段如何提取并写入对应物理内存。
> 3. **初始用户态地址空间构建 (Address Space)**
>    * **参考文件**: \`Macro kernel/src/mm/aspace/mod.rs\` 以及 \`Macro kernel/src/mm/aspace/backend/\`
>    * **提取功能**: 提取其中最底层关于硬件页表映射（\`Map\`/\`Unmap\`）的操作，用于为第一个用户进程搭建内存空间。**抛弃**宏内核中为共享内存、文件映射或 Copy-on-Write 做的复杂页面错误（Page Fault）和异常处理逻辑。
> 4. **早期临时文件读取 (VFS Bootstrap)**
>    * **参考文件**: \`Macro kernel/src/file/fs.rs\` 和 \`Macro kernel/src/pseudofs/fs.rs\` 
>    * **提取功能**: 在微内核启动早期的 \`mene-init\` 阶段，微内核内部没有完整文件系统，但我们必须读取 \`disk.img\` 中的 \`boot/init\` 文件。我们可以从中提取并“退化”出一版临时的、**只读**、**直接挂载 FAT32** 的精简代码读取文件字节流，仅供自举加载使用。
> 5. **构建第一个任务与进入用户态 (TrapFrame & Return)**
>    * **参考文件**: \`Macro kernel/src/task/user.rs\`、\`Macro kernel/src/task/ops.rs\`、\`Macro kernel/src/task/mod.rs\`
>    * **提取功能**: 复用初始化新线程控制块（TCB）、在内核栈顶压入正确的 \`TrapFrame\` 上下文（含初始用户栈地址、PC指针、argc/argv参数）的代码。同时参考底层是如何调用陷入返回指令跨特权级（如 RISC-V 的 \`sret\` 等）平滑跳入 \`init\` 程序的入口点。

### 3. \`mene-syscall\`: 微内核系统调用层
微内核只保留最原语的陷入接口。系统调用处理函数的种类极少，仅包含 IPC、Task、Memory、Capability 操作。

> 🔍 **【参考梳理】\`mene-syscall\` 阶段如何借鉴 \`Macro kernel\`：**
> 
> 1. **系统调用注册与分发架构 (Syscall Dispatcher)**
>    * **参考文件**: \`Macro kernel/src/syscall/sys.rs\`、\`Macro kernel/src/syscall/mod.rs\`
>    * **提取功能**: 提取 \`syscall_dispatcher\` 分发框架（即根据寄存器中的 syscall_id 跳转到特定处理函数的大 \`match\` 表或函数指针数组）。**彻底清理并删除**所有现存的宏内核调用实现引用（诸如 fs、net、socket）。将其替换为我们要实现的 \`sys_ipc_send\`、\`sys_ipc_recv\`、\`sys_task_yield\`。
> 2. **基础任务资源生命周期 (Task Control)**
>    * **参考文件**: \`Macro kernel/src/syscall/task/clone.rs\` 和 \`Macro kernel/src/syscall/task/exit.rs\`
>    * **提取功能**: 这是创建和销毁任务的入口。当实现微内核自身的 \`sys_task_create\` 和 \`sys_task_exit\` 时，参考这些文件中关于内核资源的释放流程（如：解绑 TCB，何时归还内核栈，何时释放页表引用计数，如何清理调度器实体）。

### 4. \`boot/init\`: 第一个用户态程序 (Root Server)
* 它是微内核自举后启动的**首个用户态进程**，持有最高级别的 Root Capability。
* **核心职责**：接管操作系统启动接力棒，加载并启动各种具体驱动进程、文件系统服务进程 (VFS Server) 和进程管理（Name Server）。

---

> 💡 **后续开发守则**：
> 在开始代码重组时，仔细阅读参考目录。如果不知道如何编写底层分配、解析 ELF 或者上下文切换逻辑，请随时根据此文档的**文件路径**，利用全文搜索去 \`Macro kernel\` 中精准提取所需的组件，严禁引入带有业务策略的宏内核代码（如 sys_read, socket 等）。


1. 完善的 IPC (进程间通信) 机制
目前的 IPC 实现是基于单个全局的 IPC_MAILBOX，所有进程共享一个缓冲区，这在实际中是不可用的。

端点/通道 (Endpoints/Channels)：需要为进程建立专门的通信通道或端点，而不是全局的大杂烩。
同步与异步 IPC：目前的 Recv 在没有数据时使用了 yield_now() 进行低效轮询。需要实现真正的阻塞/唤醒机制（如 Wait/Notify 或基于事件的机制）。
消息传递寻址：缺乏目标服务的发现机制。微内核应该有一个命名服务（Name Server），或者进程之间必须具有对方的句柄（Handle/Capability）才能发送消息。目前的 sys_ipc_send 虽然有 _pid 参数但并未被使用。
共享内存 (Shared Memory)：对于大块数据（如文件系统、网络包）的传递，需要有基于页共享的 IPC 机制，而不是仅靠值拷贝。
2. 进程与线程管理 (Task/Process Management)
目前 spawn_app 是通过 ArceOS 的底层 spawn_task 启动 ELF，但微内核暴露给用户态的控制力极弱。

进程标识符 (PID/TID)：微内核层没有维护进程树或 PID 映射表。
生命周期管理：缺少 wait (等待子进程退出)、kill (强制终止) 等系统调用。目前的 sys_exit 直接退出了 ArceOS task，但父进程无从得知。
线程支持：支持在一个进程地址空间中创建多个线程 (sys_clone 或底层的 sys_thread_create)。
3. 微内核的内存管理 (User-space Memory Management)
内核应该提供给用户态动态管理自身内存的能力。

动态内存映射 (mmap/munmap/mprotect)：允许用户态应用（如 ulib 中的堆分配器）在运行时向内核申请/释放物理内存和匿名页映射。
页面错误处理 (User-level Page Fault Handling)：目前的 ReturnReason::PageFault 会直接终止程序。高级微内核通常允许将 Page Fault 作为一个 IPC 消息发送给用户态的 VMM（虚拟内存管理器）甚至 Pager 来实现按需调页（Demand Paging）或写时复制（COW）。
4. 设备驱动与中断路由 (User-space Drivers & Interrupts)
微内核的本质是将驱动移出内核。目前的实现因为依赖 ArceOS，很多组件还在 Ring 0。

中断分发到用户态：需要一个机制让内核将硬件中断转换为一个 IPC 消息或 Event，发送给等待该中断的用户态驱动（如串口驱动、网卡驱动）。
MMIO 映射：允许特定的驱动进程将硬件的物理地址映射到自己的用户空间中进行直接操作。
端口 IO (针对 x86)：为特定特权进程开放软硬交互权限。
5. 权限与安全 (Capabilities / Security)
目前没有任何系统级权限隔离，任何 APP 都可以调用 sys_spawn 或通过 sys_read_file 访问整个文件系统。

权能机制 (Capability System)：引入类似 seL4 或 Fuchsia 的 Token/Handle 机制，进程需要持有特定的 Capability 才能调用接口或向某进程发消息。
命名空间隔离。
6. 去内核化的文件系统 (User-space VFS)
目前 sys_read_file 是内核直接调用了 axfs::api::read，也就是文件系统实际上是实现在内核里的，这违背了微内核的初衷。
演进方向：需要将文件系统实现为一个用户态进程（FS Server）。当 APP 调用 open/read 时，它们应当通过 IPC 调用发送给 FS Server，由 FS Server 控制块设备并返回数据。
7. 定时器与事件机制 (Timers & Events)
系统滴答与 Sleep：缺少诸如 sys_sleep 或注册定时器回调的系统调用。
事件多路复用：类似于 Linux 的 epoll，允许单进程监听多个通信信道、中断、或超时事件。


第一层：将基础打印、内存、退出对接 Linux ABI
我们就用项目中已经引入的 syscalls::Sysno（在你的 Cargo.toml 里面有，我刚刚将它引入到 mene-syscall 里了）。

把原本的 1 (sys_log) 映射到 Sysno::write (64 号)，伪装成向 stdout 写数据。
把原本的 6 (sys_exit) 映射到 Sysno::exit (93 号)。
把原本的 7 (sys_mmap) 映射到 Sysno::mmap (222 号)。
结果：此时，任何标准 C/C++ 编译器只要生成了这三个 Syscall，你的核心都能直接跑它！

第二层：将 MeneOS 独有的微内核特性映射到专有号段
Mene 独有的任务孵化（Spawn）、非常原始轻量的 IPC 等等，并不存在于标准 Linux 中（Linux 中 fork/exec 的概念与微内核 Spawn 完全不同）。
由于 Linux System Call ID 号目前只用到 400 左右，我们可以把：

500 定义为 MeneSysno::spawn
501 定义为 MeneSysno::ipc_send
502 定义为 MeneSysno::ipc_recv
第三层：User Space (ulib) 的无感封装
在我们的 ulib 里：
用 Rust 的 syscalls crate 生成普通的系统调用（就像普通的 C Lib 那样），而自己包一个 MeneSysno 常量枚举给你的那几个专属的高级接口用。