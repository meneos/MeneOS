# MeneOS 模块职责边界定义

## 三层架构模型

```
┌─────────────────────────────────────────────────────────┐
│                   User Services Layer                    │
│  (fs, serial, virtio_blk, user applications)            │
└─────────────────────────────────────────────────────────┘
                          ↓ IPC
┌─────────────────────────────────────────────────────────┐
│              System Control Plane Layer                  │
│  (Process Supervisor, Service Registry, Device Manager)  │
└─────────────────────────────────────────────────────────┘
                          ↓ Syscall
┌─────────────────────────────────────────────────────────┐
│                 Microkernel Core Layer                   │
│  (Scheduler, IPC Primitives, Memory Primitives, Trap)   │
└─────────────────────────────────────────────────────────┘
```

## Layer 1: Microkernel Core

### 职责范围
提供最小化的操作系统原语，不包含策略逻辑。

### 包含模块
- `kernel/mene-kernel` - 核心编排
- `kernel/mene-task` - 线程/任务调度原语
- `kernel/mene-ipc` - IPC 端点与消息传递原语
- `kernel/mene-memory` - 地址空间映射原语
- `kernel/mene-trap` - 中断/异常分发
- `kernel/mene-syscall` - 系统调用路由层

### 允许的操作
- 线程创建、调度、上下文切换
- IPC 端点创建、消息发送/接收、能力传递
- 页表映射/解映射、地址空间切换
- 中断注册、trap 分发
- 系统调用参数解析与对象路由

### 禁止的操作
- 服务发现与注册
- 进程重启策略
- 设备资源分配策略
- 文件系统/网络协议逻辑
- 业务级错误恢复

### 依赖规则
- 只能依赖 ArceOS 底层原语 (axhal, axmm, axtask)
- 不能依赖 User Services 层
- 不能依赖 Control Plane 层

---

## Layer 2: System Control Plane

### 职责范围
集中管理系统策略、服务编排、资源分配。

### 包含模块
- `apps/init` - 控制平面主服务
  - Process Supervisor 子模块
  - Service Registry 子模块
  - Device Manager 子模块
  - VMM Policy 子模块

### 允许的操作
- 服务注册、发现、版本管理
- 进程生命周期监督（启动、健康检查、重启）
- 依赖图解析与拓扑排序
- 设备资源描述与分配
- 内存布局策略与配额管理

### 禁止的操作
- 直接操作页表（应通过 Core 层 syscall）
- 直接操作调度器（应通过 Core 层 syscall）
- 实现具体业务逻辑（文件操作、网络协议等）

### 依赖规则
- 可以调用 Core 层 syscall
- 可以通��� IPC 与 User Services 通信
- 不能绕过 syscall 直接调用内核内部函数

---

## Layer 3: User Services

### 职责范围
实现具体的业务能力服务。

### 包含模块
- `apps/fs` - 文件系统服务
- `apps/serial` - 串口驱动服务
- `apps/virtio_blk` - 块设备驱动服务
- `apps/vmm` - 虚拟内存管理服务
- `apps/helloworld` - 示例应用
- `apps/syscall_compat` - Linux 兼容层

### 允许的操作
- 实现领域特定逻辑
- 通过 Service Registry 发现依赖服务
- 通过 IPC 与其他服务通信
- 通过 syscall 使用内核原语

### 禁止的操作
- 硬编码其他服务的 capability handles
- 直接访问硬件（应通过 Device Manager 获取资源描述）
- 绕过 IPC 直接共享内存（除非通过显式能力传递）

### 依赖规则
- 可以调用 Core 层 syscall
- 可以通过 IPC 与其他 User Services 通信
- 可以通过 IPC 与 Control Plane 通信
- 不能直接调用内核内部函数

---

## 分层边界判定规则

### 归属判定
- **问题是"如何调度/映射/分发"** → Core Layer
- **问题是"谁启动谁/失败如何恢复/资源如何分配"** → Control Plane Layer
- **问题是"如何实现具体业务能力"** → User Services Layer

### 跨层修改规则
如果一个改动同时触及两层及以上，必须拆分为：
1. 原语接口变更（Core Layer）
2. 控制面策略变更（Control Plane Layer）
3. 服务实现变更（User Services Layer）

每个变更独立提交，确保边界清晰。

---

## 模块间通信规范

### Core ↔ Control Plane
- 单向：Control Plane 通过 syscall 调用 Core
- Core 不主动调用 Control Plane

### Core ↔ User Services
- 单向：User Services 通过 syscall 调用 Core
- Core 不主动调用 User Services

### Control Plane ↔ User Services
- 双向：通过 IPC 通信
- 使用标准化的 IPC 协议（见 IPC_PROTOCOL.md）

### User Services ↔ User Services
- 双向：通过 IPC 通信
- 通过 Service Registry 动态发现，不硬编码

---

## CI 检查规则

### 依赖检查
```bash
# Core 层不能依赖 Control Plane 或 User Services
cargo tree -p mene-kernel | grep -E "(apps/|control-plane)" && exit 1

# User Services 不能直接依赖内核内部模块（除了 syscall 接口）
cargo tree -p fs | grep -E "mene-(task|memory|trap|ipc)" && exit 1
```

### 接口稳定性检查
- Core 层接口变更需要 BREAKING CHANGE 标记
- Control Plane 接口变更需要版本号递增
- User Services 接口变更通过协议版本管理

---

## 重构检查清单

在修改代码前，确认：
- [ ] 明确改动属于哪一层
- [ ] 确认没有违反依赖规则
- [ ] 跨层改动已拆分为独立提交
- [ ] 新增接口符合该层职责范围
- [ ] 更新了相关文档
