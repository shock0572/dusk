# DUSK — Disk Usage Survey Kit

A fast, interactive terminal disk usage analyzer. Think **ncdu**, but built in Rust for speed and cross-platform support (Windows, Linux, macOS).

## Features

- **Parallel scanning** — uses all CPU cores via Rayon for fast directory traversal
- **Interactive TUI** — browse directories, drill in/out, see what's eating your disk
- **Size bars** — visual proportional bars with color coding (green/yellow/red)
- **Multiple sort modes** — sort by size, name, or item count
- **Delete files** — remove files/directories directly from the UI with confirmation
- **Live scan progress** — animated spinner with real-time file count and size
- **Symlink-safe** — skips symbolic links to avoid loops
- **Single binary** — no runtime dependencies, just copy and run

## Installation

### From source

```bash
cargo install --path .
```

### Build manually

```bash
cargo build --release
```

The binary will be at `target/release/dusk` (or `dusk.exe` on Windows).

## Usage

```bash
# Scan current directory
dusk

# Scan a specific path
dusk /home/user/projects
dusk C:\Users

# Show help
dusk --help
```

## Keybindings

| Key | Action |
|---|---|
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `Enter` / `→` / `l` | Open directory |
| `Backspace` / `←` / `h` | Go back |
| `PgUp` / `PgDn` | Page up / down |
| `g` / `G` | Go to top / bottom |
| `s` | Cycle sort mode (size → name → count) |
| `d` | Delete selected (with confirmation) |
| `?` | Toggle help overlay |
| `q` / `Esc` | Quit |

## License

MIT
