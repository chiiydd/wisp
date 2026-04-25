//! `wisp` binary entry point.

use std::process;
use std::sync::Arc;

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

mod cli;
mod confirmer;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
    color_eyre::install().expect("color-eyre install");
    let parsed = cli::Cli::parse();
    let exit_code = match run(parsed).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err:?}");
            1
        }
    };
    if exit_code != 0 {
        process::exit(exit_code);
    }
}

async fn run(cli: cli::Cli) -> Result<i32> {
    init_tracing(cli.global.verbose, cli.global.quiet, cli.global.no_color)?;

    let cfg = match &cli.global.config {
        Some(p) => wisp_core::config::Config::load_from(p)?,
        None => wisp_core::config::Config::load()?,
    };

    let distro = Arc::from(wisp_platform::detect_distro());

    let engine_cfg = wisp_engine::EngineConfig {
        dry_run: cli.global.dry_run,
        prefer_trash: cfg.clean.prefer_trash,
        auto_approve_up_to: if cli.global.yes {
            wisp_core::types::RiskLevel::Moderate
        } else {
            wisp_core::types::RiskLevel::Safe
        },
    };
    let engine = Arc::new(wisp_engine::Engine::new(engine_cfg, distro));

    let code = match cli.command {
        None | Some(cli::Command::Tui(_)) => dispatch_tui().await?,
        Some(cli::Command::Clean(args)) => {
            dispatch_clean(args, &cli.global, engine).await?
        }
        Some(cli::Command::Analyze(args)) => dispatch_analyze(args).await?,
        Some(cli::Command::History(args)) => dispatch_history(args)?,
        Some(cli::Command::State(args)) => dispatch_state(args)?,
        Some(cli::Command::Config(args)) => dispatch_config(args)?,
        Some(cli::Command::Profile(args)) => dispatch_profile(args)?,
        Some(cli::Command::Doctor) => dispatch_doctor()?,
        Some(cli::Command::Completion(args)) => dispatch_completion(args)?,
        Some(cli::Command::Man) => dispatch_man()?,
    };
    Ok(code)
}

// ─── Tracing ─────────────────────────────────────────────────────────────────

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
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(level.into()))
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(!no_color))
        .with(ErrorLayer::default())
        .init();
    Ok(())
}

// ─── tui ─────────────────────────────────────────────────────────────────────

async fn dispatch_tui() -> Result<i32> {
    wisp_tui::run_tui()
        .await
        .map_err(|e| eyre!("{e}"))?;
    Ok(0)
}

// ─── clean ───────────────────────────────────────────────────────────────────

async fn dispatch_clean(
    args: cli::CleanArgs,
    global: &cli::GlobalOpts,
    engine: Arc<wisp_engine::Engine>,
) -> Result<i32> {
    match args.command {
        Some(cli::CleanSubcommand::List { group, risk }) => {
            print_cleaner_list(group.as_deref(), risk.as_deref());
            return Ok(0);
        }
        Some(cli::CleanSubcommand::Info { target }) => {
            return Ok(print_cleaner_info(&target));
        }
        None => {}
    }

    let target = match args.target {
        Some(t) => t,
        None => {
            eprintln!("Specify a target or subcommand. Try: wisp clean list");
            return Ok(64);
        }
    };

    // Build plan
    if !global.quiet {
        eprint!("Building plan for '{target}'…");
    }
    let plan = engine.build_plan(&[target.as_str()]).await?;
    if !global.quiet {
        eprintln!(" done.");
    }

    if plan.actions.is_empty() {
        println!("Nothing to clean for '{target}'.");
        return Ok(0);
    }

    match global.output {
        cli::OutputFormat::Human => print_plan_human(&plan, global.dry_run),
        cli::OutputFormat::Json => {
            let env = wisp_core::types::OutputEnvelope::new(
                format!("clean {target}"),
                &plan,
            );
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        cli::OutputFormat::Jsonl => {
            let summary = wisp_core::types::CleanPlanSummary::from(&plan);
            println!("{}", serde_json::to_string(&wisp_core::types::ProgressEvent::PlanBuilt(summary))?);
        }
    }

    if global.dry_run {
        println!("\n[DRY RUN] No changes made.");
        return Ok(0);
    }

    // Choose confirmer
    let cfm: Arc<dyn wisp_core::types::Confirmer> = if global.yes {
        Arc::new(confirmer::AutoConfirmer { approve_dangerous: false })
    } else {
        Arc::new(confirmer::CliConfirmer)
    };

    // Execute and stream events
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let engine_clone = engine.clone();
    let plan_clone = plan.clone();
    let cfm_clone = cfm.clone();
    let output = global.output;

    let exec_handle = tokio::spawn(async move {
        engine_clone.execute(plan_clone, cfm_clone, tx).await
    });

    while let Some(event) = rx.recv().await {
        match output {
            cli::OutputFormat::Human => print_event_human(&event),
            _ => println!("{}", serde_json::to_string(&event)?),
        }
    }

    let report = exec_handle.await??;

    if global.output == cli::OutputFormat::Human {
        print_report_human(&report);
    }

    Ok(if report.failed > 0 { 4 } else { 0 })
}

fn print_cleaner_list(group: Option<&str>, risk: Option<&str>) {
    println!("{:<20}  {:<8}  {:<8}  {}", "ID", "RISK", "GROUP", "NAME");
    println!("{}", "-".repeat(70));
    for entry in wisp_engine::all_cleaners() {
        let m = entry.meta;
        if let Some(g) = group {
            if !format!("{:?}", m.group()).to_lowercase().contains(g) {
                continue;
            }
        }
        if let Some(r) = risk {
            if !format!("{:?}", m.risk()).to_lowercase().contains(r) {
                continue;
            }
        }
        println!(
            "{:<20}  {:<8}  {:<8}  {}",
            m.id(),
            format!("{:?}", m.risk()),
            format!("{:?}", m.group()),
            m.name(),
        );
    }
}

fn print_cleaner_info(target: &str) -> i32 {
    let cleaners = wisp_engine::all_cleaners();
    match cleaners.iter().find(|e| e.meta.id().as_str() == target) {
        Some(entry) => {
            let m = entry.meta;
            println!("ID          {}", m.id());
            println!("Name        {}", m.name());
            println!("Group       {:?}", m.group());
            println!("Risk        {:?}", m.risk());
            println!("Root        {}", m.requires_root());
            println!("Description {}", m.description());
            0
        }
        None => {
            eprintln!("Cleaner '{target}' not found. Try: wisp clean list");
            1
        }
    }
}

fn print_plan_human(plan: &wisp_core::types::CleanPlan, dry_run: bool) {
    let prefix = if dry_run { "[DRY RUN] " } else { "" };
    println!(
        "\n{}Plan  {}  risk={:?}  actions={}",
        prefix,
        plan.id,
        plan.risk,
        plan.actions.len()
    );
    println!("{}", "─".repeat(60));
    for action in &plan.actions {
        match action {
            wisp_core::types::CleanAction::Delete { path, size, via } => {
                println!(
                    "  DELETE  {:>10}  {:?}  {path}",
                    humansize::format_size(*size, humansize::DECIMAL),
                    via,
                );
            }
            wisp_core::types::CleanAction::RunExternal { cmd, estimated_size } => {
                let est = estimated_size
                    .map(|s| humansize::format_size(s, humansize::DECIMAL))
                    .unwrap_or_else(|| "?".into());
                println!(
                    "  RUN     {:>10}  {} {}",
                    est,
                    cmd.program,
                    cmd.args.join(" ")
                );
            }
        }
    }
    println!("{}", "─".repeat(60));
    println!(
        "  Total estimated: {}",
        humansize::format_size(plan.estimated_size, humansize::DECIMAL)
    );
}

fn print_event_human(event: &wisp_core::types::ProgressEvent) {
    use wisp_core::types::ProgressEvent as E;
    match event {
        E::PlanBuilt(s) => println!(
            "  Plan built  {} actions  ~{}",
            s.action_count,
            humansize::format_size(s.estimated_size, humansize::DECIMAL)
        ),
        E::ActionStarted { id } => eprint!("  [{:>4}] … ", id.0),
        E::ActionProgress { id, bytes_done } => {
            eprint!("\r  [{:>4}] {} ", id.0, humansize::format_size(*bytes_done, humansize::DECIMAL));
        }
        E::ActionFinished { id, result } => {
            use wisp_core::types::ActionResult as R;
            match result {
                R::Success { bytes_freed } => println!(
                    "\r  [{:>4}] ✓  {}",
                    id.0,
                    humansize::format_size(*bytes_freed, humansize::DECIMAL)
                ),
                R::Skipped { reason } => println!("\r  [{:>4}] –  skipped ({reason})", id.0),
                R::Failed { error } => println!("\r  [{:>4}] ✗  {error}", id.0),
            }
        }
        E::PlanFinished(r) => {
            println!(
                "\nDone  freed={}  ok={}  skip={}  fail={}",
                humansize::format_size(r.bytes_freed, humansize::DECIMAL),
                r.succeeded,
                r.skipped,
                r.failed,
            );
        }
        E::Warning(msg) => eprintln!("  warn: {msg}"),
    }
}

fn print_report_human(r: &wisp_core::types::CleanReport) {
    println!(
        "\nFreed {}  ({} ok, {} skipped, {} failed)",
        humansize::format_size(r.bytes_freed, humansize::DECIMAL),
        r.succeeded, r.skipped, r.failed,
    );
}

// ─── analyze ─────────────────────────────────────────────────────────────────

async fn dispatch_analyze(args: cli::AnalyzeArgs) -> Result<i32> {
    use wisp_core::scanner::{ScanOptions, format_flat, format_tree, scan_directory};

    let raw_path = args.path.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let canonical = raw_path.canonicalize().map_err(|e| eyre!("{e}: {}", raw_path.display()))?;
    let utf8 = camino::Utf8PathBuf::from_path_buf(canonical)
        .map_err(|p| eyre!("Path is not valid UTF-8: {}", p.display()))?;

    let opts = ScanOptions {
        max_depth: args.depth.map(|d| d as usize),
        min_size: None,
        follow_symlinks: false,
    };

    eprintln!("Scanning {}…", utf8);
    let tree = scan_directory(utf8, opts).await?;

    let depth = args.depth.unwrap_or(2) as usize;
    let top = args.top.unwrap_or(20);

    match args.format.unwrap_or(cli::AnalyzeFormat::Tree) {
        cli::AnalyzeFormat::Tree | cli::AnalyzeFormat::Treemap => {
            print!("{}", format_tree(&tree, depth, 10));
        }
        cli::AnalyzeFormat::Flat => {
            print!("{}", format_flat(&tree, top));
        }
    }
    Ok(0)
}

// ─── history ─────────────────────────────────────────────────────────────────

fn dispatch_history(args: cli::HistoryArgs) -> Result<i32> {
    match args.command.unwrap_or(cli::HistorySubcommand::List { since: None, limit: None }) {
        cli::HistorySubcommand::List { limit, .. } => {
            let entries = wisp_engine::history::read(limit.unwrap_or(20));
            if entries.is_empty() {
                println!("No history yet.");
            } else {
                for r in &entries {
                    println!(
                        "  {}  freed={}  ok={}  fail={}",
                        r.plan_id,
                        humansize::format_size(r.bytes_freed, humansize::DECIMAL),
                        r.succeeded, r.failed,
                    );
                }
            }
        }
        cli::HistorySubcommand::Show { id } => {
            let entries = wisp_engine::history::read(1000);
            match entries.iter().find(|r| r.plan_id.to_string().starts_with(&id)) {
                Some(r) => println!("{}", serde_json::to_string_pretty(r)?),
                None => {
                    eprintln!("History entry '{id}' not found.");
                    return Ok(1);
                }
            }
        }
        cli::HistorySubcommand::Restore { .. } => {
            eprintln!("Restore is only possible for entries deleted via trash (Phase 6).");
        }
        cli::HistorySubcommand::Clear => {
            eprintln!("History clear not yet implemented.");
        }
    }
    Ok(0)
}

// ─── state ───────────────────────────────────────────────────────────────────

fn dispatch_state(args: cli::StateArgs) -> Result<i32> {
    match args.command {
        cli::StateSubcommand::Fav(fav) => {
            match fav.command {
                cli::FavSubcommand::List => println!("Favourites: (none yet – Phase 5)"),
                cli::FavSubcommand::Add { target } => {
                    println!("Add favourite '{target}': not yet implemented.");
                }
                cli::FavSubcommand::Remove { target } => {
                    println!("Remove favourite '{target}': not yet implemented.");
                }
            }
        }
        cli::StateSubcommand::Export { path } => {
            eprintln!("Export to {}: not yet implemented.", path.display());
        }
        cli::StateSubcommand::Import { path } => {
            eprintln!("Import from {}: not yet implemented.", path.display());
        }
    }
    Ok(0)
}

// ─── config ──────────────────────────────────────────────────────────────────

fn dispatch_config(args: cli::ConfigArgs) -> Result<i32> {
    match args.command {
        None | Some(cli::ConfigSubcommand::Path) => {
            match wisp_core::config::Config::config_path() {
                Some(p) => println!("{}", p.display()),
                None => eprintln!("Cannot determine config path."),
            }
        }
        Some(cli::ConfigSubcommand::Show { key: None }) => {
            let cfg = wisp_core::config::Config::load()?;
            print!("{}", toml::to_string_pretty(&cfg).map_err(|e| eyre!("{e}"))?);
        }
        Some(cli::ConfigSubcommand::Show { key: Some(k) }) => {
            eprintln!("Show key '{k}': not yet implemented.");
        }
        Some(cli::ConfigSubcommand::Edit) => {
            let path = wisp_core::config::Config::config_path()
                .ok_or_else(|| eyre!("Cannot find config path"))?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
            std::process::Command::new(&editor).arg(&path).status()?;
        }
        Some(cli::ConfigSubcommand::Set { key, value }) => {
            eprintln!("Set '{key}={value}': not yet implemented.");
        }
        Some(cli::ConfigSubcommand::Reset) => {
            eprintln!("Config reset: not yet implemented.");
        }
    }
    Ok(0)
}

// ─── profile ─────────────────────────────────────────────────────────────────

fn dispatch_profile(_args: cli::ProfileArgs) -> Result<i32> {
    eprintln!("Profile management not yet implemented (Phase 5).");
    Ok(0)
}

// ─── doctor ──────────────────────────────────────────────────────────────────

fn dispatch_doctor() -> Result<i32> {
    let distro = wisp_platform::detect_distro();

    println!("─── wisp doctor ───────────────────────────────────────────────");
    println!("  Distribution:  {} (id={})", distro.name(), distro.id());
    println!("  Platform:      {}", std::env::consts::OS);
    println!("  Architecture:  {}", std::env::consts::ARCH);
    println!("  wisp version:  {}", env!("CARGO_PKG_VERSION"));

    // Config
    match wisp_core::config::Config::config_path() {
        Some(p) => {
            let status = if p.exists() { "found" } else { "not found (defaults used)" };
            println!("  Config:        {} [{status}]", p.display());
        }
        None => println!("  Config:        <path unavailable>"),
    }

    // State dir
    match wisp_core::config::Config::state_dir() {
        Some(p) => {
            let status = if p.exists() { "found" } else { "will be created on first use" };
            println!("  State dir:     {} [{status}]", p.display());
        }
        None => println!("  State dir:     <unavailable>"),
    }

    // Check key tools
    println!();
    println!("  External tools:");
    for tool in &["pacman", "paccache", "journalctl", "flatpak", "docker", "npm", "go"] {
        let found = std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        println!("    {:<12} {}", tool, if found { "✓" } else { "–" });
    }

    println!();
    println!("  Cleaners registered: {}", wisp_engine::all_cleaners().len());
    println!("─────────────────────────────────────────────────────────────");
    Ok(0)
}

// ─── completion ───────────────────────────────────────────────────────────────

fn dispatch_completion(args: cli::CompletionArgs) -> Result<i32> {
    use clap::CommandFactory;
    use clap_complete::generate;

    let mut cmd = cli::Cli::command();
    let shell = match args.shell {
        cli::Shell::Bash => clap_complete::Shell::Bash,
        cli::Shell::Zsh => clap_complete::Shell::Zsh,
        cli::Shell::Fish => clap_complete::Shell::Fish,
        cli::Shell::Elvish => clap_complete::Shell::Elvish,
        cli::Shell::Nu => {
            eprintln!("Nu completion not yet supported by clap_complete.");
            return Ok(1);
        }
    };
    generate(shell, &mut cmd, "wisp", &mut std::io::stdout());
    Ok(0)
}

// ─── man ─────────────────────────────────────────────────────────────────────

fn dispatch_man() -> Result<i32> {
    use clap::CommandFactory;
    let cmd = cli::Cli::command();
    let man = clap_mangen::Man::new(cmd);
    man.render(&mut std::io::stdout())?;
    Ok(0)
}

