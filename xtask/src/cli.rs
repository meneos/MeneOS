use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "cargo xtask")]
#[command(about = "Workspace automation tasks for MeneOS")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Build(MakeArgs),
    Run(MakeArgs),
}

#[derive(Debug, Clone, Args)]
pub struct MakeArgs {
    #[arg(long, default_value = "aarch64")]
    pub arch: String,
    #[arg(long, default_value = "entry")]
    pub app: String,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub make_args: Vec<String>,
}