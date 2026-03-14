use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use axsync::Mutex;
use flat_device_tree as fdt;
use mene_ipc::capability::Capability;

use crate::process::ProcessInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceBus {
    Platform,
    PciHost,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioRegion {
    pub base: usize,
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub node_name: String,
    pub bus: DeviceBus,
    pub compatibles: Vec<String>,
    pub mmio_regions: Vec<MmioRegion>,
    pub interrupts: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    ProbeFailed,
}

#[derive(Clone, Copy)]
pub struct DriverRegistration {
    pub name: &'static str,
    pub matches: fn(&DeviceInfo) -> bool,
    pub probe: fn(&DeviceInfo) -> Result<(), DriverError>,
}

lazy_static::lazy_static! {
    static ref DEVICE_LIST: Mutex<Vec<DeviceInfo>> = Mutex::new(Vec::new());
    static ref DRIVER_REGISTRY: Mutex<Vec<DriverRegistration>> = Mutex::new(Vec::new());
}

pub fn register_driver(driver: DriverRegistration) {
    DRIVER_REGISTRY.lock().push(driver);
}

pub fn devices() -> Vec<DeviceInfo> {
    DEVICE_LIST.lock().clone()
}

pub fn pci_ecam_base() -> Option<usize> {
    let list = DEVICE_LIST.lock();
    list.iter()
        .find(|d| d.bus == DeviceBus::PciHost)
        .and_then(|d| d.mmio_regions.first().map(|r| r.base))
}

pub fn init_device_model() {
    register_builtin_drivers();

    if let Some(fdt) = parse_current_fdt() {
        let mut parsed = Vec::new();
        for node in fdt.all_nodes() {
            if let Some(dev) = parse_device_node(node) {
                parsed.push(dev);
            }
        }

        let count = parsed.len();
        *DEVICE_LIST.lock() = parsed;
        axlog::info!("device-model: parsed {} DT device nodes", count);

        probe_registered_drivers();
    } else {
        axlog::warn!("device-model: no valid DTB available");
    }
}

fn probe_registered_drivers() {
    let devices = DEVICE_LIST.lock().clone();
    let drivers = DRIVER_REGISTRY.lock().clone();

    for dev in &devices {
        for drv in &drivers {
            if (drv.matches)(dev) {
                match (drv.probe)(dev) {
                    Ok(()) => {
                        axlog::info!(
                            "device-model: driver '{}' probed node '{}'",
                            drv.name,
                            dev.node_name
                        );
                    }
                    Err(_) => {
                        axlog::warn!(
                            "device-model: driver '{}' failed to probe node '{}'",
                            drv.name,
                            dev.node_name
                        );
                    }
                }
            }
        }
    }
}

fn parse_current_fdt() -> Option<fdt::Fdt<'static>> {
    let dtb_paddr = axhal::dtb::get_bootarg();
    let dtb_ptr = axhal::mem::phys_to_virt(dtb_paddr.into()).as_mut_ptr();

    // SAFETY: bootarg points to the DTB passed by bootloader/platform.
    unsafe { fdt::Fdt::from_ptr(dtb_ptr as *const u8).ok() }
}

fn parse_device_node(node: fdt::node::FdtNode<'_, 'static>) -> Option<DeviceInfo> {
    let mut compatibles = Vec::new();
    if let Some(c) = node.compatible() {
        for s in c.all() {
            compatibles.push(s.to_string());
        }
    }

    if compatibles.is_empty() {
        return None;
    }

    if node
        .property("status")
        .and_then(|p| p.as_str())
        .is_some_and(|s| s == "disabled")
    {
        return None;
    }

    let mut mmio_regions = Vec::new();
    for reg in node.reg() {
        mmio_regions.push(MmioRegion {
            base: reg.starting_address as usize,
            size: reg.size.unwrap_or(0),
        });
    }

    let mut interrupts = Vec::new();
    for irq in node.interrupts() {
        interrupts.push(irq);
    }

    Some(DeviceInfo {
        node_name: node.name.to_string(),
        bus: detect_bus(&compatibles),
        compatibles,
        mmio_regions,
        interrupts,
    })
}

fn detect_bus(compatibles: &[String]) -> DeviceBus {
    if compatibles
        .iter()
        .any(|c| c.contains("pci") || c.contains("pcie"))
    {
        return DeviceBus::PciHost;
    }

    if compatibles
        .iter()
        .any(|c| c.contains("simple-bus") || c.contains("virtio,mmio"))
    {
        return DeviceBus::Platform;
    }

    DeviceBus::Unknown
}

fn register_builtin_drivers() {
    register_driver(DriverRegistration {
        name: "virtio-mmio",
        matches: |dev| dev.compatibles.iter().any(|c| c == "virtio,mmio"),
        probe: |dev| {
            let mmio = dev.mmio_regions.first().copied();
            axlog::info!(
                "driver-model: virtio-mmio discovered node='{}' mmio={:?} irq={:?}",
                dev.node_name,
                mmio,
                dev.interrupts
            );
            Ok(())
        },
    });

    register_driver(DriverRegistration {
        name: "pl011-uart",
        matches: |dev| {
            dev.compatibles
                .iter()
                .any(|c| c == "arm,pl011" || c == "arm,sbsa-uart")
        },
        probe: |dev| {
            axlog::info!(
                "driver-model: pl011 discovered node='{}' mmio={:?} irq={:?}",
                dev.node_name,
                dev.mmio_regions.first().copied(),
                dev.interrupts
            );
            Ok(())
        },
    });

    register_driver(DriverRegistration {
        name: "generic-pci-host",
        matches: |dev| dev.bus == DeviceBus::PciHost,
        probe: |dev| {
            axlog::info!(
                "driver-model: pci host node='{}' compatible={:?}",
                dev.node_name,
                dev.compatibles
            );
            Ok(())
        },
    });
}

pub fn inject_bootstrap_capabilities(
    app_path: &str,
    cspace_map: &mut BTreeMap<usize, Capability>,
    ptable: &BTreeMap<usize, ProcessInfo>,
) {
    inject_service_endpoint_by_path(cspace_map, ptable, "/boot/serial", 2);
    inject_service_endpoint_by_path(cspace_map, ptable, "/boot/vmm", 3);
    inject_service_endpoint_by_path(cspace_map, ptable, "/boot/fs", 5);

    let has_virtio_in_dtb = devices().iter().any(|d| {
        d.compatibles
            .iter()
            .any(|c| c == "virtio,mmio" || c.contains("virtio"))
    });

    // Only expose virtio-blk endpoint to services/apps that may need block I/O.
    let need_blk = app_path != "/boot/serial";
    if has_virtio_in_dtb && need_blk {
        inject_service_endpoint_by_path(cspace_map, ptable, "/boot/virtio_blk", 4);
    }
}

fn inject_service_endpoint_by_path(
    cspace_map: &mut BTreeMap<usize, Capability>,
    ptable: &BTreeMap<usize, ProcessInfo>,
    service_path: &str,
    handle: usize,
) {
    if let Some((_, p)) = ptable.iter().find(|(_, p)| p.app_path == service_path) {
        cspace_map.insert(
            handle,
            Capability::Endpoint(p.local_endpoint.clone()),
        );
    }
}
