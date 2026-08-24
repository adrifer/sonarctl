//! TUI rendering. No Sonar access happens here.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};

use crate::sonar::models::Channel;
use crate::tui::app::{Mode, RouteTarget, TuiApp, TuiTab};

const HELP_TEXT: &str = "\
Global
  Tab          switch Routing / Devices
  q            quit
  ?            toggle this help

Routing
  j / Down     next route
  k / Up       previous route
  g / G        first / last route
  Enter        choose device
  r            refresh

Devices
  j / Down     next device
  k / Up       previous device
  Space/Enter  show or hide in pickers

Device picker
  j / Down     next device
  k / Up       previous device
  Enter        apply
  /            filter
  Esc          cancel";

/// Draw the whole interface.
pub fn draw(frame: &mut Frame, app: &TuiApp) {
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(frame.area());
    draw_tabs(frame, app, areas[0]);
    match app.tab {
        TuiTab::Routing => draw_routing(frame, app, areas[1]),
        TuiTab::Devices => draw_devices(frame, app, areas[1]),
    }
    draw_status(frame, app, areas[2]);

    match app.mode {
        Mode::Picker => draw_picker(frame, app, frame.area()),
        Mode::Help => draw_help(frame, frame.area()),
        Mode::Channels => {}
    }
}

fn draw_tabs(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let selected = match app.tab {
        TuiTab::Routing => 0,
        TuiTab::Devices => 1,
    };
    frame.render_widget(
        Tabs::new([" Routing ", " Devices "])
            .select(selected)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("│"),
        area,
    );
}

fn draw_routing(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let sections = Layout::vertical([Constraint::Min(7), Constraint::Length(3)]).split(area);
    draw_outputs(frame, app, sections[0]);
    draw_input(frame, app, sections[1]);
}

fn draw_outputs(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let width = area.width.saturating_sub(4) as usize;
    let targets = &RouteTarget::ALL[..5];
    let label_width = targets
        .iter()
        .map(|target| target.label().len())
        .max()
        .unwrap_or(10)
        + 2;

    let items: Vec<ListItem> = targets
        .iter()
        .map(|target| {
            let device = match target {
                RouteTarget::AllOutputs => app.all_outputs_device(),
                RouteTarget::Channel(channel) => app.device_for(*channel),
            };
            let name = format!("{:<label_width$}", target.label());
            let line = Line::from(vec![
                Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(truncate(&device, width.saturating_sub(label_width))),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    if app.selected < targets.len() {
        state.select(Some(app.selected));
    }

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Output ")
                .title_bottom(Line::from(
                    " ↑↓ select   Enter change   Tab devices   ? help   q quit ",
                ))
                .title_alignment(Alignment::Left),
        )
        .highlight_symbol("> ")
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_input(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let item = ListItem::new(Line::from(vec![
        Span::styled(
            format!("{:<14}", Channel::Microphone.display_name()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.device_for(Channel::Microphone)),
    ]));
    let mut state = ListState::default();
    if app.selected == 5 {
        state.select(Some(0));
    }
    frame.render_stateful_widget(
        List::new([item])
            .block(Block::bordered().title(" Input "))
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        area,
        &mut state,
    );
}

fn draw_devices(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let items: Vec<ListItem> = app
        .devices()
        .iter()
        .map(|device| {
            let marker = if app.device_is_visible(&device.id) {
                "[x]"
            } else {
                "[ ]"
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} {:<9}", device.role.label()),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(&device.name),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.device_selected.min(items.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(" Picker visibility ")
                    .title_bottom(Line::from(
                        " Space/Enter toggle   Tab routing   r refresh   ? help   q quit ",
                    )),
            )
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        area,
        &mut state,
    );
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

    let title = format!(" Select device for {} ", picker.target.label());
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
