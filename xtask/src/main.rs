mod app;
mod cli;
mod commands;
mod context;
mod error;
mod make;

use std::process::ExitCode;

fn main() -> ExitCode {
    app::run()
}