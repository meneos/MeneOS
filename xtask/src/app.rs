use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::commands;

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Build(args) => commands::build::run(args),
        Commands::Run(args) => commands::run::run(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}