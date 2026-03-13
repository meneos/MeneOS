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

    if cfg!(target_os = "macos") && !has_accel_override(args) {
        command.push("ACCEL=n".to_string());
    }

    command.extend(args.make_args.clone());
    command.push(format!("TARGET_DIR={}", workspace.root_dir.join("target").display()));
    command.push(format!("OUT_CONFIG={}", workspace.root_dir.join(".axconfig.toml").display()));
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

fn has_accel_override(args: &MakeArgs) -> bool {
    args.make_args.iter().any(|arg| arg.starts_with("ACCEL="))
}