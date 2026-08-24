//! TUI rendering. No Sonar access happens here.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::tui::app::{FocusPane, Mode, RouteTarget, TuiApp};

const HELP_TEXT: &str = "\
Global
  Tab          focus Routing / Devices
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
    let areas = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(frame.area());
    let panels = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(areas[0]);
    draw_routing(frame, app, panels[0]);
    draw_devices(frame, app, panels[1]);
    draw_status(frame, app, areas[1]);

    match app.mode {
        Mode::Picker => draw_picker(frame, app, frame.area()),
        Mode::Help => draw_help(frame, frame.area()),
        Mode::Channels => {}
    }
}

fn draw_routing(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let width = area.width.saturating_sub(4) as usize;
    let label_width = RouteTarget::ALL
        .iter()
        .map(|target| target.label().len())
        .max()
        .unwrap_or(10)
        + 2;

    let mut items = vec![ListItem::new(Line::styled(
        "Output",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))];
    items.extend(RouteTarget::ALL[..5].iter().map(|target| {
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
    }));
    items.push(ListItem::new(Line::styled(
        "Input",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    let microphone = RouteTarget::ALL[5];
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            format!("{:<label_width$}", microphone.label()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(truncate(
            &app.device_for(crate::sonar::models::Channel::Microphone),
            width.saturating_sub(label_width),
        )),
    ])));

    let mut state = ListState::default();
    if app.focus == FocusPane::Routing {
        state.select(Some(if app.selected < 5 {
            app.selected + 1
        } else {
            7
        }));
    }

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Routing ")
                .title_bottom(Line::from(
                    " Tab focus   ↑↓ select   Enter change   ? help   q quit ",
                ))
                .border_style(if app.focus == FocusPane::Routing {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                })
                .title_alignment(Alignment::Left),
        )
        .highlight_symbol("> ")
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));

    frame.render_stateful_widget(list, area, &mut state);
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
    if !items.is_empty() && app.focus == FocusPane::Devices {
        state.select(Some(app.device_selected.min(items.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(" Picker visibility ")
                    .title_bottom(Line::from(" Tab focus   Space/Enter toggle   r refresh "))
                    .border_style(if app.focus == FocusPane::Devices {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    }),
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
