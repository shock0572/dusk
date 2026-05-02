mod app;
mod report;
mod scanner;
mod ui;

use std::io;
use std::path::{Path, PathBuf, absolute};
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::{App, AppAction};
use crate::scanner::{ScanProgress, scan_directory};

#[derive(Parser)]
#[command(
    name = "dusk",
    about = "DUSK - Disk Usage Survey Kit\nAn interactive disk usage analyzer for your terminal.",
    version
)]
struct Cli {
    /// Directory to scan (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Print a size report to stdout instead of launching the TUI
    #[arg(long, short)]
    report: bool,

    /// Minimum size to include in reports (in GiB, default: 1)
    #[arg(long, default_value = "1", value_parser = parse_min_gib)]
    min_gib: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut scan_path = absolute(&cli.path)
        .with_context(|| format!("Cannot resolve path: {}", cli.path.display()))?
        .to_path_buf();

    if !scan_path.is_dir() {
        anyhow::bail!("{} is not a directory", scan_path.display());
    }

    let min_bytes = (cli.min_gib * 1_073_741_824.0) as u64;

    if cli.report {
        return run_report_mode(&scan_path, min_bytes);
    }

    let mut select_name: Option<String> = None;

    loop {
        let mut session = TerminalSession::new()?;

        let result = run_scan_phase(&mut session.terminal, &scan_path);

        match result {
            Ok(Some(root)) => {
                let mut app = match select_name.take() {
                    Some(ref name) => App::new_with_selection(root, name, min_bytes),
                    None => App::new(root, min_bytes),
                };

                match run_app(&mut session.terminal, &mut app)? {
                    AppAction::Quit | AppAction::Continue => {
                        return Ok(());
                    }
                    AppAction::Rescan { path, came_from } => {
                        scan_path = path;
                        select_name = Some(came_from);
                    }
                }
            }
            Ok(None) => {
                return Ok(());
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn new() -> Result<Self> {
        enable_raw_mode()?;

        let mut stdout = io::stdout();
        if let Err(err) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }

        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(err) => {
                let _ = disable_raw_mode();
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                Err(err.into())
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn parse_min_gib(value: &str) -> Result<f64, String> {
    let parsed: f64 = value
        .parse()
        .map_err(|_| "minimum size must be a number".to_string())?;

    if !parsed.is_finite() || parsed < 0.0 {
        return Err("minimum size must be a finite, non-negative number".to_string());
    }

    let max_gib = u64::MAX as f64 / 1_073_741_824.0;
    if parsed > max_gib {
        return Err(format!("minimum size must be at most {max_gib:.3} GiB"));
    }

    Ok(parsed)
}

fn run_report_mode(path: &Path, min_bytes: u64) -> Result<()> {
    let progress = ScanProgress::new();
    eprintln!("Scanning {}...", path.display());
    let root = scan_directory(path, &progress);
    if root.error {
        return Err(anyhow!("Cannot read directory: {}", path.display()));
    }
    let report = report::generate_report(&root, min_bytes);
    print!("{report}");
    Ok(())
}

fn run_scan_phase(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &Path,
) -> Result<Option<scanner::Entry>> {
    let progress = ScanProgress::new();
    let scan_path = path.to_path_buf();
    let files_counter = progress.files_scanned.clone();
    let bytes_counter = progress.bytes_scanned.clone();
    let cancelled = progress.cancelled.clone();

    let scan_handle = std::thread::spawn(move || {
        let progress = ScanProgress {
            files_scanned: files_counter.clone(),
            bytes_scanned: bytes_counter.clone(),
            cancelled: cancelled.clone(),
        };
        scan_directory(&scan_path, &progress)
    });

    let files_display = progress.files_scanned.clone();
    let bytes_display = progress.bytes_scanned.clone();

    loop {
        let files = files_display.load(Ordering::Relaxed);
        let bytes = bytes_display.load(Ordering::Relaxed);
        let size_str = humansize::format_size(bytes, humansize::BINARY);

        terminal.draw(|f| {
            let area = f.area();
            let block = ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan))
                .title(ratatui::text::Span::styled(
                    " DUSK - Scanning ",
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::White)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));

            let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let idx = (files as usize / 50) % spinner_chars.len();
            let spinner = spinner_chars[idx];

            let text = vec![
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    format!("  {spinner} Scanning: {}", path.display()),
                    ratatui::style::Style::default().fg(ratatui::style::Color::Cyan),
                )),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(format!("    Items scanned: {files:>10}")),
                ratatui::text::Line::from(format!("    Total size:    {size_str:>10}")),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    "  Press 'q' or Esc to cancel",
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                )),
            ];

            let para = ratatui::widgets::Paragraph::new(text).block(block);
            f.render_widget(para, area);
        })?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && (key.code == KeyCode::Char('q')
                || key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            progress.cancelled.store(true, Ordering::Relaxed);
            let _ = scan_handle.join();
            return Ok(None);
        }

        if scan_handle.is_finished() {
            match scan_handle.join() {
                Ok(root) => return Ok(Some(root)),
                Err(_) => anyhow::bail!("Scan thread panicked"),
            }
        }
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<AppAction> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        app.tick_message();

        if event::poll(Duration::from_millis(50))? {
            let mut last_key = None;
            while event::poll(Duration::from_millis(0))? {
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        last_key = Some(k);
                    }
                } else {
                    break;
                }
            }
            if let Some(key) = last_key {
                if app.confirm_delete.is_some() {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => app.confirm_delete_yes(),
                        _ => app.confirm_delete_no(),
                    }
                    continue;
                }

                if app.show_report {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('r') | KeyCode::Char('q') => {
                            app.close_report();
                        }
                        KeyCode::Down | KeyCode::Char('j') => app.report_scroll += 1,
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.report_scroll = app.report_scroll.saturating_sub(1);
                        }
                        KeyCode::PageDown => app.report_scroll += 20,
                        KeyCode::PageUp => {
                            app.report_scroll = app.report_scroll.saturating_sub(20);
                        }
                        KeyCode::Char('w') => {
                            match report::export_report(app.current_entry(), app.min_bytes) {
                                Ok(path) => {
                                    app.close_report();
                                    app.set_message(format!("Report saved: {}", path.display()));
                                }
                                Err(e) => {
                                    app.close_report();
                                    app.set_message(format!("Report error: {e}"));
                                }
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.show_help {
                    app.show_help = false;
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(AppAction::Quit),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(AppAction::Quit);
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                        let action = app.enter_selected();
                        if matches!(action, AppAction::Rescan { .. }) {
                            return Ok(action);
                        }
                    }
                    KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                        let action = app.go_up();
                        if matches!(action, AppAction::Rescan { .. }) {
                            return Ok(action);
                        }
                    }
                    KeyCode::PageDown => app.page_down(20),
                    KeyCode::PageUp => app.page_up(20),
                    KeyCode::Char('g') => {
                        app.selected = 0;
                        app.scroll_offset = 0;
                    }
                    KeyCode::Char('G') => {
                        let count = app.display_count();
                        if count > 0 {
                            app.selected = count - 1;
                        }
                    }
                    KeyCode::Char('s') => {
                        app.sort_by = app.sort_by.next();
                    }
                    KeyCode::Char('d') => {
                        app.request_delete();
                    }
                    KeyCode::Char('r') => {
                        app.open_report();
                    }
                    KeyCode::Char('?') => {
                        app.toggle_help();
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_min_gib;

    #[test]
    fn min_gib_parser_rejects_invalid_values() {
        assert!(parse_min_gib("-1").is_err());
        assert!(parse_min_gib("NaN").is_err());
        assert!(parse_min_gib("inf").is_err());
    }

    #[test]
    fn min_gib_parser_accepts_non_negative_finite_values() {
        assert_eq!(parse_min_gib("0").unwrap(), 0.0);
        assert_eq!(parse_min_gib("1.5").unwrap(), 1.5);
    }
}
