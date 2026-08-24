//! TUI rendering. No Sonar access happens here.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::sonar::models::Channel;
use crate::tui::app::{Mode, TuiApp};

const HELP_TEXT: &str = "\
Channels
  j / Down     next channel
  k / Up       previous channel
  g / G        first / last channel
  Enter        choose device
  r            refresh
  ?            toggle this help
  q            quit

Device picker
  j / Down     next device
  k / Up       previous device
  Enter        apply
  /            filter
  Esc          cancel";

/// Draw the whole interface.
pub fn draw(frame: &mut Frame, app: &TuiApp) {
    let areas = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(frame.area());
    draw_channels(frame, app, areas[0]);
    draw_status(frame, app, areas[1]);

    match app.mode {
        Mode::Picker => draw_picker(frame, app, frame.area()),
        Mode::Help => draw_help(frame, frame.area()),
        Mode::Channels => {}
    }
}

fn draw_channels(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let width = area.width.saturating_sub(4) as usize;
    let channel_width = Channel::ALL
        .iter()
        .map(|channel| channel.display_name().len())
        .max()
        .unwrap_or(10)
        + 2;

    let items: Vec<ListItem> = Channel::ALL
        .iter()
        .map(|channel| {
            let device = app.device_for(*channel);
            let name = format!("{:<channel_width$}", channel.display_name());
            let line = Line::from(vec![
                Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(truncate(&device, width.saturating_sub(channel_width))),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.selected));

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" sonarctl ")
                .title_bottom(Line::from(
                    " ↑↓ select   Enter change   r refresh   ? help   q quit ",
                ))
                .title_alignment(Alignment::Left),
        )
        .highlight_symbol("> ")
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_status(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let style = if app.status.is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let text = if app.status.text.is_empty() {
        String::new()
    } else {
        format!(" {}", app.status.text)
    };
    frame.render_widget(Paragraph::new(Line::styled(text, style)), area);
}

fn draw_picker(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let Some(picker) = app.picker.as_ref() else {
        return;
    };

    let popup = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup);

    let items: Vec<ListItem> = picker
        .visible()
        .iter()
        .map(|device| {
            let current = picker
                .current_id
                .as_deref()
                .is_some_and(|id| id == device.id);
            let marker = if current { " ●" } else { "" };
            ListItem::new(Line::from(format!("{}{marker}", device.name)))
        })
        .collect();

    let title = format!(" Select device for {} ", picker.channel.display_name());
    let footer = if picker.filtering {
        format!(" filter: {}_ ", picker.filter)
    } else if picker.filter.is_empty() {
        " ↑↓ select   Enter apply   / filter   Esc cancel ".to_string()
    } else {
        format!(
            " filter: {}   ↑↓ select   Enter apply   Esc cancel ",
            picker.filter
        )
    };

    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(picker.selected.min(items.len() - 1)));
    }

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title)
                .title_bottom(Line::from(footer)),
        )
        .highlight_symbol("> ")
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));

    frame.render_stateful_widget(list, popup, &mut state);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(HELP_TEXT).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(" Help ")
                .title_bottom(Line::from(" any key closes ")),
        ),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 || text.chars().count() <= width {
        return text.to_string();
    }
    let mut result: String = text.chars().take(width.saturating_sub(1)).collect();
    result.push('…');
    result
}
