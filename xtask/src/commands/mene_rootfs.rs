use std::process::Command;

use crate::cli::MeneRootfsArgs;
use crate::error::{Result, XtaskError};

pub fn run(args: MeneRootfsArgs) -> Result<()> {
    println!("Building apps in release mode...");
    let target = match args.arch.as_str() {
        "aarch64" => "aarch64-unknown-none-softfloat",
        _ => return Err(XtaskError::Message(format!("Unsupported architecture: {}", args.arch))),
    };

    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            target,
            "--manifest-path",
            "apps/Cargo.toml",
        ])
        .status()?;

    if !status.success() {
        return Err(XtaskError::Message("Failed to build apps".into()));
    }

    let disk_img = "disk.img";

    println!("Resetting /boot in {}", disk_img);
    Command::new("vdisk")
        .args([disk_img, "rm", "-rf", "/boot"])
        .status()?;

    Command::new("vdisk")
        .args([disk_img, "mkdir", "boot"])
        .status()?;

    // Dynamically find all apps (members with src/main.rs)
    let mut apps = Vec::new();
    if let Ok(entries) = std::fs::read_dir("apps") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("src/main.rs").exists() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    apps.push(name.to_string());
                }
            }
        }
    }

    for app in apps {
        let host_path = format!("host:apps/target/{}/release/{}", target, app);
        println!("Copying {} to /boot/...", app);
        let status = Command::new("vdisk")
            .args([disk_img, "cp", &host_path, "/boot/"])
            .status()?;
        
        if !status.success() {
            return Err(XtaskError::Message(format!("Failed to copy {}", app)));
        }
    }

    println!("Copying boot.cfg to /boot/...");
    let status = Command::new("vdisk")
        .args([disk_img, "cp", "host:apps/boot.cfg", "/boot/"])
        .status()?;

    if !status.success() {
        return Err(XtaskError::Message("Failed to copy boot.cfg".into()));
    }

    println!("mene-rootfs completed successfully.");
    Ok(())
}
