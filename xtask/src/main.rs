//! xtask – build and packaging helpers.
//!
//! Run with `cargo xtask <task>`.

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "wisp build helpers")]
struct Args {
    #[command(subcommand)]
    task: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// Generate shell completion scripts (Phase 5).
    Completion,
    /// Generate man pages (Phase 5).
    Man,
    /// Build a release tarball (Phase 8).
    Dist,
}

fn main() {
    let args = Args::parse();
    match args.task {
        Task::Completion => eprintln!("completion generation not yet implemented"),
        Task::Man => eprintln!("man page generation not yet implemented"),
        Task::Dist => eprintln!("dist packaging not yet implemented"),
    }
}
