# stytsch

**Persistent, searchable command history for Windows `cmd.exe` — like [Atuin](https://github.com/atuinsh/atuin), but for Windows.**

I got frustrated that `cmd.exe` had nothing like Atuin. Clink gives you a better readline, but its history is a flat file with no fuzzy search, no context, no sync. And Atuin doesn't support Windows cmd at all. So I built stytsch — a Lua + Rust wrapper that hooks into Clink to simulate Atuin's behavior: press **Up Arrow** and get instant fuzzy search over your entire command history, across sessions, with execution metadata.

![demo](https://img.shields.io/badge/status-working-brightgreen)

## How It Works

stytsch is two things:

1. **A Clink Lua plugin** (`stytsch.lua`) that hooks into `cmd.exe` via Clink's readline. It captures every command you run (with exit code, duration, and working directory) and binds Up Arrow / Ctrl+R to launch fuzzy search.

2. **A Rust CLI binary** (`stytsch.exe`) that stores history in a local SQLite database (WAL mode for multi-session concurrency), deduplicates results, and pipes them through fzf for instant fuzzy matching.

```
You type commands → Clink captures them → stytsch records to SQLite
You press Up Arrow → Clink calls stytsch → stytsch pipes history through fzf
You select a command → Enter runs it | Tab pastes it for editing
```

## Features

- **Up Arrow / Ctrl+R** — fuzzy search your entire history via fzf
- **Enter** — select and execute immediately
- **Tab** — select and paste to command line for editing
- **Ctrl+Q** — toggle history tracking on/off mid-session
- **Cross-session** — SQLite with WAL mode, works across multiple terminal windows simultaneously
- **Context-aware** — records working directory, exit code, and duration for every command
- **Auto-pruning** — configurable `max_history` (default 100,000), oldest entries pruned automatically
- **Type-to-run** — type a new command in the search box and press Enter to execute it directly
- **Auto-installs fzf** — if fzf isn't found, stytsch tries to install it via scoop, choco, or winget

## Prerequisites

- **[Clink](https://github.com/chrisant996/clink)** — required. Clink hooks into `cmd.exe` and provides the readline/scripting layer that stytsch plugs into. Install via scoop, choco, or the [installer](https://github.com/chrisant996/clink/releases).
- **[fzf](https://github.com/junegunn/fzf)** — required for fuzzy search. stytsch will try to auto-install it, or install manually:

  | Package Manager | Command |
  |---|---|
  | Scoop | `scoop install fzf` |
  | Chocolatey | `choco install fzf` |
  | Winget | `winget install fzf` |
  | MSYS2 | `pacman -S $MINGW_PACKAGE_PREFIX-fzf` |

## Installation

### From source (requires Rust)

```
git clone https://github.com/yourusername/stytsch
cd stytsch
cargo install --path .
stytsch install
```

### What `stytsch install` does

1. Copies `stytsch.lua` to Clink's scripts directory (`%LOCALAPPDATA%\clink\`)
2. Enables `cmd.get_errorlevel` in Clink settings (for exit code tracking)
3. Creates the config and database in `%LOCALAPPDATA%\stytsch\`

Open a **new `cmd.exe` window** after installing — Clink loads scripts at startup.

## Usage

### In the terminal

Just use `cmd.exe` normally. stytsch records every command in the background.

| Key | Action |
|---|---|
| **Up Arrow** | Open fuzzy search |
| **Ctrl+R** | Open fuzzy search (alternative) |
| **Enter** (in search) | Select and execute immediately |
| **Tab** (in search) | Select and paste for editing |
| **Esc** (in search) | Cancel |
| **Ctrl+Q** | Toggle history tracking on/off |

You can also type a new command directly in the search box — press Enter to run it even if it doesn't match any history.

### CLI commands

```
stytsch search              # Open fuzzy search (default when run with no args)
stytsch search --fzf        # Force fzf backend
stytsch history list        # Show recent history
stytsch history list -c 50  # Show last 50 commands
stytsch history list --cwd C:\Projects  # Filter by directory
stytsch history delete <id> # Soft-delete an entry
stytsch stats               # Top commands, total count, database size
stytsch prune               # Remove entries beyond max_history
stytsch prune --keep 5000   # Keep only 5000 most recent
stytsch config show         # Show current configuration
stytsch config path         # Print config file location
stytsch install             # Install the Clink plugin
stytsch uninstall           # Remove the Clink plugin (keeps history)
```

## Configuration

Config file: `%LOCALAPPDATA%\stytsch\config.toml`

```toml
# Search backend: "fzf" (default), "fuzzy" (built-in TUI), or "auto"
search_mode = "fzf"

# Maximum history entries to keep. Oldest are auto-pruned.
max_history = 100000

# Sync server URL (future feature)
# sync_server = "https://your-server.com"
```

## Data Storage

- **Database:** `%LOCALAPPDATA%\stytsch\history.db` (SQLite, WAL mode)
- **Config:** `%LOCALAPPDATA%\stytsch\config.toml`
- **Clink plugin:** `%LOCALAPPDATA%\clink\stytsch.lua`

Each history entry stores:

| Field | Description |
|---|---|
| `id` | UUID for deduplication |
| `command` | The exact command text |
| `cwd` | Working directory at execution time |
| `exit` | Exit code (`%ERRORLEVEL%`) |
| `duration` | How long the command took |
| `timestamp` | When it was executed (nanoseconds) |
| `session` | Terminal session identifier |
| `hostname` | Machine name (for future sync) |

## Multi-Session Support

SQLite WAL (Write-Ahead Logging) mode allows multiple `cmd.exe` windows to read and write the same database simultaneously without locking. The Rust binary includes retry logic for the rare case of write contention under heavy load.

## Uninstalling

```
stytsch uninstall
```

This removes the Clink plugin. Your history database is preserved at `%LOCALAPPDATA%\stytsch\history.db`.

To fully remove:
```
stytsch uninstall
cargo uninstall stytsch
rmdir /s /q %LOCALAPPDATA%\stytsch
```

## Architecture

```
stytsch/
├── Cargo.toml
├── scripts/stytsch.lua     # Clink plugin (also embedded in binary)
└── src/
    ├── main.rs             # CLI: search, record, history, stats, prune, install
    ├── config.rs           # TOML config
    ├── db.rs               # SQLite + WAL, insert/search/prune with retry
    ├── schema.rs           # History struct, search filters
    ├── search.rs           # fzf integration with --expect and --print-query
    └── tui.rs              # Built-in ratatui TUI (fallback when fzf unavailable)
```

~700 lines of Rust + ~160 lines of Lua. One crate, no workspace complexity.

## Why not just use Atuin?

Atuin is excellent — on macOS and Linux. But it doesn't support Windows `cmd.exe`. Atuin hooks into POSIX shells (bash, zsh, fish) via native shell hooks (`precmd`, `preexec`). Windows `cmd.exe` has no such hooks — it's a closed, legacy binary that Microsoft won't modify.

stytsch solves this by leveraging **Clink**, which injects a DLL into `cmd.exe` to replace its readline. Clink provides the Lua scripting API that stytsch uses to intercept commands and key bindings. The Rust binary handles everything else — database, search, deduplication, pruning.

## License

MIT
