use crate::cli::MakeArgs;
use crate::context::Workspace;
use crate::error::Result;
use crate::make;

pub fn run(args: MakeArgs) -> Result<()> {
    let workspace = Workspace::discover(&args.app)?;
    make::invoke(&workspace, &args, "build")
}
