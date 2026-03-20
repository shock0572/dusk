use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;
use crate::scanner::Entry;

const BAR_WIDTH: usize = 20;

fn format_size(bytes: u64) -> String {
    humansize::format_size(bytes, humansize::BINARY)
}

fn size_bar(ratio: f64, width: usize) -> String {
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "#".repeat(filled), " ".repeat(empty))
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_file_list(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    if app.show_help {
        draw_help_popup(f);
    }

    if app.confirm_delete.is_some() {
        draw_delete_confirm(f, app);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let current = app.current_entry();
    let title = format!(
        " DUSK — {} ({}) ",
        app.current_path.display(),
        format_size(current.size)
    );

    let items_info = format!(
        "{} items, sorted by {}",
        current.children.len(),
        app.sort_by.label()
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(Span::styled(
            items_info,
            Style::default().fg(Color::DarkGray),
        )));

    f.render_widget(block, area);
}

fn draw_file_list(f: &mut Frame, app: &mut App, area: Rect) {
    let visible_height = area.height as usize;

    if app.selected < app.scroll_offset {
        app.scroll_offset = app.selected;
    } else if app.selected >= app.scroll_offset + visible_height {
        app.scroll_offset = app.selected - visible_height + 1;
    }

    let has_parent = app.has_parent();
    let offset = if has_parent { 1 } else { 0 };
    let children = app.sorted_children();

    if children.is_empty() && !has_parent {
        let empty = Paragraph::new("  (empty directory)")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty, area);
        return;
    }

    let parent_size = app.current_entry().size.max(1) as f64;
    let total_count = children.len() + offset;

    let mut rows: Vec<Row> = Vec::with_capacity(visible_height);
    for i in app.scroll_offset..total_count.min(app.scroll_offset + visible_height) {
        if has_parent && i == 0 {
            rows.push(dotdot_row(i == app.selected));
        } else {
            let child_idx = i - offset;
            if let Some(entry) = children.get(child_idx) {
                rows.push(entry_to_row(entry, i == app.selected, parent_size));
            }
        }
    }

    let widths = [
        Constraint::Length(10),
        Constraint::Length(7),
        Constraint::Length(BAR_WIDTH as u16 + 2),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths).column_spacing(1);

    f.render_widget(table, area);
}

fn dotdot_row(selected: bool) -> Row<'static> {
    let style = if selected {
        Style::default()
            .bg(Color::Rgb(40, 40, 60))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Row::new(vec![
        Span::styled("         ", style),
        Span::styled("      ", style),
        Span::styled(format!("[{}]", " ".repeat(BAR_WIDTH)), style),
        Span::styled("/..", style),
    ])
}

fn entry_to_row<'a>(entry: &Entry, selected: bool, parent_size: f64) -> Row<'a> {
    let ratio = entry.size as f64 / parent_size;
    let pct = ratio * 100.0;
    let bar = size_bar(ratio, BAR_WIDTH);

    let icon = if entry.is_dir {
        if entry.error {
            "!"
        } else {
            "/"
        }
    } else {
        " "
    };

    let name_display = format!("{}{}", entry.name, icon);

    let style = if selected {
        Style::default()
            .bg(Color::Rgb(40, 40, 60))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else if entry.error {
        Style::default().fg(Color::Red)
    } else if entry.is_dir {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };

    let bar_color = if ratio > 0.5 {
        Color::Red
    } else if ratio > 0.25 {
        Color::Yellow
    } else {
        Color::Green
    };

    Row::new(vec![
        Span::styled(format!("{:>9}", format_size(entry.size)), style),
        Span::styled(format!("{:>5.1}%", pct), style),
        Span::styled(bar, Style::default().fg(bar_color)),
        Span::styled(name_display, style),
    ])
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let text = if let Some((msg, _)) = &app.message {
        Line::from(Span::styled(
            msg.as_str(),
            Style::default().fg(Color::Yellow),
        ))
    } else {
        Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(Color::Cyan)),
            Span::raw(" navigate  "),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::raw(" open  "),
            Span::styled("Backspace", Style::default().fg(Color::Cyan)),
            Span::raw(" back  "),
            Span::styled("s", Style::default().fg(Color::Cyan)),
            Span::raw(" sort  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" delete  "),
            Span::styled("?", Style::default().fg(Color::Cyan)),
            Span::raw(" help  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ])
    };

    f.render_widget(Paragraph::new(text), area);
}

fn draw_help_popup(f: &mut Frame) {
    let area = centered_rect(50, 60, f.area());
    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  DUSK - Disk Usage Survey Kit",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  Navigation:"),
        Line::from("    ↑/k         Move up"),
        Line::from("    ↓/j         Move down"),
        Line::from("    Enter/→/l   Open directory"),
        Line::from("    Backspace/←/h  Go back"),
        Line::from("    PgUp        Page up"),
        Line::from("    PgDn        Page down"),
        Line::from("    g           Go to top"),
        Line::from("    G           Go to bottom"),
        Line::from(""),
        Line::from("  Actions:"),
        Line::from("    s           Cycle sort mode (size/name/count)"),
        Line::from("    d           Delete selected file/directory"),
        Line::from("    ?           Toggle this help"),
        Line::from("    q/Esc       Quit"),
        Line::from(""),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let para = Paragraph::new(help_text).block(block);
    f.render_widget(para, area);
}

fn draw_delete_confirm(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);

    let path_str = app
        .confirm_delete
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Really delete?",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("  {path_str}")),
        Line::from(""),
        Line::from("  Press 'y' to confirm, any other key to cancel"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(Span::styled(
            " Confirm Delete ",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ));

    let para = Paragraph::new(text).block(block);
    f.render_widget(para, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
