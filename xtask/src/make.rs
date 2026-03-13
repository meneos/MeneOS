use duct::cmd;

use crate::cli::MakeArgs;
use crate::context::Workspace;
use crate::error::{Result, XtaskError};

pub fn invoke(workspace: &Workspace, args: &MakeArgs, target: &str) -> Result<()> {
    ensure_app(workspace)?;

    let command = build_command_args(workspace, args, target);
    cmd("make", command)
        .dir(&workspace.root_dir)
        .run()
        .map_err(XtaskError::from)?;

    Ok(())
}

fn build_command_args(workspace: &Workspace, args: &MakeArgs, target: &str) -> Vec<String> {
    let mut command = vec![
        "-C".to_string(),
        workspace.arceos_dir.display().to_string(),
        format!("A={}", workspace.app_dir.display()),
        format!("ARCH={}", args.arch),
    ];

    // Load .env variables if present
    let _ = dotenvy::dotenv();

    // Mapping from environment keys to their default make arguments if not provided by `.env`
    if cfg!(target_os = "macos")
        && !has_arg_override(args, "ACCEL=")
        && std::env::var("ACCEL").is_err()
    {
        command.push("ACCEL=n".to_string());
    }

    if !has_arg_override(args, "BLK=") && std::env::var("BLK").is_err() {
        command.push("BLK=y".to_string());
    }

    if !has_arg_override(args, "DISK_IMG=") {
        if let Ok(disk_img) = std::env::var("DISK_IMG") {
            command.push(format!(
                "DISK_IMG={}",
                workspace.root_dir.join(disk_img).display()
            ));
        } else {
            command.push(format!(
                "DISK_IMG={}",
                workspace.root_dir.join("disk.img").display()
            ));
        }
    }

    if !has_arg_override(args, "NO_AXSTD=") && std::env::var("NO_AXSTD").is_err() {
        // Fallback for NO_AXSTD shouldn't strictly be needed since it's in .env, but keeping structural compatibility
        command.push("NO_AXSTD=y".to_string());
    }

    // Append environment variables loaded that are part of standard ArceOS params
    for key in &["ACCEL", "BLK", "NO_AXSTD"] {
        if !has_arg_override(args, &format!("{}=", key)) {
            if let Ok(val) = std::env::var(key) {
                command.push(format!("{}={}", key, val));
            }
        }
    }

    command.extend(args.make_args.clone());
    command.push(format!(
        "TARGET_DIR={}",
        workspace.root_dir.join("target").display()
    ));
    command.push(format!(
        "OUT_CONFIG={}",
        workspace.root_dir.join(".axconfig.toml").display()
    ));
    command.push(target.to_string());
    command
}

fn ensure_app(workspace: &Workspace) -> Result<()> {
    let manifest = workspace.app_dir.join("Cargo.toml");
    if manifest.is_file() {
        Ok(())
    } else {
        Err(XtaskError::Message(format!(
            "app manifest not found: {}",
            manifest.display()
        )))
    }
}

fn has_arg_override(args: &MakeArgs, prefix: &str) -> bool {
    args.make_args.iter().any(|arg| arg.starts_with(prefix))
}
