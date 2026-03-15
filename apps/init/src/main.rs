#![no_std]
#![no_main]

use ulib::{blk, control, fs, Handle};

const MAX_SERVICES: usize = 16;
const MAX_DEPS: usize = 4;
const SUPERVISOR_TICK_MS: usize = 100;
const SUPERVISOR_PROBE_INTERVAL_TICKS: usize = 10; // 1s when tick is 100ms
const MAX_NAME: usize = control::MAX_SERVICE_NAME;
const MAX_BINARY: usize = fs::MAX_PATH;

const DEV_PCI_ECAM_BASE: usize = 0x4010_000000;
const DEV_VIRTIO_BLK_MMIO_BASE: usize = 0x0a00_0000;
const DEV_VIRTIO_BLK_MMIO_SIZE: usize = 0x1000;

#[derive(Clone, Copy)]
enum RestartPolicy {
    Always,
    OnFailure,
    OneShot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HealthProbe {
    None,
    FsPing,
    BlkPing,
}

#[derive(Clone, Copy)]
enum LaunchMode {
    Spawn,
    FsExec,
}

#[derive(Clone, Copy)]
struct ServiceSpec {
    valid: bool,
    name_len: usize,
    name: [u8; MAX_NAME],
    binary_len: usize,
    binary: [u8; MAX_BINARY],
    dep_count: usize,
    dep_name_lens: [usize; MAX_DEPS],
    dep_names: [[u8; MAX_NAME]; MAX_DEPS],
    dep_mask: u32,
    restart_policy: RestartPolicy,
    health_probe: HealthProbe,
    launch_mode: LaunchMode,
}

impl ServiceSpec {
    const fn empty() -> Self {
        Self {
            valid: false,
            name_len: 0,
            name: [0; MAX_NAME],
            binary_len: 0,
            binary: [0; MAX_BINARY],
            dep_count: 0,
            dep_name_lens: [0; MAX_DEPS],
            dep_names: [[0; MAX_NAME]; MAX_DEPS],
            dep_mask: 0,
            restart_policy: RestartPolicy::OneShot,
            health_probe: HealthProbe::None,
            launch_mode: LaunchMode::Spawn,
        }
    }

    fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    fn binary_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.binary[..self.binary_len]).ok()
    }
}

#[derive(Clone, Copy)]
struct ServiceRuntime {
    started: bool,
    attempted: bool,
    healthy_once: bool,
    probe_fail_streak: u8,
    next_probe_tick: usize,
    next_retry_tick: usize,
}

impl ServiceRuntime {
    const fn empty() -> Self {
        Self {
            started: false,
            attempted: false,
            healthy_once: false,
            probe_fail_streak: 0,
            next_probe_tick: 0,
            next_retry_tick: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct RegistrySlot {
    in_use: bool,
    owner_pid: usize,
    cap_handle: usize,
    name_len: usize,
    name: [u8; MAX_NAME],
}

impl RegistrySlot {
    const fn empty() -> Self {
        Self {
            in_use: false,
            owner_pid: 0,
            cap_handle: 0,
            name_len: 0,
            name: [0; MAX_NAME],
        }
    }

    fn name_eq(&self, n: &[u8]) -> bool {
        self.in_use && self.name_len == n.len() && self.name[..self.name_len] == *n
    }
}

struct ServiceRegistry {
    slots: [RegistrySlot; MAX_SERVICES],
}

impl ServiceRegistry {
    fn new() -> Self {
        Self {
            slots: [RegistrySlot::empty(); MAX_SERVICES],
        }
    }

    fn register(&mut self, name: &[u8], owner_pid: usize, cap_handle: usize) -> bool {
        if name.is_empty() || name.len() > MAX_NAME || cap_handle == 0 {
            return false;
        }

        for slot in self.slots.iter_mut() {
            if slot.name_eq(name) {
                slot.owner_pid = owner_pid;
                slot.cap_handle = cap_handle;
                return true;
            }
        }

        for slot in self.slots.iter_mut() {
            if !slot.in_use {
                slot.in_use = true;
                slot.owner_pid = owner_pid;
                slot.cap_handle = cap_handle;
                slot.name_len = name.len();
                slot.name[..name.len()].copy_from_slice(name);
                return true;
            }
        }

        false
    }

    fn lookup(&self, name: &[u8]) -> Option<usize> {
        self.slots
            .iter()
            .find(|slot| slot.name_eq(name))
            .map(|slot| slot.cap_handle)
    }
}

fn clean_line(raw: &str) -> &str {
    let no_comment = match raw.find('#') {
        Some(pos) => &raw[..pos],
        None => raw,
    };
    no_comment.trim()
}

fn parse_kv(line: &str) -> Option<(&str, &str)> {
    let idx = line.find('=')?;
    let k = line[..idx].trim();
    let mut v = line[idx + 1..].trim();
    if v.len() >= 2 && v.as_bytes()[0] == b'"' && v.as_bytes()[v.len() - 1] == b'"' {
        v = &v[1..v.len() - 1];
    }
    Some((k, v))
}

fn parse_service_section_name(line: &str) -> Option<&str> {
    if !line.starts_with("[service.") || !line.ends_with(']') {
        return None;
    }
    let name = &line[9..line.len() - 1];
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn copy_bytes(dst: &mut [u8], src: &[u8]) -> usize {
    let n = core::cmp::min(dst.len(), src.len());
    dst[..n].copy_from_slice(&src[..n]);
    n
}

fn parse_dep_list(spec: &mut ServiceSpec, value: &str) {
    spec.dep_count = 0;
    for dep in value.split(',') {
        let dep = dep.trim();
        if dep.is_empty() || spec.dep_count >= MAX_DEPS {
            continue;
        }
        let i = spec.dep_count;
        let n = copy_bytes(&mut spec.dep_names[i], dep.as_bytes());
        spec.dep_name_lens[i] = n;
        spec.dep_count += 1;
    }
}

fn parse_policy(value: &str) -> RestartPolicy {
    match value {
        "always" => RestartPolicy::Always,
        "on-failure" => RestartPolicy::OnFailure,
        _ => RestartPolicy::OneShot,
    }
}

fn parse_probe(value: &str) -> HealthProbe {
    match value {
        "fs_ping" => HealthProbe::FsPing,
        "blk_ping" => HealthProbe::BlkPing,
        _ => HealthProbe::None,
    }
}

fn finish_spec(out: &mut [ServiceSpec; MAX_SERVICES], count: &mut usize, cur: &ServiceSpec) {
    if !cur.valid || cur.name_len == 0 || cur.binary_len == 0 || *count >= MAX_SERVICES {
        return;
    }
    out[*count] = *cur;
    *count += 1;
}

fn resolve_dep_mask(specs: &mut [ServiceSpec; MAX_SERVICES], count: usize) {
    let mut i = 0;
    while i < count {
        let mut mask = 0u32;
        let mut di = 0;
        while di < specs[i].dep_count {
            let dname = &specs[i].dep_names[di][..specs[i].dep_name_lens[di]];
            let mut j = 0;
            while j < count {
                if specs[j].name_bytes() == dname {
                    mask |= 1u32 << j;
                    break;
                }
                j += 1;
            }
            di += 1;
        }
        specs[i].dep_mask = mask;
        i += 1;
    }
}

fn parse_boot_graph(config: &str, out: &mut [ServiceSpec; MAX_SERVICES]) -> usize {
    let mut count = 0usize;
    let mut cur = ServiceSpec::empty();
    let mut in_service = false;

    for raw in config.lines() {
        let line = clean_line(raw);
        if line.is_empty() {
            continue;
        }

        if let Some(name) = parse_service_section_name(line) {
            if in_service {
                finish_spec(out, &mut count, &cur);
            }
            cur = ServiceSpec::empty();
            cur.valid = true;
            cur.launch_mode = LaunchMode::Spawn;
            cur.restart_policy = RestartPolicy::OneShot;
            cur.health_probe = HealthProbe::None;
            cur.name_len = copy_bytes(&mut cur.name, name.as_bytes());
            in_service = true;
            continue;
        }

        if !in_service {
            continue;
        }

        let Some((k, v)) = parse_kv(line) else {
            continue;
        };
        match k {
            "binary" => {
                cur.binary_len = copy_bytes(&mut cur.binary, v.as_bytes());
            }
            "depends_on" => {
                parse_dep_list(&mut cur, v);
            }
            "restart_policy" => {
                cur.restart_policy = parse_policy(v);
            }
            "health_probe" => {
                cur.health_probe = parse_probe(v);
            }
            "launch" => {
                cur.launch_mode = if v == "fs_exec" {
                    LaunchMode::FsExec
                } else {
                    LaunchMode::Spawn
                };
            }
            _ => {}
        }
    }

    if in_service {
        finish_spec(out, &mut count, &cur);
    }

    resolve_dep_mask(out, count);
    count
}

fn parse_control_req(req: &[u8]) -> Option<(u16, &[u8])> {
    if req.len() < control::HDR_LEN {
        return None;
    }
    let opcode = u16::from_le_bytes([req[0], req[1]]);
    let name_len = u16::from_le_bytes([req[2], req[3]]) as usize;
    if name_len == 0 || name_len > control::MAX_SERVICE_NAME {
        return None;
    }
    if req.len() < control::HDR_LEN + name_len {
        return None;
    }
    Some((opcode, &req[4..4 + name_len]))
}

fn device_query_value(key: &[u8]) -> Option<usize> {
    if key == b"pci.ecam_base" {
        return Some(DEV_PCI_ECAM_BASE);
    }
    if key == b"virtio_blk.mmio_base" {
        return Some(DEV_VIRTIO_BLK_MMIO_BASE);
    }
    if key == b"virtio_blk.mmio_size" {
        return Some(DEV_VIRTIO_BLK_MMIO_SIZE);
    }
    None
}

fn handle_control_plane_msg(
    registry: &mut ServiceRegistry,
    from_pid: usize,
    msg: &[u8],
    recv_cap: Option<Handle>,
) -> bool {
    let Some((opcode, name)) = parse_control_req(msg) else {
        return false;
    };

    match opcode {
        control::REQ_REGISTER_SERVICE => {
            if let Some(cap) = recv_cap {
                let _ = registry.register(name, from_pid, cap.to_usize());
            }
            true
        }
        control::REQ_LOOKUP_SERVICE => {
            let Some(reply_ep) = recv_cap else {
                return true;
            };
            if let Some(service_cap) = registry.lookup(name) {
                ulib::sys_ipc_send(reply_ep, b"OK", Some(Handle::Dynamic(service_cap)));
            } else {
                ulib::sys_ipc_send(reply_ep, b"ENOENT", None);
            }
            true
        }
        control::REQ_DEVICE_QUERY => {
            let Some(reply_ep) = recv_cap else {
                return true;
            };
            if let Some(v) = device_query_value(name) {
                let mut out = [0u8; 10];
                out[0..2].copy_from_slice(b"OK");
                out[2..10].copy_from_slice(&(v as u64).to_le_bytes());
                ulib::sys_ipc_send(reply_ep, &out, None);
            } else {
                ulib::sys_ipc_send(reply_ep, b"ENOENT", None);
            }
            true
        }
        _ => false,
    }
}

fn wait_for_reply_or_control(
    registry: &mut ServiceRegistry,
    expected: &[u8],
    timeout_ms: usize,
    tries: usize,
) -> bool {
    let mut buf = [0u8; 64];
    let mut i = 0;
    while i < tries {
        let mut from_pid = 0usize;
        let mut recv_cap = None;
        let n = ulib::sys_ipc_recv_timeout(&mut from_pid, &mut buf, &mut recv_cap, timeout_ms);
        if n < 0 {
            i += 1;
            continue;
        }
        let n = n as usize;
        if recv_cap.is_none() && n == expected.len() && &buf[..n] == expected {
            return true;
        }
        let _ = handle_control_plane_msg(registry, from_pid, &buf[..n], recv_cap);
        i += 1;
    }
    false
}

fn send_ping_and_wait(registry: &mut ServiceRegistry, target: usize, req: &[u8], expected: &[u8]) -> bool {
    if !ulib::sys_ipc_send_checked(Handle::Dynamic(target), req, Some(Handle::LocalEndpoint)) {
        return false;
    }
    wait_for_reply_or_control(registry, expected, 20, 20)
}

fn fs_exec(registry: &mut ServiceRegistry, path: &str) -> bool {
    let p = path.as_bytes();
    if p.is_empty() || p.len() > fs::MAX_PATH {
        return false;
    }

    let mut req = [0u8; fs::PATH_HDR_LEN + fs::MAX_PATH];
    req[0..2].copy_from_slice(&fs::REQ_EXEC.to_le_bytes());
    req[2..4].copy_from_slice(&(p.len() as u16).to_le_bytes());
    req[4..4 + p.len()].copy_from_slice(p);

    if !ulib::sys_ipc_send_checked(Handle::FsEndpoint, &req[..4 + p.len()], Some(Handle::LocalEndpoint)) {
        return false;
    }

    // REQ_EXEC may take seconds because fs needs to read ELF via user-space blk IPC.
    wait_for_reply_or_control(registry, b"OK", 20, 250)
}

fn probe_service(spec: &ServiceSpec, registry: &mut ServiceRegistry) -> bool {
    match spec.health_probe {
        HealthProbe::None => true,
        HealthProbe::FsPing => {
            let Some(h) = registry.lookup(spec.name_bytes()) else {
                return false;
            };
            let req = fs::REQ_PING.to_le_bytes();
            send_ping_and_wait(registry, h, &req, b"PONG")
        }
        HealthProbe::BlkPing => {
            let Some(h) = registry.lookup(spec.name_bytes()) else {
                return false;
            };
            let req = blk::REQ_PING.to_le_bytes();
            send_ping_and_wait(registry, h, &req, b"PONG")
        }
    }
}

fn spawn_service(spec: &ServiceSpec, registry: &mut ServiceRegistry) -> bool {
    match spec.launch_mode {
        LaunchMode::Spawn => {
            let Some(path) = spec.binary_str() else {
                return false;
            };
            ulib::sys_spawn(path) != 0
        }
        LaunchMode::FsExec => {
            let Some(path) = spec.binary_str() else {
                return false;
            };
            fs_exec(registry, path)
        }
    }
}

fn launch_graph(
    specs: &[ServiceSpec; MAX_SERVICES],
    count: usize,
    runtime: &mut [ServiceRuntime; MAX_SERVICES],
    registry: &mut ServiceRegistry,
) {
    let all_mask = if count == 32 { usize::MAX } else { (1usize << count) - 1 };
    let mut started_mask = 0usize;

    loop {
        let mut progress = false;
        let mut i = 0;
        while i < count {
            if (started_mask & (1usize << i)) != 0 {
                i += 1;
                continue;
            }
            if (specs[i].dep_mask as usize) & !started_mask != 0 {
                i += 1;
                continue;
            }
            runtime[i].attempted = true;
            if spawn_service(&specs[i], registry) {
                runtime[i].started = true;
                started_mask |= 1usize << i;
                progress = true;
            }
            i += 1;
        }

        if started_mask == all_mask || !progress {
            break;
        }
    }
}

fn supervisor_tick(
    tick: usize,
    specs: &[ServiceSpec; MAX_SERVICES],
    count: usize,
    runtime: &mut [ServiceRuntime; MAX_SERVICES],
    registry: &mut ServiceRegistry,
) {
    let mut started_mask = 0u32;
    let mut i = 0;
    while i < count {
        if runtime[i].started {
            started_mask |= 1u32 << i;
        }
        i += 1;
    }

    i = 0;
    while i < count {
        if !runtime[i].started {
            if matches!(specs[i].restart_policy, RestartPolicy::OneShot) && runtime[i].attempted {
                i += 1;
                continue;
            }
            if (specs[i].dep_mask & !started_mask) == 0
                && tick >= runtime[i].next_retry_tick
            {
                runtime[i].attempted = true;
                if spawn_service(&specs[i], registry) {
                    runtime[i].started = true;
                }
            }
            i += 1;
            continue;
        }

        if matches!(specs[i].restart_policy, RestartPolicy::OneShot)
            || matches!(specs[i].health_probe, HealthProbe::None)
        {
            i += 1;
            continue;
        }

        if tick < runtime[i].next_probe_tick {
            i += 1;
            continue;
        }
        runtime[i].next_probe_tick = tick.saturating_add(SUPERVISOR_PROBE_INTERVAL_TICKS);

        if !probe_service(&specs[i], registry) {
            runtime[i].probe_fail_streak = runtime[i].probe_fail_streak.saturating_add(1);
            let restart = match specs[i].restart_policy {
                RestartPolicy::Always => runtime[i].probe_fail_streak >= 3,
                RestartPolicy::OnFailure => runtime[i].healthy_once && runtime[i].probe_fail_streak >= 3,
                RestartPolicy::OneShot => false,
            };
            if restart {
                runtime[i].started = false;
                runtime[i].healthy_once = false;
                runtime[i].probe_fail_streak = 0;
                runtime[i].next_probe_tick = tick.saturating_add(SUPERVISOR_PROBE_INTERVAL_TICKS);
                runtime[i].next_retry_tick = tick + 10;
                ulib::sys_log("init: supervisor restarting unhealthy service");
            }
        } else {
            runtime[i].healthy_once = true;
            runtime[i].probe_fail_streak = 0;
        }

        i += 1;
    }
}

fn control_plane_loop(
    specs: &[ServiceSpec; MAX_SERVICES],
    count: usize,
    runtime: &mut [ServiceRuntime; MAX_SERVICES],
    registry: &mut ServiceRegistry,
) -> ! {
    let mut tick = 0usize;
    let mut buf = [0u8; control::HDR_LEN + control::MAX_SERVICE_NAME + 8];

    loop {
        let mut from_pid = 0usize;
        let mut recv_cap = None;
        let n = ulib::sys_ipc_recv_timeout(&mut from_pid, &mut buf, &mut recv_cap, SUPERVISOR_TICK_MS);
        if n >= 0 {
            let n = n as usize;
            if n > 0 {
                let _ = handle_control_plane_msg(registry, from_pid, &buf[..n], recv_cap);
            }
        }

        tick = tick.saturating_add(1);
        supervisor_tick(tick, specs, count, runtime, registry);
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    ulib::init_allocator();
    ulib::sys_log("init: control plane started");

    let mut registry = ServiceRegistry::new();
    let _ = registry.register(b"init", 1, Handle::LocalEndpoint.to_usize());
    let _ = registry.register(b"device_manager", 1, Handle::LocalEndpoint.to_usize());

    let mut specs = [ServiceSpec::empty(); MAX_SERVICES];
    let mut runtime = [ServiceRuntime::empty(); MAX_SERVICES];

    let mut cfg_buf = [0u8; 2048];
    let cfg_len = ulib::sys_get_boot_cfg(&mut cfg_buf);
    if cfg_len == 0 {
        ulib::sys_log("init: missing boot cfg");
        control_plane_loop(&specs, 0, &mut runtime, &mut registry);
    }

    let count = match core::str::from_utf8(&cfg_buf[..cfg_len]) {
        Ok(s) => parse_boot_graph(s, &mut specs),
        Err(_) => 0,
    };

    launch_graph(&specs, count, &mut runtime, &mut registry);
    ulib::sys_log("init: boot graph launched, supervisor running");
    control_plane_loop(&specs, count, &mut runtime, &mut registry)
}
