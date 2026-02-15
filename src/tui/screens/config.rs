// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use crate::app::{AppCommand, ConfigItem, FileBrowserMode};
use crate::config::Settings;
use crate::token_bucket::TokenBucket;
use crate::tui::formatters::{format_limit_bps, path_to_string};
use crate::tui::screen_context::ScreenContext;
use directories::UserDirs;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::prelude::{Frame, Line, Span, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEventKind};
use tokio::sync::mpsc;

pub enum ConfigOutcome {
    Stay,
    ToNormal,
}

pub fn draw(
    f: &mut Frame,
    screen: &ScreenContext<'_>,
    settings: &Settings,
    selected_index: usize,
    items: &[ConfigItem],
    editing: &Option<(ConfigItem, String)>,
) {
    let ctx = screen.theme;

    let area = crate::tui::formatters::centered_rect(80, 60, f.area());
    f.render_widget(Clear, f.area());
    let block = Block::default()
        .title(Span::styled(
            "Config",
            ctx.apply(Style::default().fg(ctx.state_selected())),
        ))
        .borders(Borders::ALL)
        .border_style(ctx.apply(Style::default().fg(ctx.theme.semantic.border)));
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner_area);
    let settings_area = chunks[0];
    let footer_area = chunks[1];
    let rows_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            items
                .iter()
                .map(|_| Constraint::Length(1))
                .collect::<Vec<_>>(),
        )
        .split(settings_area);

    for (i, item) in items.iter().enumerate() {
        let (name_str, value_str) = match item {
            ConfigItem::ClientPort => ("Listen Port", settings.client_port.to_string()),
            ConfigItem::DefaultDownloadFolder => (
                "Default Download Folder",
                path_to_string(settings.default_download_folder.as_deref()),
            ),
            ConfigItem::WatchFolder => (
                "Torrent Watch Folder",
                path_to_string(settings.watch_folder.as_deref()),
            ),
            ConfigItem::GlobalDownloadLimit => (
                "Global DL Limit",
                format_limit_bps(settings.global_download_limit_bps),
            ),
            ConfigItem::GlobalUploadLimit => (
                "Global UL Limit",
                format_limit_bps(settings.global_upload_limit_bps),
            ),
        };

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(rows_layout[i]);
        let is_highlighted = if let Some((edited_item, _)) = editing {
            *edited_item == *item
        } else {
            i == selected_index
        };
        let row_style = if is_highlighted {
            ctx.apply(Style::default().fg(ctx.state_warning()))
        } else {
            ctx.apply(Style::default().fg(ctx.theme.semantic.text))
        };
        let name_with_selector = if is_highlighted {
            format!("▶ {}", name_str)
        } else {
            format!("  {}", name_str)
        };

        let name_p = Paragraph::new(name_with_selector).style(row_style);
        f.render_widget(name_p, columns[0]);

        if let Some((_edited_item, buffer)) = editing {
            if is_highlighted {
                let edit_p = Paragraph::new(buffer.as_str()).style(row_style.fg(ctx.state_warning()));
                f.set_cursor_position((columns[1].x + buffer.len() as u16, columns[1].y));
                f.render_widget(edit_p, columns[1]);
            } else {
                let value_p = Paragraph::new(value_str).style(row_style);
                f.render_widget(value_p, columns[1]);
            }
        } else {
            let value_p = Paragraph::new(value_str).style(row_style);
            f.render_widget(value_p, columns[1]);
        }
    }

    let help_text = if editing.is_some() {
        Line::from(vec![
            Span::styled(
                "[Enter]",
                ctx.apply(Style::default().fg(ctx.state_success())),
            ),
            Span::raw(" to confirm, "),
            Span::styled("[Esc]", ctx.apply(Style::default().fg(ctx.state_error()))),
            Span::raw(" to cancel."),
        ])
    } else {
        Line::from(vec![
            Span::raw("Use "),
            Span::styled(
                "↑/↓/k/j",
                ctx.apply(Style::default().fg(ctx.state_warning())),
            ),
            Span::raw(" to navigate. "),
            Span::styled(
                "[Enter]",
                ctx.apply(Style::default().fg(ctx.state_warning())),
            ),
            Span::raw(" to edit. "),
            Span::styled("[r]", ctx.apply(Style::default().fg(ctx.state_warning()))),
            Span::raw("eset to default. "),
            Span::styled(
                "[Esc]|[Q]",
                ctx.apply(Style::default().fg(ctx.state_success())),
            ),
            Span::raw(" to Save & Exit, "),
        ])
    };

    let footer_paragraph = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)));
    f.render_widget(footer_paragraph, footer_area);
}

pub fn handle_event(
    event: CrosstermEvent,
    settings_edit: &mut Box<Settings>,
    selected_index: &mut usize,
    items: &mut [ConfigItem],
    editing: &mut Option<(ConfigItem, String)>,
    app_command_tx: &mpsc::Sender<AppCommand>,
    global_dl_bucket: &Arc<TokenBucket>,
    global_ul_bucket: &Arc<TokenBucket>,
) -> ConfigOutcome {
    if let Some((item, buffer)) = editing {
        if let CrosstermEvent::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char(c) => {
                        if c.is_ascii_digit() {
                            buffer.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                    }
                    KeyCode::Esc => *editing = None,
                    KeyCode::Enter => {
                        match item {
                            ConfigItem::ClientPort => {
                                if let Ok(new_port) = buffer.parse::<u16>() {
                                    if new_port > 0 {
                                        settings_edit.client_port = new_port;
                                    }
                                }
                            }
                            ConfigItem::GlobalDownloadLimit => {
                                if let Ok(new_rate) = buffer.parse::<u64>() {
                                    settings_edit.global_download_limit_bps = new_rate;
                                    let bucket = global_dl_bucket.clone();
                                    tokio::spawn(async move {
                                        bucket.set_rate(new_rate as f64);
                                    });
                                }
                            }
                            ConfigItem::GlobalUploadLimit => {
                                if let Ok(new_rate) = buffer.parse::<u64>() {
                                    settings_edit.global_upload_limit_bps = new_rate;
                                    let bucket = global_ul_bucket.clone();
                                    tokio::spawn(async move {
                                        bucket.set_rate(new_rate as f64);
                                    });
                                }
                            }
                            _ => {}
                        }
                        *editing = None;
                    }
                    _ => {}
                }
            }
        }
        return ConfigOutcome::Stay;
    }

    if let CrosstermEvent::Key(key) = event {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Esc | KeyCode::Char('Q') => {
                    let _ = app_command_tx.try_send(AppCommand::UpdateConfig(*settings_edit.clone()));
                    return ConfigOutcome::ToNormal;
                }
                KeyCode::Enter => {
                    let selected_item = items[*selected_index];
                    match selected_item {
                        ConfigItem::GlobalDownloadLimit
                        | ConfigItem::GlobalUploadLimit
                        | ConfigItem::ClientPort => {
                            *editing = Some((selected_item, String::new()));
                        }
                        ConfigItem::DefaultDownloadFolder | ConfigItem::WatchFolder => {
                            let initial_path = if selected_item == ConfigItem::WatchFolder {
                                settings_edit.watch_folder.clone()
                            } else {
                                settings_edit.default_download_folder.clone()
                            }
                            .unwrap_or_else(|| {
                                UserDirs::new()
                                    .and_then(|ud| ud.download_dir().map(|p| p.to_path_buf()))
                                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                            });

                            let _ = app_command_tx.try_send(AppCommand::FetchFileTree {
                                path: initial_path,
                                browser_mode: FileBrowserMode::ConfigPathSelection {
                                    target_item: selected_item,
                                    current_settings: settings_edit.clone(),
                                    selected_index: *selected_index,
                                    items: items.to_vec(),
                                },
                                highlight_path: None,
                            });
                        }
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    *selected_index = selected_index.saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if *selected_index < items.len() - 1 {
                        *selected_index += 1;
                    }
                }
                KeyCode::Char('r') => {
                    let default_settings = Settings::default();
                    let selected_item = items[*selected_index];
                    match selected_item {
                        ConfigItem::ClientPort => {
                            settings_edit.client_port = default_settings.client_port;
                        }
                        ConfigItem::DefaultDownloadFolder => {
                            settings_edit.default_download_folder =
                                default_settings.default_download_folder;
                        }
                        ConfigItem::WatchFolder => {
                            settings_edit.watch_folder = default_settings.watch_folder;
                        }
                        ConfigItem::GlobalDownloadLimit => {
                            settings_edit.global_download_limit_bps =
                                default_settings.global_download_limit_bps;
                        }
                        ConfigItem::GlobalUploadLimit => {
                            settings_edit.global_upload_limit_bps =
                                default_settings.global_upload_limit_bps;
                        }
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    let item = items[*selected_index];
                    let increment = 10_000 * 8;
                    match item {
                        ConfigItem::GlobalDownloadLimit => {
                            let new_rate =
                                settings_edit.global_download_limit_bps.saturating_add(increment);
                            settings_edit.global_download_limit_bps = new_rate;
                            let bucket = global_dl_bucket.clone();
                            tokio::spawn(async move {
                                bucket.set_rate(new_rate as f64);
                            });
                        }
                        ConfigItem::GlobalUploadLimit => {
                            let new_rate =
                                settings_edit.global_upload_limit_bps.saturating_add(increment);
                            settings_edit.global_upload_limit_bps = new_rate;
                            let bucket = global_ul_bucket.clone();
                            tokio::spawn(async move {
                                bucket.set_rate(new_rate as f64);
                            });
                        }
                        _ => {}
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    let item = items[*selected_index];
                    let decrement = 10_000 * 8;
                    match item {
                        ConfigItem::ClientPort => {}
                        ConfigItem::GlobalDownloadLimit => {
                            let new_rate =
                                settings_edit.global_download_limit_bps.saturating_sub(decrement);
                            settings_edit.global_download_limit_bps = new_rate;
                            let bucket = global_dl_bucket.clone();
                            tokio::spawn(async move {
                                bucket.set_rate(new_rate as f64);
                            });
                        }
                        ConfigItem::GlobalUploadLimit => {
                            let new_rate =
                                settings_edit.global_upload_limit_bps.saturating_sub(decrement);
                            settings_edit.global_upload_limit_bps = new_rate;
                            let bucket = global_ul_bucket.clone();
                            tokio::spawn(async move {
                                bucket.set_rate(new_rate as f64);
                            });
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    ConfigOutcome::Stay
}
