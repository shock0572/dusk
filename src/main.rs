mod app;
mod report;
mod scanner;
mod ui;

use std::io;
use std::path::{absolute, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{App, AppAction};
use crate::scanner::{scan_directory, ScanProgress};

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
    #[arg(long, default_value = "1")]
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
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = run_scan_phase(&mut terminal, &scan_path);

        match result {
            Ok(Some(root)) => {
                let mut app = match select_name.take() {
                    Some(ref name) => App::new_with_selection(root, name, min_bytes),
                    None => App::new(root, min_bytes),
                };

                match run_app(&mut terminal, &mut app)? {
                    AppAction::Quit | AppAction::Continue => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        terminal.show_cursor()?;
                        return Ok(());
                    }
                    AppAction::Rescan { path, came_from } => {
                        scan_path = path;
                        select_name = Some(came_from);
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        terminal.show_cursor()?;
                    }
                }
            }
            Ok(None) => {
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                terminal.show_cursor()?;
                return Ok(());
            }
            Err(e) => {
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                terminal.show_cursor()?;
                return Err(e);
            }
        }
    }
}

fn run_report_mode(path: &PathBuf, min_bytes: u64) -> Result<()> {
    let progress = ScanProgress::new();
    eprintln!("Scanning {}...", path.display());
    let root = scan_directory(path, &progress);
    let report = report::generate_report(&root, min_bytes);
    print!("{report}");
    Ok(())
}

fn run_scan_phase(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &PathBuf,
) -> Result<Option<scanner::Entry>> {
    let progress = ScanProgress::new();
    let scan_path = path.clone();
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
                ratatui::text::Line::from(format!(
                    "    Items scanned: {files:>10}"
                )),
                ratatui::text::Line::from(format!(
                    "    Total size:    {size_str:>10}"
                )),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    "  Press 'q' or Esc to cancel",
                    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                )),
            ];

            let para = ratatui::widgets::Paragraph::new(text).block(block);
            f.render_widget(para, area);
        })?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q')
                    || key.code == KeyCode::Esc
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    progress.cancelled.store(true, Ordering::Relaxed);
                    let _ = scan_handle.join();
                    return Ok(None);
                }
            }
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

                if app.show_help {
                    app.show_help = false;
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(AppAction::Quit),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(AppAction::Quit)
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
                        match report::export_report(&app.root, app.min_bytes) {
                            Ok(path) => app.set_message(format!("Report saved: {}", path.display())),
                            Err(e) => app.set_message(format!("Report error: {e}")),
                        }
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
