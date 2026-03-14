# MeneOS User-Space Virtio Driver + FS Service Design

## 1. Goal

Build a user-space storage stack under the current MeneOS microkernel model:

- `virtio-blk` runs in user space and owns device interaction.
- `fs` runs in user space and owns file semantics.
- apps use IPC to `fs` only.
- kernel keeps only mechanism: scheduling, address-space control, IPC, capability transfer.

This design is intentionally incremental and aligned with existing code in the repository.

## 2. Current Baseline (Already Available)

The following mechanisms already exist and can be reused directly:

- Handle-based IPC with optional capability passing.
- Per-process local endpoint + CSpace.
- User-space device mapping syscall (`MapDevice`).
- VMM-assisted page mapping model (`VmmMapPageTo`).
- User-space service examples (`serial`, `vmm`) with request/reply style.

Known limitation in current baseline:

- `IpcRecv` currently returns sender PID but user library treats it like a handle in some call sites. This should be normalized before broad protocol expansion.

## 3. Target Architecture

### 3.1 Components

- Kernel
  - IPC routing and capability transfer
  - process table and CSpace ownership
  - memory map syscalls (`MapDevice`, `VmmMapPageTo`)
- `virtio-blk` service (user)
  - MMIO + virtqueue control
  - block read/write/flush operations
  - optional interrupt handling (later phase)
- `fs` service (user)
  - path lookup and file object table
  - open/read/write/close/stat API
  - block cache and metadata handling
- apps (user)
  - call `ulib` wrappers for FS operations
  - no direct device access capability

### 3.2 Trust and Capability Model

- `virtio-blk` should have:
  - capability to map the virtio MMIO region
  - optional IRQ capability (later)
- `fs` should have:
  - endpoint capability to send requests to `virtio-blk`
- regular apps should have:
  - endpoint capability to send requests to `fs`
- regular apps should not receive `virtio-blk` capability.

## 4. Boot and Service Startup Order

Recommended order in `/boot/boot.cfg`:

1. `/boot/serial`
2. `/boot/vmm`
3. `/boot/virtio_blk`
4. `/boot/fs`
5. regular apps

Rationale:

- `serial` first for logs.
- `vmm` first for later memory helpers.
- `virtio-blk` before `fs` because `fs` depends on block I/O.

## 5. IPC Protocols

## 5.1 Common Envelope

All service protocols should use a common binary envelope:

- `u16 opcode`
- `u16 flags`
- `u32 req_id`
- `u32 payload_len`
- `payload bytes...`

Reply should mirror:

- `u32 req_id`
- `i32 status`
- `u32 payload_len`
- `payload bytes...`

Status rules:

- `0` for success
- negative errno-compatible values for failure

## 5.2 Virtio-Block Service Protocol (v1)

Opcodes:

- `1` READ_BLOCK
  - input: `u64 lba`, `u32 block_count`
  - output: raw block bytes
- `2` WRITE_BLOCK
  - input: `u64 lba`, `u32 block_count`, bytes
  - output: empty
- `3` FLUSH
  - input: none
  - output: empty
- `4` GET_INFO
  - input: none
  - output: block_size, block_count, features

Notes:

- In phase 1, use polling completion to simplify implementation.
- IRQ-driven completion can be introduced in phase 2.

## 5.3 FS Service Protocol (v1)

Opcodes:

- `1` OPEN
  - input: path + flags
  - output: file handle
- `2` READ
  - input: file handle, offset, length
  - output: bytes
- `3` WRITE
  - input: file handle, offset, bytes
  - output: written length
- `4` CLOSE
  - input: file handle
  - output: empty
- `5` STAT
  - input: path or handle
  - output: metadata

`fs` internally translates high-level operations into block-level RPCs to `virtio-blk`.

## 6. Kernel and ABI Changes

## 6.1 New Stable Service Handles

Extend handle constants with reserved IDs:

- `4` for `virtio-blk` endpoint
- `5` for `fs` endpoint

Optionally keep dynamic handle assignment for transferred capabilities.

## 6.2 IPC Identity Semantics Fix

Normalize `IpcRecv` identity reporting:

- Option A: return sender endpoint handle (preferred)
- Option B: keep sender PID but do not decode it as handle in `ulib`

This must be fixed before relying on strict service identity checks.

## 6.3 Transitional FS Path

Current `ReadFile` syscall path reads from kernel FS directly.

Transition plan:

- keep kernel `ReadFile` only for bootstrap compatibility
- add `ulib` FS client wrappers that use `fs` IPC
- migrate `init` and apps to `fs` service
- remove kernel direct file read path when migration completes

## 7. User Library (`ulib`) Plan

Add service clients:

- block client
  - `blk_read(lba, blocks, out)`
  - `blk_write(lba, blocks, data)`
  - `blk_flush()`
- fs client
  - `fs_open(path, flags)`
  - `fs_read(fd, off, out)`
  - `fs_write(fd, off, data)`
  - `fs_close(fd)`
  - `fs_stat(path, out)`

Implementation rules:

- all wrappers use request/reply IPC with `req_id`
- strict bounds checks on message lengths
- no panics on malformed replies

## 8. Implementation Phases

## Phase 1: Minimal end-to-end path

- add `apps/virtio_blk` with MMIO mapping + polling I/O loop
- add `apps/fs` with a tiny read-only path support
- add handle injection for `virtio-blk` and `fs`
- add `ulib` wrappers for FS read path
- update boot order and run smoke test

Success criteria:

- app reads a file through `app -> fs -> virtio-blk` path
- no direct kernel `ReadFile` call on that app path

## Phase 2: Correctness and safety

- fix IPC identity semantics
- add request timeout and retry policy
- add basic validation and permission checks for handle usage
- add negative tests for malformed IPC

Success criteria:

- fs and blk survive malformed packets without crash
- service identity checks are not PID/handle ambiguous

## Phase 3: Performance and reliability

- add block cache in `fs`
- optional batched I/O and larger transfer windows
- optional IRQ path in `virtio-blk`
- add service restart policy in `init` (or a supervisor)

Success criteria:

- measurable throughput improvement over phase 1
- bounded latency for common read path

## 9. Testing Plan

- unit tests (where possible)
  - message codec decode/encode
  - bounds checks and status mapping
- integration tests
  - spawn order and service readiness
  - read existing file from app via fs
  - invalid handle and malformed request handling
- stress tests
  - repeated open/read/close loops
  - concurrent readers

## 10. Risks and Mitigations

- Risk: ambiguous sender identity in IPC
  - Mitigation: normalize semantics before scaling protocols
- Risk: capability overexposure
  - Mitigation: strict per-process capability injection policy
- Risk: boot dependency deadlock
  - Mitigation: fixed startup order + readiness ping protocol
- Risk: large-copy overhead
  - Mitigation: introduce shared-memory transport in later phase

## 11. Definition of Done

This design is considered complete when:

- `virtio-blk` is fully user-space and serves block RPC.
- `fs` is fully user-space and serves file RPC.
- at least one app reads files via `fs` IPC path only.
- kernel direct file read path is no longer required for regular apps.

---

This document defines the target state and incremental path without requiring a large one-shot rewrite.


需要特别注意的一个点
当前 ipc_recv 回填的是发送者 pid，但 ulib 把它当 Handle 用（见 lib.rs:67）。这在 pid=3 时“恰好像 VMM 句柄”能工作，但语义不稳。建议尽快把返回字段改成 sender_handle 或在协议里明确 reply capability，避免后续 virtio/fs 时踩坑。

