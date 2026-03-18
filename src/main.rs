mod config;
mod db;
mod schema;
mod search;
#[cfg(test)]
mod tests;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};

use config::Config;
use db::Database;
use schema::SearchFilter;

#[derive(Parser)]
#[command(
    name = "stytsch",
    about = "Persistent command history for Windows cmd.exe — works across sessions",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Open fuzzy search to find and recall commands (default).
    Search {
        /// Optional initial search query.
        query: Option<String>,
        /// Use fzf instead of the built-in TUI.
        #[arg(long)]
        fzf: bool,
    },

    /// List or manage history entries.
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },

    /// Show usage statistics.
    Stats,

    /// Show or edit configuration.
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Record a command execution (called by Clink plugin).
    Record {
        #[arg(long)]
        command: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long, default_value = "0")]
        exit: i32,
        #[arg(long, default_value = "0")]
        duration: i64,
    },

    /// Install the Clink plugin for automatic history tracking.
    Install,

    /// Uninstall the Clink plugin.
    Uninstall,

    /// Prune old history entries beyond the configured max.
    Prune {
        /// Keep this many entries (overrides config).
        #[arg(long)]
        keep: Option<usize>,
    },
}

#[derive(Subcommand)]
enum HistoryAction {
    /// List recent history entries.
    List {
        #[arg(short, long, default_value = "25")]
        count: usize,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        host: Option<String>,
    },
    /// Delete a history entry by ID.
    Delete { id: String },
}

#[derive(Subcommand)]
enum ConfigAction {
    Path,
    Show,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("stytsch=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        None => cmd_search(None, false),
        Some(Commands::Search { query, fzf }) => cmd_search(query, fzf),
        Some(Commands::History { action }) => cmd_history(action),
        Some(Commands::Stats) => cmd_stats(),
        Some(Commands::Config { action }) => cmd_config(action),
        Some(Commands::Record { command, file, cwd, exit, duration }) => {
            cmd_record(command, file, cwd, exit, duration)
        }
        Some(Commands::Install) => cmd_install(),
        Some(Commands::Uninstall) => cmd_uninstall(),
        Some(Commands::Prune { keep }) => cmd_prune(keep),
    }
}

fn cmd_search(query: Option<String>, use_fzf: bool) -> Result<()> {
    let db = Database::open()?;
    let config = Config::load()?;

    let should_use_fzf = use_fzf
        || config.search_mode == "fzf"
        || (config.search_mode == "auto" && search::which_fzf().is_some());

    if should_use_fzf {
        match search::search_with_fzf(&db, query.as_deref())? {
            search::FzfResult::Execute(cmd) => print!("EXEC:{cmd}"),
            search::FzfResult::Edit(cmd) => print!("EDIT:{cmd}"),
            search::FzfResult::Cancelled => {}
        }
    } else {
        if let Some(cmd) = tui::standalone_search(&db, query.as_deref())? {
            print!("EDIT:{cmd}");
        }
    }

    Ok(())
}

fn cmd_history(action: HistoryAction) -> Result<()> {
    let db = Database::open()?;

    match action {
        HistoryAction::List { count, cwd, host } => {
            let filter = SearchFilter {
                cwd,
                hostname: host,
                limit: count,
                ..Default::default()
            };
            let entries = db.search(&filter)?;

            if entries.is_empty() {
                println!("No history entries found.");
                return Ok(());
            }

            println!("{:<8} {:<6} {:<4} {:<40} {}", "TIME", "DUR", "EXIT", "COMMAND", "CWD");
            println!("{}", "-".repeat(100));

            for h in &entries {
                let dur_ms = h.duration / 1_000_000;
                let dur = if dur_ms < 1000 {
                    format!("{dur_ms}ms")
                } else {
                    format!("{:.1}s", dur_ms as f64 / 1000.0)
                };
                let cmd = if h.command.len() > 38 {
                    format!("{}...", &h.command[..35])
                } else {
                    h.command.clone()
                };
                println!("{:<8} {:<6} {:<4} {:<40} {}", format_relative(h.timestamp), dur, h.exit, cmd, h.cwd);
            }
        }
        HistoryAction::Delete { id } => {
            db.soft_delete(&id, epoch_nanos())?;
            println!("Deleted: {id}");
        }
    }

    Ok(())
}

fn cmd_stats() -> Result<()> {
    let db = Database::open()?;
    let total = db.count()?;
    let db_size = std::fs::metadata(Config::db_path())
        .map(|m| m.len())
        .unwrap_or(0);

    println!("stytsch statistics:");
    println!("  Total commands:  {total}");
    println!("  Database:        {}", Config::db_path().display());
    println!("  Database size:   {}", format_bytes(db_size));
    println!("  Max history:     {}", Config::load().map(|c| c.max_history).unwrap_or(100_000));

    let filter = SearchFilter::new(10000);
    let entries = db.search(&filter)?;

    if entries.is_empty() {
        println!("  No history recorded yet.");
        return Ok(());
    }

    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for h in &entries {
        let cmd_name = h.command.split_whitespace().next().unwrap_or(&h.command);
        *freq.entry(cmd_name.to_string()).or_default() += 1;
    }

    let mut sorted: Vec<_> = freq.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    println!("\n  Top 10 commands:");
    for (i, (cmd, count)) in sorted.iter().take(10).enumerate() {
        println!("    {}. {cmd} ({count})", i + 1);
    }

    Ok(())
}

fn cmd_config(action: Option<ConfigAction>) -> Result<()> {
    match action {
        None | Some(ConfigAction::Show) => {
            let config = Config::load()?;
            println!("{}", toml::to_string_pretty(&config)?);
        }
        Some(ConfigAction::Path) => {
            println!("{}", Config::config_path().display());
        }
    }
    Ok(())
}

fn cmd_record(
    command: Option<String>,
    file: Option<String>,
    cwd: Option<String>,
    exit: i32,
    duration: i64,
) -> Result<()> {
    let command_text = if let Some(path) = file {
        std::fs::read_to_string(&path)?
    } else if let Some(cmd) = command {
        cmd
    } else {
        anyhow::bail!("Either --command or --file must be provided");
    };

    let command_text = command_text.trim().to_string();
    if command_text.is_empty() {
        return Ok(());
    }

    let db = Database::open()?;
    let config = Config::load()?;

    let now = epoch_nanos();
    let duration_ns = duration * 1_000_000_000;

    let entry = schema::History {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: now - duration_ns,
        duration: duration_ns,
        exit,
        command: command_text,
        cwd: cwd.unwrap_or_else(|| ".".to_string()),
        session: std::env::var("STYTSCH_SESSION")
            .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()),
        hostname: hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
        deleted_at: None,
    };

    db.insert(&entry)?;

    // Auto-prune if over max.
    let count = db.count()?;
    if count > config.max_history {
        let excess = count - config.max_history;
        db.prune_oldest(excess)?;
    }

    Ok(())
}

fn cmd_install() -> Result<()> {
    let clink_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("clink");

    if !clink_dir.exists() {
        println!("Clink profile directory not found at: {}", clink_dir.display());
        println!("Install Clink first: https://github.com/chrisant996/clink");
        return Ok(());
    }

    let script_dst = clink_dir.join("stytsch.lua");
    std::fs::write(&script_dst, include_str!("../scripts/stytsch.lua"))?;
    println!("[OK] Wrote Lua script to: {}", script_dst.display());

    println!("[..] Enabling cmd.get_errorlevel...");
    let _ = std::process::Command::new("clink")
        .args(["set", "cmd.get_errorlevel", "true"])
        .status();
    println!("[OK] Done.");

    let _ = Config::load()?;
    let _ = Database::open()?;
    println!("[OK] Config and database initialized.");

    let in_path = std::process::Command::new("where")
        .arg("stytsch")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !in_path {
        let exe = std::env::current_exe()?;
        println!();
        println!("[WARN] stytsch not in PATH. Add: {}", exe.parent().unwrap().display());
    }

    println!();
    println!("Done! Open a new cmd.exe to activate.");
    println!("  Up Arrow / Ctrl+R  -> fuzzy search");
    println!("  Ctrl+Q             -> toggle tracking");
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    let script = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("clink")
        .join("stytsch.lua");

    if script.exists() {
        std::fs::remove_file(&script)?;
        println!("[OK] Removed: {}", script.display());
    } else {
        println!("Script not found at: {}", script.display());
    }

    println!("History preserved at: {}", Config::db_path().display());
    Ok(())
}

fn cmd_prune(keep: Option<usize>) -> Result<()> {
    let db = Database::open()?;
    let config = Config::load()?;
    let max = keep.unwrap_or(config.max_history);
    let count = db.count()?;

    if count <= max {
        println!("Nothing to prune. {count} entries <= {max} max.");
        return Ok(());
    }

    let excess = count - max;
    db.prune_oldest(excess)?;
    println!("Pruned {excess} old entries. {max} remaining.");
    Ok(())
}

fn epoch_nanos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos() as i64
}

fn format_relative(timestamp_ns: i64) -> String {
    let diff = (epoch_nanos() - timestamp_ns) / 1_000_000_000;
    if diff < 60 { format!("{diff}s") }
    else if diff < 3600 { format!("{}m", diff / 60) }
    else if diff < 86400 { format!("{}h", diff / 3600) }
    else { format!("{}d", diff / 86400) }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 { format!("{bytes} B") }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)) }
}
