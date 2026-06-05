//! `wisp` binary entry point.

use std::process;
use std::sync::Arc;

use clap::Parser;
use color_eyre::eyre::{Result, eyre};
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

mod cli;
mod confirmer;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
    #[allow(clippy::expect_used)]
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

    let span = tracing::info_span!("wisp.run");
    let _g = span.enter();

    let cfg = match &cli.global.config {
        Some(p) => wisp_engine::config::Config::load_from(p)?,
        None => wisp_engine::config::Config::load()?,
    };

    let distro: Arc<dyn wisp_engine::Distro> = Arc::from(wisp_engine::detect_distro());

    let code = match cli.command {
        None | Some(cli::Command::Tui(_)) => dispatch_tui().await?,
        Some(cli::Command::Clean(args)) => dispatch_clean(args, &cli.global, &cfg, distro).await?,
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
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(!no_color),
        )
        .with(ErrorLayer::default())
        .init();
    Ok(())
}

// ─── tui ─────────────────────────────────────────────────────────────────────

async fn dispatch_tui() -> Result<i32> {
    wisp_tui::run_tui().await.map_err(|e| eyre!("{e}"))?;
    Ok(0)
}

fn not_implemented(feature: impl std::fmt::Display) -> i32 {
    eprintln!("{feature}: not implemented yet.");
    70
}

// ─── clean ───────────────────────────────────────────────────────────────────

async fn dispatch_clean(
    args: cli::CleanArgs,
    global: &cli::GlobalOpts,
    cfg: &wisp_engine::config::Config,
    distro: Arc<dyn wisp_engine::Distro>,
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

    let apply = args.apply && !args.dry_run;
    let dry_run = !apply;
    let output = args.output;
    let target_label = match args.target.as_deref() {
        Some(target) => target.to_owned(),
        None => "recommended".to_owned(),
    };
    let recommended = args.target.is_none();

    let engine_cfg = wisp_engine::EngineConfig {
        dry_run,
        prefer_trash: cfg.clean.prefer_trash && !args.no_trash,
        auto_approve_up_to: if apply {
            wisp_engine::types::RiskLevel::Moderate
        } else {
            wisp_engine::types::RiskLevel::Safe
        },
    };
    let engine = Arc::new(wisp_engine::Engine::new(engine_cfg, distro));
    let targets = match args.target.as_deref() {
        Some(target) => vec![target],
        None => vec!["@user", "@dev"],
    };

    let show_progress = !global.quiet && output == cli::OutputFormat::Human;
    if show_progress {
        eprintln!("Building plan for '{target_label}'...");
    }
    let mut plan = engine.build_plan(&targets).await?;
    if args.target.is_none() && !args.deep {
        plan = without_dangerous_actions(plan);
    }
    if show_progress {
        eprintln!("Plan ready.");
    }

    if plan.actions.is_empty() && output == cli::OutputFormat::Human {
        println!("Nothing to clean for '{target_label}'.");
        return Ok(0);
    }

    match output {
        cli::OutputFormat::Human => print_plan_human(
            &plan,
            CleanDisplayOptions {
                target_label: &target_label,
                dry_run,
                recommended,
                deep: args.deep,
            },
        ),
        cli::OutputFormat::Json => {
            let env =
                wisp_engine::types::OutputEnvelope::new(format!("clean {target_label}"), &plan)
                    .with_warnings(plan.warnings.clone());
            println!("{}", serde_json::to_string_pretty(&env)?);
        }
        cli::OutputFormat::Jsonl => {
            let summary = wisp_engine::types::CleanPlanSummary::from(&plan);
            println!(
                "{}",
                serde_json::to_string(&wisp_engine::types::ProgressEvent::PlanBuilt(summary))?
            );
        }
    }

    if dry_run {
        return Ok(0);
    }

    // Choose confirmer
    let cfm: Arc<dyn wisp_engine::types::Confirmer> = if apply {
        Arc::new(confirmer::AutoConfirmer {
            approve_dangerous: false,
        })
    } else {
        Arc::new(confirmer::CliConfirmer)
    };

    // Execute and stream events
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let engine_clone = engine.clone();
    let plan_clone = plan.clone();
    let cfm_clone = cfm.clone();

    let exec_handle =
        tokio::spawn(async move { engine_clone.execute(plan_clone, cfm_clone, tx).await });

    while let Some(event) = rx.recv().await {
        match output {
            cli::OutputFormat::Human => print_event_human(&event),
            _ => println!("{}", serde_json::to_string(&event)?),
        }
    }

    let report = exec_handle.await??;

    if output == cli::OutputFormat::Human {
        print_report_human(&report);
    }

    Ok(if report.failed > 0 { 4 } else { 0 })
}

fn without_dangerous_actions(plan: wisp_engine::types::CleanPlan) -> wisp_engine::types::CleanPlan {
    let mut actions = Vec::new();
    let mut risks = Vec::new();

    for (idx, action) in plan.actions.into_iter().enumerate() {
        let risk = plan.risks.get(idx).copied().unwrap_or(plan.risk);
        if risk == wisp_engine::types::RiskLevel::Dangerous {
            continue;
        }
        actions.push(action);
        risks.push(risk);
    }

    let estimated_size = actions
        .iter()
        .map(|action| match action {
            wisp_engine::types::CleanAction::Delete { size, .. } => *size,
            wisp_engine::types::CleanAction::RunExternal { estimated_size, .. } => {
                estimated_size.unwrap_or(0)
            }
        })
        .sum();
    let risk = risks
        .iter()
        .copied()
        .max()
        .unwrap_or(wisp_engine::types::RiskLevel::Trivial);

    wisp_engine::types::CleanPlan {
        actions,
        risks,
        estimated_size,
        risk,
        ..plan
    }
}

fn print_cleaner_list(group: Option<&str>, risk: Option<&str>) {
    print!("{}", format_cleaner_list(group, risk));
}

fn format_cleaner_list(group: Option<&str>, risk: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<22}  {:<8}  {:<9}  {:<4}  NAME\n",
        "ID", "GROUP", "RISK", "ROOT"
    ));
    out.push_str(&format!("{}\n", "-".repeat(82)));

    for entry in wisp_engine::all_cleaners() {
        let m = entry.meta;
        let group_text = group_label(m.group());
        let risk_text = risk_label(m.risk());
        if let Some(g) = group
            && !group_text.contains(&g.to_lowercase())
        {
            continue;
        }
        if let Some(r) = risk
            && !risk_text.contains(&r.to_lowercase())
        {
            continue;
        }
        out.push_str(&format!(
            "{:<22}  {:<8}  {:<9}  {:<4}  {}\n",
            m.id(),
            group_text,
            risk_text,
            bool_label(m.requires_root()),
            m.name(),
        ));
    }

    out.push_str("\nFilters: wisp clean list --group dev --risk safe\n");
    out.push_str("Inspect: wisp clean info <id>\n");
    out
}

fn print_cleaner_info(target: &str) -> i32 {
    match format_cleaner_info(target) {
        Ok(rendered) => {
            print!("{rendered}");
            0
        }
        Err(CleanerInfoError::NotFound) => {
            eprintln!("Cleaner '{target}' not found.");
            eprintln!("List cleaners: wisp clean list");
            1
        }
        Err(CleanerInfoError::Ambiguous(matches)) => {
            eprintln!("Target '{target}' matched multiple cleaners:");
            for id in matches {
                eprintln!("  {id}");
            }
            eprintln!("Use a full cleaner ID. List cleaners: wisp clean list");
            64
        }
    }
}

enum CleanerInfoError {
    NotFound,
    Ambiguous(Vec<String>),
}

fn format_cleaner_info(target: &str) -> Result<String, CleanerInfoError> {
    let entries = wisp_engine::resolve_targets(&[target]);
    match entries.as_slice() {
        [entry] => {
            let m = entry.meta;
            let id = m.id();
            let mut out = String::new();
            out.push_str(&format!("ID          {id}\n"));
            out.push_str(&format!("Name        {}\n", m.name()));
            out.push_str(&format!("Group       {}\n", group_label(m.group())));
            out.push_str(&format!("Risk        {}\n", risk_label(m.risk())));
            out.push_str(&format!("Root        {}\n", bool_label(m.requires_root())));
            out.push_str(&format!("Description {}\n", m.description()));
            out.push_str(&format!("Preview     wisp clean {id}\n"));
            out.push_str(&format!("Apply       wisp clean {id} --apply\n"));
            Ok(out)
        }
        [] => Err(CleanerInfoError::NotFound),
        matches => Err(CleanerInfoError::Ambiguous(
            matches
                .iter()
                .map(|entry| entry.meta.id().to_string())
                .collect(),
        )),
    }
}

const HUMAN_PREVIEW_LIMIT: usize = 8;

#[derive(Clone, Copy)]
struct CleanDisplayOptions<'a> {
    target_label: &'a str,
    dry_run: bool,
    recommended: bool,
    deep: bool,
}

fn print_plan_human(plan: &wisp_engine::types::CleanPlan, options: CleanDisplayOptions<'_>) {
    print!("{}", format_plan_human(plan, options));
}

fn format_plan_human(
    plan: &wisp_engine::types::CleanPlan,
    options: CleanDisplayOptions<'_>,
) -> String {
    let title = if options.dry_run { "Preview" } else { "Plan" };
    let mode = if options.dry_run { "preview" } else { "apply" };
    let mut out = String::new();

    out.push_str(&format!("{title}: {}\n", options.target_label));
    out.push_str(&format!(
        "Mode: {mode}  Actions: {}  Risk: {}  Estimated reclaim: {}\n",
        plan.actions.len(),
        risk_label(plan.risk),
        humansize::format_size(plan.estimated_size, humansize::DECIMAL)
    ));
    out.push_str(&format!(
        "Root required: {}\n",
        bool_label(plan.required_privileges.requires_root)
    ));
    if options.recommended && !options.deep {
        out.push_str("High-risk actions are excluded. Use `wisp clean --deep` to preview them.\n");
    }
    if !plan.warnings.is_empty() {
        out.push_str("\nWarnings\n");
        out.push_str("--------\n");
        for warning in &plan.warnings {
            out.push_str(&format!("  - {warning}\n"));
        }
    }

    out.push_str("\nFiles and directories\n");
    out.push_str("---------------------\n");
    render_delete_actions(plan, &mut out);

    out.push_str("\nExternal commands\n");
    out.push_str("-----------------\n");
    render_external_actions(plan, &mut out);

    if options.dry_run {
        out.push_str("\nNext steps\n");
        out.push_str("----------\n");
        out.push_str("  Run: wisp clean --apply\n");
        out.push_str("  Inspect: wisp clean list\n");
        out.push_str("  Include high risk: wisp clean --deep\n");
    }

    out
}

fn render_delete_actions(plan: &wisp_engine::types::CleanPlan, out: &mut String) {
    let mut total = 0usize;
    for (idx, action) in plan.actions.iter().enumerate() {
        let wisp_engine::types::CleanAction::Delete { path, size, via } = action else {
            continue;
        };
        total += 1;
        if total <= HUMAN_PREVIEW_LIMIT {
            let risk = plan.risks.get(idx).copied().unwrap_or(plan.risk);
            out.push_str(&format!(
                "  {:>10}  {:<8}  {:<8}  {path}\n",
                humansize::format_size(*size, humansize::DECIMAL),
                risk_label(risk),
                deletion_via_label(*via)
            ));
        }
    }
    append_hidden_count(out, total, "delete action");
}

fn render_external_actions(plan: &wisp_engine::types::CleanPlan, out: &mut String) {
    let mut total = 0usize;
    for (idx, action) in plan.actions.iter().enumerate() {
        let wisp_engine::types::CleanAction::RunExternal {
            cmd,
            estimated_size,
        } = action
        else {
            continue;
        };
        total += 1;
        if total <= HUMAN_PREVIEW_LIMIT {
            let risk = plan.risks.get(idx).copied().unwrap_or(plan.risk);
            let est = estimated_size
                .map(|s| humansize::format_size(s, humansize::DECIMAL))
                .unwrap_or_else(|| "unknown".into());
            out.push_str(&format!(
                "  {:>10}  {:<8}  {} {}\n",
                est,
                risk_label(risk),
                cmd.program,
                cmd.args.join(" ")
            ));
        }
    }
    append_hidden_count(out, total, "external command");
}

fn append_hidden_count(out: &mut String, total: usize, label: &str) {
    if total == 0 {
        out.push_str("  none\n");
    } else if total > HUMAN_PREVIEW_LIMIT {
        out.push_str(&format!(
            "  ... and {} more {}{}\n",
            total - HUMAN_PREVIEW_LIMIT,
            label,
            if total - HUMAN_PREVIEW_LIMIT == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
}

fn risk_label(risk: wisp_engine::types::RiskLevel) -> &'static str {
    match risk {
        wisp_engine::types::RiskLevel::Trivial => "trivial",
        wisp_engine::types::RiskLevel::Safe => "safe",
        wisp_engine::types::RiskLevel::Moderate => "moderate",
        wisp_engine::types::RiskLevel::Dangerous => "dangerous",
    }
}

fn group_label(group: wisp_engine::types::CleanerGroup) -> &'static str {
    match group {
        wisp_engine::types::CleanerGroup::System => "system",
        wisp_engine::types::CleanerGroup::User => "user",
        wisp_engine::types::CleanerGroup::Dev => "dev",
    }
}

fn deletion_via_label(via: wisp_engine::types::DeletionVia) -> &'static str {
    match via {
        wisp_engine::types::DeletionVia::Trash => "trash",
        wisp_engine::types::DeletionVia::Direct => "direct",
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn print_event_human(event: &wisp_engine::types::ProgressEvent) {
    use wisp_engine::types::ProgressEvent as E;
    match event {
        E::PlanBuilt(s) => println!(
            "  Plan built  {} actions  ~{}",
            s.action_count,
            humansize::format_size(s.estimated_size, humansize::DECIMAL)
        ),
        E::ActionStarted { id } => eprint!("  [{:>4}] … ", id.0),
        E::ActionProgress { id, bytes_done } => {
            eprint!(
                "\r  [{:>4}] {} ",
                id.0,
                humansize::format_size(*bytes_done, humansize::DECIMAL)
            );
        }
        E::ActionFinished { id, result } => {
            use wisp_engine::types::ActionResult as R;
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

fn print_report_human(r: &wisp_engine::types::CleanReport) {
    println!(
        "\nFreed {}  ({} ok, {} skipped, {} failed)",
        humansize::format_size(r.bytes_freed, humansize::DECIMAL),
        r.succeeded,
        r.skipped,
        r.failed,
    );
}

// ─── analyze ─────────────────────────────────────────────────────────────────

async fn dispatch_analyze(args: cli::AnalyzeArgs) -> Result<i32> {
    use wisp_engine::scanner::{ScanOptions, format_flat, format_tree, scan_directory};

    let raw_path = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let canonical = raw_path
        .canonicalize()
        .map_err(|e| eyre!("{e}: {}", raw_path.display()))?;
    wisp_engine::fs::check_blacklist(&canonical).map_err(|e| eyre!("{e}"))?;
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
    match args.command.unwrap_or(cli::HistorySubcommand::List {
        since: None,
        limit: None,
    }) {
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
                        r.succeeded,
                        r.failed,
                    );
                }
            }
        }
        cli::HistorySubcommand::Show { id } => {
            let entries = wisp_engine::history::read(1000);
            match entries
                .iter()
                .find(|r| r.plan_id.to_string().starts_with(&id))
            {
                Some(r) => println!("{}", serde_json::to_string_pretty(r)?),
                None => {
                    eprintln!("History entry '{id}' not found.");
                    return Ok(1);
                }
            }
        }
        cli::HistorySubcommand::Restore { id } => {
            return Ok(not_implemented(format_args!("history restore {id}")));
        }
        cli::HistorySubcommand::Clear => {
            return Ok(not_implemented("history clear"));
        }
    }
    Ok(0)
}

// ─── state ───────────────────────────────────────────────────────────────────

fn dispatch_state(args: cli::StateArgs) -> Result<i32> {
    match args.command {
        cli::StateSubcommand::Fav(fav) => match fav.command {
            cli::FavSubcommand::List => println!("Favourites: (none yet – Phase 5)"),
            cli::FavSubcommand::Add { target } => {
                return Ok(not_implemented(format_args!("state fav add {target}")));
            }
            cli::FavSubcommand::Remove { target } => {
                return Ok(not_implemented(format_args!("state fav remove {target}")));
            }
        },
        cli::StateSubcommand::Export { path } => {
            return Ok(not_implemented(format_args!(
                "state export {}",
                path.display()
            )));
        }
        cli::StateSubcommand::Import { path } => {
            return Ok(not_implemented(format_args!(
                "state import {}",
                path.display()
            )));
        }
    }
    Ok(0)
}

// ─── config ──────────────────────────────────────────────────────────────────

fn dispatch_config(args: cli::ConfigArgs) -> Result<i32> {
    match args.command {
        None | Some(cli::ConfigSubcommand::Info) => {
            match wisp_engine::config::Config::config_path() {
                Some(p) => println!("{}", p.display()),
                None => eprintln!("Cannot determine config path."),
            }
        }
        Some(cli::ConfigSubcommand::Show { key: None }) => {
            let cfg = wisp_engine::config::Config::load()?;
            print!(
                "{}",
                toml::to_string_pretty(&cfg).map_err(|e| eyre!("{e}"))?
            );
        }
        Some(cli::ConfigSubcommand::Show { key: Some(k) }) => {
            return Ok(not_implemented(format_args!("config show {k}")));
        }
        Some(cli::ConfigSubcommand::Edit) => {
            let path = wisp_engine::config::Config::config_path()
                .ok_or_else(|| eyre!("Cannot find config path"))?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
            std::process::Command::new(&editor).arg(&path).status()?;
        }
        Some(cli::ConfigSubcommand::Set { key, value }) => {
            return Ok(not_implemented(format_args!("config set {key}={value}")));
        }
        Some(cli::ConfigSubcommand::Reset) => {
            return Ok(not_implemented("config reset"));
        }
    }
    Ok(0)
}

// ─── profile ─────────────────────────────────────────────────────────────────

fn dispatch_profile(_args: cli::ProfileArgs) -> Result<i32> {
    Ok(not_implemented("profile management"))
}

// ─── doctor ──────────────────────────────────────────────────────────────────

fn dispatch_doctor() -> Result<i32> {
    let distro = wisp_engine::detect_distro();

    println!("─── wisp doctor ───────────────────────────────────────────────");
    println!("  Distribution:  {} (id={})", distro.name(), distro.id());
    println!("  Platform:      {}", std::env::consts::OS);
    println!("  Architecture:  {}", std::env::consts::ARCH);
    println!("  wisp version:  {}", env!("CARGO_PKG_VERSION"));

    // Config
    match wisp_engine::config::Config::config_path() {
        Some(p) => {
            let status = if p.exists() {
                "found"
            } else {
                "not found (defaults used)"
            };
            println!("  Config:        {} [{status}]", p.display());
        }
        None => println!("  Config:        <path unavailable>"),
    }

    // State dir
    match wisp_engine::config::Config::state_dir() {
        Some(p) => {
            let status = if p.exists() {
                "found"
            } else {
                "will be created on first use"
            };
            println!("  State dir:     {} [{status}]", p.display());
        }
        None => println!("  State dir:     <unavailable>"),
    }

    // Check key tools
    println!();
    println!("  External tools:");
    for tool in &[
        "pacman",
        "paccache",
        "journalctl",
        "flatpak",
        "docker",
        "npm",
        "go",
    ] {
        let found = std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        println!("    {:<12} {}", tool, if found { "✓" } else { "–" });
    }

    println!();
    println!(
        "  Cleaners registered: {}",
        wisp_engine::all_cleaners().len()
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_engine::types::CleanPlan;

    fn sample_plan() -> CleanPlan {
        let value = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000000",
            "actions": [
                {
                    "kind": "delete",
                    "path": "/tmp/a",
                    "size": 1024,
                    "via": "direct"
                },
                {
                    "kind": "run_external",
                    "cmd": {
                        "program": "npm",
                        "args": ["cache", "clean", "--force"]
                    },
                    "estimated_size": null
                }
            ],
            "risks": ["safe", "moderate"],
            "estimated_size": 1024,
            "required_privileges": {
                "requires_root": false
            },
            "risk": "moderate",
            "warnings": []
        });
        match serde_json::from_value(value) {
            Ok(plan) => plan,
            Err(err) => panic!("sample plan must deserialize: {err}"),
        }
    }

    #[test]
    fn human_preview_starts_with_summary_and_next_steps() {
        let rendered = format_plan_human(
            &sample_plan(),
            CleanDisplayOptions {
                target_label: "recommended",
                dry_run: true,
                recommended: true,
                deep: false,
            },
        );

        assert!(rendered.contains("Preview: recommended"));
        assert!(rendered.contains("Estimated reclaim:"));
        assert!(rendered.contains("Files and directories"));
        assert!(rendered.contains("External commands"));
        assert!(rendered.contains("Run: wisp clean --apply"));
        assert!(rendered.contains("Include high risk: wisp clean --deep"));
    }

    #[test]
    fn cleaner_list_footer_explains_filters_and_info() {
        let rendered = format_cleaner_list(None, None);

        assert!(rendered.contains("wisp clean list --group dev"));
        assert!(rendered.contains("wisp clean info <id>"));
        assert!(rendered.contains("ROOT"));
    }

    #[test]
    fn cleaner_info_includes_preview_command() {
        let rendered = match format_cleaner_info("dev.npm") {
            Ok(rendered) => rendered,
            Err(_) => panic!("dev.npm cleaner exists"),
        };

        assert!(rendered.contains("Preview"));
        assert!(rendered.contains("wisp clean dev.npm"));
    }
}
