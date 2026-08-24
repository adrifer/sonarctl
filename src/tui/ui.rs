//! TUI rendering. No Sonar access happens here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::sonar::models::DeviceRole;
use crate::tui::app::{FocusPane, Mode, RouteTarget, TuiApp};

const HELP_TEXT: &str = "\
Global
  1 / 2 / 3    focus Output / Input / Devices
  Tab          focus next pane
  Shift+Tab    focus previous pane
  q            quit
  ?            toggle this help

Output routing
  j / Down     next route
  k / Up       previous route
  g / G        first / last route
  Enter        choose device
  h / l, [ / ] decrease / increase selected channel volume
  m            toggle selected channel mute
  r            refresh

Input routing
  Enter        choose device
  h / l, [ / ] decrease / increase microphone volume
  m            toggle microphone mute
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
    let navigation = Layout::vertical([
        Constraint::Length(7),
        Constraint::Length(3),
        Constraint::Min(3),
    ])
    .split(panels[0]);
    draw_output_routing(frame, app, navigation[0]);
    draw_input_routing(frame, app, navigation[1]);
    draw_devices(frame, app, navigation[2]);
    draw_mixer(frame, app, panels[1]);
    draw_footer(frame, app, areas[1]);

    match app.mode {
        Mode::Picker => draw_picker(frame, app, frame.area()),
        Mode::Help => draw_help(frame, frame.area()),
        Mode::Channels => {}
    }
}

fn draw_mixer(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let block = Block::bordered().title(" Channel details ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(state) = app.mixer_state() else {
        let message = if app.mixer_error().is_some() {
            "Mixer unavailable"
        } else {
            "Waiting for Sonar…"
        };
        frame.render_widget(Paragraph::new(message), inner);
        return;
    };
    let percent = format!("{:.0}%", state.percent());
    let label_width = 9;
    let bar_width = usize::from(inner.width)
        .saturating_sub(label_width + percent.chars().count() + 2)
        .min(24);
    let filled = ((state.volume * bar_width as f64).round() as usize).min(bar_width);
    let muted_style = if state.muted {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:<label_width$}", "Channel"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(app.mixer_channel.display_name()),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:<label_width$}", "Volume"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("█".repeat(filled), Style::default().fg(Color::Cyan)),
            Span::styled(
                "░".repeat(bar_width.saturating_sub(filled)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(format!("  {percent}")),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:<label_width$}", "Muted"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(if state.muted { "Yes" } else { "No" }, muted_style),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_output_routing(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let width = area.width.saturating_sub(4) as usize;
    let label_width = RouteTarget::OUTPUT
        .iter()
        .map(|target| target.label().len())
        .max()
        .unwrap_or(10)
        + 2;

    let items: Vec<ListItem> = RouteTarget::OUTPUT
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
    if app.focus == FocusPane::Output {
        state.select(Some(app.output_selected));
    }

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" [1] Output routing ")
                .border_style(if app.focus == FocusPane::Output {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                }),
        )
        .highlight_symbol("> ")
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_input_routing(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let target = RouteTarget::INPUT;
    let item = ListItem::new(Line::from(vec![
        Span::styled(
            format!("{:<14}", target.label()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.device_for(crate::sonar::models::Channel::Microphone)),
    ]));
    let mut state = ListState::default();
    if app.focus == FocusPane::Input {
        state.select(Some(0));
    }
    frame.render_stateful_widget(
        List::new([item])
            .block(Block::bordered().title(" [2] Input routing ").border_style(
                if app.focus == FocusPane::Input {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                },
            ))
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        area,
        &mut state,
    );
}

fn draw_devices(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let mut items = Vec::new();
    let mut selected_item = None;
    for (role, heading) in [
        (DeviceRole::Playback, "OUTPUT DEVICES"),
        (DeviceRole::Capture, "INPUT DEVICES"),
        (DeviceRole::Unknown, "OTHER DEVICES"),
    ] {
        let group: Vec<_> = app
            .devices()
            .iter()
            .enumerate()
            .filter(|(_, device)| device.role == role)
            .collect();
        if group.is_empty() {
            continue;
        }
        items.push(ListItem::new(Line::styled(
            format!("── {heading} ──"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for (device_index, device) in group {
            if app.focus == FocusPane::Devices && device_index == app.device_selected {
                selected_item = Some(items.len());
            }
            let marker = if app.device_is_visible(&device.id) {
                "[x]"
            } else {
                "[ ]"
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(&device.name),
            ])));
        }
    }
    let mut state = ListState::default();
    state.select(selected_item);
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::bordered().title(" [3] Devices ").border_style(
                if app.focus == FocusPane::Devices {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                },
            ))
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        area,
        &mut state,
    );
}

fn draw_footer(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let style = if app.status.is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let text = if app.status.text.is_empty() {
        let action = match app.focus {
            FocusPane::Output => "↑↓ select  Enter route  h/l volume  m mute",
            FocusPane::Input => "Enter route  h/l volume  m mute",
            FocusPane::Devices => "↑↓ select  Space/Enter toggle",
        };
        format!(" [1] Output  [2] Input  [3] Devices  │  {action}  │  ? help  q quit")
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
