//! `wisp` binary entry point.

use std::process;

use clap::Parser;
use color_eyre::eyre::Result;
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

mod cli;

// Use mimalloc as the global allocator for better performance.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    // Install color-eyre before anything that might panic.
    color_eyre::install().expect("color-eyre install");

    let parsed = cli::Cli::parse();

    if let Err(err) = run(parsed) {
        eprintln!("{err:?}");
        process::exit(1);
    }
}

fn run(cli: cli::Cli) -> Result<()> {
    init_tracing(cli.global.verbose, cli.global.quiet, cli.global.no_color)?;

    // Load config (respects --config override).
    let _cfg = match &cli.global.config {
        Some(path) => wisp_core::config::Config::load_from(path)?,
        None => wisp_core::config::Config::load()?,
    };

    match cli.command {
        None | Some(cli::Command::Tui(_)) => dispatch_tui(),
        Some(cli::Command::Clean(args)) => dispatch_clean(args),
        Some(cli::Command::Analyze(args)) => dispatch_analyze(args),
        Some(cli::Command::History(args)) => dispatch_history(args),
        Some(cli::Command::State(args)) => dispatch_state(args),
        Some(cli::Command::Config(args)) => dispatch_config(args),
        Some(cli::Command::Profile(args)) => dispatch_profile(args),
        Some(cli::Command::Doctor) => dispatch_doctor(),
        Some(cli::Command::Completion(args)) => dispatch_completion(args),
        Some(cli::Command::Man) => dispatch_man(),
    }
}

// ─── Tracing initialisation ───────────────────────────────────────────────────

fn init_tracing(verbose: u8, quiet: bool, no_color: bool) -> Result<()> {
    if quiet {
        return Ok(());
    }

    let level = match verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };

    let filter = EnvFilter::from_default_env()
        .add_directive(level.into());

    let fmt_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(!no_color);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();

    Ok(())
}

// ─── Dispatch stubs ───────────────────────────────────────────────────────────

fn dispatch_tui() -> Result<()> {
    eprintln!("TUI not yet implemented (Phase 6). Use `wisp clean` or `wisp analyze`.");
    Ok(())
}

fn dispatch_clean(args: cli::CleanArgs) -> Result<()> {
    match args.command {
        Some(cli::CleanSubcommand::List { group, risk }) => {
            let cleaners = wisp_engine::all_cleaners();
            for entry in cleaners {
                let meta = entry.meta;
                if let Some(ref g) = group {
                    let group_str = format!("{:?}", meta.group()).to_lowercase();
                    if !group_str.contains(g.as_str()) {
                        continue;
                    }
                }
                if let Some(ref r) = risk {
                    let risk_str = format!("{:?}", meta.risk()).to_lowercase();
                    if !risk_str.contains(r.as_str()) {
                        continue;
                    }
                }
                println!("{:20}  {:10}  {}", meta.id(), format!("{:?}", meta.risk()), meta.name());
            }
        }
        Some(cli::CleanSubcommand::Info { target }) => {
            let cleaners = wisp_engine::all_cleaners();
            let found = cleaners.iter().find(|e| e.meta.id().as_str() == target);
            if let Some(entry) = found {
                let m = entry.meta;
                println!("ID:          {}", m.id());
                println!("Name:        {}", m.name());
                println!("Group:       {:?}", m.group());
                println!("Risk:        {:?}", m.risk());
                println!("Root:        {}", m.requires_root());
                println!("Description: {}", m.description());
            } else {
                eprintln!("Cleaner '{target}' not found.");
                process::exit(1);
            }
        }
        None => {
            if let Some(target) = args.target {
                eprintln!("Clean '{target}' not yet implemented (Phase 2-3).");
            } else {
                eprintln!("Specify a target or subcommand. Try `wisp clean list`.");
            }
        }
    }
    Ok(())
}

fn dispatch_analyze(_args: cli::AnalyzeArgs) -> Result<()> {
    eprintln!("Analyze not yet implemented (Phase 1).");
    Ok(())
}

fn dispatch_history(_args: cli::HistoryArgs) -> Result<()> {
    eprintln!("History not yet implemented (Phase 4).");
    Ok(())
}

fn dispatch_state(_args: cli::StateArgs) -> Result<()> {
    eprintln!("State not yet implemented.");
    Ok(())
}

fn dispatch_config(args: cli::ConfigArgs) -> Result<()> {
    match args.command {
        Some(cli::ConfigSubcommand::Path) | None => {
            match wisp_core::config::Config::config_path() {
                Some(p) => println!("{}", p.display()),
                None => eprintln!("Could not determine config path."),
            }
        }
        Some(cli::ConfigSubcommand::Show { key: None }) => {
            let cfg = wisp_core::config::Config::load()?;
            println!("{}", toml_value(&cfg)?);
        }
        _ => eprintln!("Config subcommand not yet implemented."),
    }
    Ok(())
}

fn dispatch_profile(_args: cli::ProfileArgs) -> Result<()> {
    eprintln!("Profile not yet implemented.");
    Ok(())
}

fn dispatch_doctor() -> Result<()> {
    let distro = wisp_platform::detect_distro();
    println!("Distribution:  {} ({})", distro.name(), distro.id());
    println!("Platform:      {}", std::env::consts::OS);
    println!("Architecture:  {}", std::env::consts::ARCH);

    match wisp_core::config::Config::config_path() {
        Some(p) => {
            let exists = if p.exists() { "found" } else { "not found (defaults used)" };
            println!("Config:        {} [{}]", p.display(), exists);
        }
        None => println!("Config:        unable to determine path"),
    }

    println!("Doctor check complete.");
    Ok(())
}

fn dispatch_completion(args: cli::CompletionArgs) -> Result<()> {
    eprintln!("Shell completion for {:?} not yet implemented (Phase 5).", args.shell);
    Ok(())
}

fn dispatch_man() -> Result<()> {
    eprintln!("Man page generation not yet implemented (Phase 5).");
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn toml_value<T: serde::Serialize>(v: &T) -> Result<String> {
    toml::to_string_pretty(v)
        .map_err(|e| color_eyre::eyre::eyre!("serialisation error: {e}"))
}
