use crate::cli::MakeArgs;
use crate::context::Workspace;
use crate::error::Result;
use crate::make;

pub fn run(args: MakeArgs) -> Result<()> {
    let workspace = Workspace::discover(&args.app)?;

    // Check if disk.img exists before running
    let disk_img = "disk.img";
    if !std::path::Path::new(disk_img).exists() {
        return Err(crate::error::XtaskError::Message(
            "disk.img not found. Please run `cargo xtask mene-rootfs` first.".into(),
        ));
    }

    make::invoke(&workspace, &args, "run")
}
