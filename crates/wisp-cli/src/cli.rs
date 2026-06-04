//! Clap-based CLI definition (Section 4 of the design doc).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

// ─── Top-level ────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "wisp",
    version,
    author,
    about = "Modern disk cleanup and analysis for Linux",
    long_about = "\
wisp is a modern disk cleanup and analysis tool for Linux.\n\n\
Run without arguments to enter the interactive TUI.\n\n\
EXAMPLES\n\
    wisp clean pacman -n          # dry-run pacman cache cleanup\n\
    wisp clean @user -y           # clean all user targets without prompting\n\
    wisp analyze ~/               # analyse home directory disk usage\n\
    wisp doctor                   # check environment and permissions"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,

    #[command(subcommand)]
    pub command: Option<Command>,
}

// ─── Global options ───────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct GlobalOpts {
    /// Increase log verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all output except errors.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Automatically confirm operations up to Moderate risk.
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,

    /// Preview actions without executing them.
    #[arg(short = 'n', long, global = true)]
    pub dry_run: bool,

    /// Delete directly without moving to the trash.
    #[arg(long, alias = "purge", global = true)]
    pub no_trash: bool,

    /// Disable coloured output.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Output format.
    #[arg(long, value_enum, default_value = "human", global = true)]
    pub output: OutputFormat,

    /// Path to an alternative config file.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Named configuration profile to use.
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text.
    Human,
    /// Single JSON object (`OutputEnvelope<T>`).
    Json,
    /// Streaming JSONL (`ProgressEvent` per line).
    Jsonl,
}

// ─── Commands ────────────────────────────────────────────────────────────────

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Enter the interactive TUI (default when no command given).
    #[command(alias = "ui")]
    Tui(TuiArgs),

    /// Command-mode disk cleaning.
    Clean(CleanArgs),

    /// Disk usage analysis.
    Analyze(AnalyzeArgs),

    /// Deletion history management.
    History(HistoryArgs),

    /// User state management (favourites, export/import).
    State(StateArgs),

    /// Configuration management.
    #[command(name = "config")]
    Config(ConfigArgs),

    /// Named configuration profiles.
    Profile(ProfileArgs),

    /// Environment self-check.
    Doctor,

    /// Generate shell completion scripts.
    Completion(CompletionArgs),

    /// Generate a man page.
    Man,
}

// ─── tui ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct TuiArgs {
    /// Jump directly to a page.
    #[arg(value_enum)]
    pub page: Option<TuiPage>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TuiPage {
    Analyze,
    Clean,
    History,
}

// ─── clean ───────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct CleanArgs {
    #[command(subcommand)]
    pub command: Option<CleanSubcommand>,

    /// Target name (e.g. `pacman`, `@user`) or group (e.g. `@all`).
    pub target: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum CleanSubcommand {
    /// List available clean targets.
    List {
        #[arg(long)]
        group: Option<String>,
        #[arg(long, value_name = "LEVEL")]
        risk: Option<String>,
    },
    /// Show details for a target.
    Info { target: String },
}

// ─── analyze ─────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct AnalyzeArgs {
    /// Path to analyse (defaults to `/`).
    pub path: Option<PathBuf>,

    /// Show top N largest entries.
    #[arg(long, value_name = "N")]
    pub top: Option<usize>,

    /// Maximum directory depth.
    #[arg(long, value_name = "N")]
    pub depth: Option<u32>,

    /// Minimum size filter (e.g. `10MB`).
    #[arg(long, value_name = "SIZE")]
    pub min_size: Option<String>,

    /// Save scan result to cache.
    #[arg(long)]
    pub cache: bool,

    /// Re-use a previously cached scan.
    #[arg(long)]
    pub use_cache: bool,

    /// Visualisation format.
    #[arg(long, value_enum)]
    pub format: Option<AnalyzeFormat>,

    #[command(subcommand)]
    pub command: Option<AnalyzeSubcommand>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AnalyzeFormat {
    Treemap,
    Tree,
    Flat,
}

#[derive(Debug, Subcommand)]
pub enum AnalyzeSubcommand {
    /// Manage saved scan caches.
    Cache(AnalyzeCacheArgs),
}

#[derive(Debug, Args)]
pub struct AnalyzeCacheArgs {
    #[command(subcommand)]
    pub command: AnalyzeCacheSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AnalyzeCacheSubcommand {
    /// List saved scans.
    List,
    /// Delete all saved scans.
    Clear,
}

// ─── history ─────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub command: Option<HistorySubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum HistorySubcommand {
    /// List deletion history.
    List {
        #[arg(long, value_name = "DURATION")]
        since: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Show details of a history entry.
    Show { id: String },
    /// Restore a trashed item from a history entry.
    #[command(alias = "undo")]
    Restore { id: String },
    /// Clear all history.
    Clear,
}

// ─── state ───────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct StateArgs {
    #[command(subcommand)]
    pub command: StateSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum StateSubcommand {
    /// Manage favourite targets and paths.
    Fav(FavArgs),
    /// Export state to a file.
    Export { path: PathBuf },
    /// Import state from a file.
    Import { path: PathBuf },
}

#[derive(Debug, Args)]
pub struct FavArgs {
    #[command(subcommand)]
    pub command: FavSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum FavSubcommand {
    /// Add a target or path to favourites.
    Add { target: String },
    /// List favourites.
    List,
    /// Remove from favourites.
    Remove { target: String },
}

// ─── config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: Option<ConfigSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    /// Print the path to the config file.
    Info,
    /// Open the config file in $EDITOR.
    Edit,
    /// Show config values.
    Show { key: Option<String> },
    /// Set a config value.
    Set { key: String, value: String },
    /// Reset config to defaults.
    Reset,
}

// ─── profile ─────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub command: Option<ProfileSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum ProfileSubcommand {
    /// List profiles.
    List,
    /// Add a new profile.
    Add { name: String },
    /// Remove a profile.
    Remove { name: String },
    /// Show a profile.
    Show { name: String },
    /// Set the active default profile.
    Use { name: String },
}

// ─── completion ───────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct CompletionArgs {
    /// Target shell.
    #[arg(value_enum)]
    pub shell: Shell,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    Nu,
}
