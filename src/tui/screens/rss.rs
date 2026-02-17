// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::{AppCommand, AppMode, AppState, RssScreen, RssSectionFocus};
use crate::tui::formatters::centered_rect;
use crate::tui::screen_context::ScreenContext;
use chrono::{DateTime, Local, Utc};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEventKind};
use ratatui::{prelude::*, widgets::*};
use reqwest::Url;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub enum RssAction {
    ToNormal,
    ToggleHistory,
    FocusNext,
    MoveUp,
    MoveDown,
    TriggerSync,
    InsertChar(char),
    Backspace,
    CommitInput,
    CancelInput,
    AddEntry,
    DeleteEntry,
    ToggleFeedEnabled,
    StartSearch,
}

#[derive(Default)]
pub struct RssReduceResult {
    pub effects: Vec<RssAction>,
}

fn map_key_to_rss_action(
    key_code: KeyCode,
    key_kind: KeyEventKind,
    app_state: &AppState,
) -> Option<RssAction> {
    if key_kind != KeyEventKind::Press {
        return None;
    }

    if app_state.ui.rss.is_editing || app_state.ui.rss.is_searching {
        return match key_code {
            KeyCode::Esc => Some(RssAction::CancelInput),
            KeyCode::Enter => Some(RssAction::CommitInput),
            KeyCode::Backspace => Some(RssAction::Backspace),
            KeyCode::Char(c) => Some(RssAction::InsertChar(c)),
            _ => None,
        };
    }

    match key_code {
        KeyCode::Esc | KeyCode::Char('q') => Some(RssAction::ToNormal),
        KeyCode::Char('h') => Some(RssAction::ToggleHistory),
        KeyCode::Tab => Some(RssAction::FocusNext),
        KeyCode::Char('s') => Some(RssAction::TriggerSync),
        KeyCode::Char('a') => Some(RssAction::AddEntry),
        KeyCode::Char('d') => Some(RssAction::DeleteEntry),
        KeyCode::Char(' ') => Some(RssAction::ToggleFeedEnabled),
        KeyCode::Char('/') => Some(RssAction::StartSearch),
        KeyCode::Up | KeyCode::Char('k') => Some(RssAction::MoveUp),
        KeyCode::Down | KeyCode::Char('j') => Some(RssAction::MoveDown),
        _ => None,
    }
}

fn reduce_rss_action(action: RssAction) -> RssReduceResult {
    RssReduceResult {
        effects: vec![action],
    }
}

fn next_focus(current: RssSectionFocus) -> RssSectionFocus {
    match current {
        RssSectionFocus::Links => RssSectionFocus::Filters,
        RssSectionFocus::Filters => RssSectionFocus::Explorer,
        RssSectionFocus::Explorer => RssSectionFocus::Links,
    }
}

fn selected_index_mut(app_state: &mut AppState) -> &mut usize {
    if matches!(app_state.ui.rss.active_screen, RssScreen::History) {
        return &mut app_state.ui.rss.selected_history_index;
    }

    match app_state.ui.rss.focused_section {
        RssSectionFocus::Links => &mut app_state.ui.rss.selected_feed_index,
        RssSectionFocus::Filters => &mut app_state.ui.rss.selected_filter_index,
        RssSectionFocus::Explorer => &mut app_state.ui.rss.selected_explorer_index,
    }
}

fn current_list_len(app_state: &AppState, settings: &crate::config::Settings) -> usize {
    if matches!(app_state.ui.rss.active_screen, RssScreen::History) {
        return filtered_history_entries(
            &app_state.rss_runtime.history,
            &app_state.ui.rss.search_query,
        )
        .len();
    }

    match app_state.ui.rss.focused_section {
        RssSectionFocus::Links => settings.rss.feeds.len(),
        RssSectionFocus::Filters => settings.rss.filters.len(),
        RssSectionFocus::Explorer => app_state.rss_runtime.preview_items.len(),
    }
}

fn sorted_feed_indices(settings: &crate::config::Settings) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..settings.rss.feeds.len()).collect();
    indices.sort_by(|a, b| {
        settings.rss.feeds[*a]
            .url
            .to_lowercase()
            .cmp(&settings.rss.feeds[*b].url.to_lowercase())
    });
    indices
}

fn selected_feed_actual_idx(
    settings: &crate::config::Settings,
    selected_display_idx: usize,
) -> Option<usize> {
    sorted_feed_indices(settings)
        .get(selected_display_idx)
        .copied()
}

fn sorted_filter_indices(settings: &crate::config::Settings) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..settings.rss.filters.len()).collect();
    indices.sort_by(|a, b| {
        settings.rss.filters[*a]
            .query
            .to_lowercase()
            .cmp(&settings.rss.filters[*b].query.to_lowercase())
    });
    indices
}

fn selected_filter_actual_idx(
    settings: &crate::config::Settings,
    selected_display_idx: usize,
) -> Option<usize> {
    sorted_filter_indices(settings)
        .get(selected_display_idx)
        .copied()
}

fn enabled_filter_queries(settings: &crate::config::Settings) -> Vec<String> {
    settings
        .rss
        .filters
        .iter()
        .filter(|f| f.enabled)
        .map(|f| f.query.trim().to_lowercase())
        .filter(|q| !q.is_empty())
        .collect()
}

fn explorer_should_be_greyed_out(settings: &crate::config::Settings) -> bool {
    settings.rss.filters.iter().all(|f| !f.enabled)
}

fn is_valid_feed_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .is_some_and(|u| matches!(u.scheme(), "http" | "https"))
}

fn execute_rss_effects(
    app_state: &mut AppState,
    settings: &crate::config::Settings,
    app_command_tx: &mpsc::Sender<AppCommand>,
    effects: Vec<RssAction>,
) {
    fn set_rss_status(app_state: &mut AppState, message: impl Into<String>) {
        app_state.ui.rss.status_message = Some(message.into());
    }

    for effect in effects {
        match effect {
            RssAction::ToNormal => app_state.mode = AppMode::Normal,
            RssAction::ToggleHistory => {
                if matches!(app_state.ui.rss.active_screen, RssScreen::History) {
                    app_state.ui.rss.active_screen = RssScreen::Unified;
                    app_state.ui.rss.focused_section = RssSectionFocus::Explorer;
                } else {
                    app_state.ui.rss.active_screen = RssScreen::History;
                }
            }
            RssAction::FocusNext => {
                if matches!(app_state.ui.rss.active_screen, RssScreen::Unified) {
                    app_state.ui.rss.focused_section = next_focus(app_state.ui.rss.focused_section);
                }
            }
            RssAction::MoveUp => {
                let len = current_list_len(app_state, settings);
                let index = selected_index_mut(app_state);
                if len > 0 {
                    *index = index.saturating_sub(1);
                } else {
                    *index = 0;
                }
            }
            RssAction::MoveDown => {
                let len = current_list_len(app_state, settings);
                let index = selected_index_mut(app_state);
                if len > 0 {
                    *index = (*index + 1).min(len - 1);
                } else {
                    *index = 0;
                }
            }
            RssAction::TriggerSync => {
                let now = Instant::now();
                if let Some(last) = app_state.ui.rss.last_sync_request_at {
                    if now.duration_since(last) < Duration::from_secs(1) {
                        set_rss_status(app_state, "RSS sync throttled");
                        continue;
                    }
                }
                app_state.ui.rss.last_sync_request_at = Some(now);

                if !settings.rss.enabled {
                    let mut new_settings = settings.clone();
                    new_settings.rss.enabled = true;
                    let _ = app_command_tx.try_send(AppCommand::UpdateConfig(new_settings));
                }
                if app_command_tx.try_send(AppCommand::RssSyncNow).is_err() {
                    set_rss_status(app_state, "RSS sync enqueue failed");
                } else {
                    set_rss_status(app_state, "RSS sync requested");
                }
            }
            RssAction::InsertChar(c) => {
                if app_state.ui.rss.is_editing {
                    app_state.ui.rss.edit_buffer.push(c);
                    if matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters) {
                        app_state.ui.rss.filter_draft = app_state.ui.rss.edit_buffer.clone();
                    }
                } else if app_state.ui.rss.is_searching {
                    app_state.ui.rss.search_query.push(c);
                }
            }
            RssAction::Backspace => {
                if app_state.ui.rss.is_editing {
                    app_state.ui.rss.edit_buffer.pop();
                    if matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters) {
                        app_state.ui.rss.filter_draft = app_state.ui.rss.edit_buffer.clone();
                    }
                } else if app_state.ui.rss.is_searching {
                    app_state.ui.rss.search_query.pop();
                }
            }
            RssAction::CommitInput => {
                if app_state.ui.rss.is_editing {
                    let value = app_state.ui.rss.edit_buffer.trim().to_string();
                    if !value.is_empty() {
                        let mut new_settings = settings.clone();
                        match app_state.ui.rss.focused_section {
                            RssSectionFocus::Links => {
                                if !is_valid_feed_url(&value) {
                                    set_rss_status(app_state, "Invalid feed URL (use http/https)");
                                    app_state.ui.rss.is_editing = false;
                                    app_state.ui.rss.edit_buffer.clear();
                                    continue;
                                }
                                new_settings.rss.enabled = true;
                                new_settings.rss.feeds.push(crate::config::RssFeed {
                                    url: value,
                                    enabled: true,
                                });
                                set_rss_status(app_state, "Link added");
                            }
                            RssSectionFocus::Filters => {
                                new_settings.rss.filters.push(crate::config::RssFilter {
                                    query: value,
                                    enabled: true,
                                });
                                app_state.ui.rss.filter_draft.clear();
                                set_rss_status(app_state, "Filter added");
                            }
                            RssSectionFocus::Explorer => {}
                        }
                        let _ = app_command_tx.try_send(AppCommand::UpdateConfig(new_settings));
                    }
                    app_state.ui.rss.is_editing = false;
                    app_state.ui.rss.edit_buffer.clear();
                } else if app_state.ui.rss.is_searching {
                    if app_state.ui.rss.search_query.trim().is_empty() {
                        app_state.ui.rss.is_searching = false;
                        set_rss_status(app_state, "Search cleared");
                    } else {
                        set_rss_status(app_state, "Search applied");
                    }
                }
            }
            RssAction::CancelInput => {
                if app_state.ui.rss.is_editing {
                    app_state.ui.rss.is_editing = false;
                    app_state.ui.rss.edit_buffer.clear();
                    app_state.ui.rss.filter_draft.clear();
                    set_rss_status(app_state, "Edit cancelled");
                } else if app_state.ui.rss.is_searching {
                    app_state.ui.rss.is_searching = false;
                    app_state.ui.rss.search_query.clear();
                    set_rss_status(app_state, "Search cleared");
                } else {
                    app_state.mode = AppMode::Normal;
                }
            }
            RssAction::AddEntry => {
                if matches!(app_state.ui.rss.active_screen, RssScreen::Unified)
                    && matches!(
                        app_state.ui.rss.focused_section,
                        RssSectionFocus::Links | RssSectionFocus::Filters
                    )
                {
                    app_state.ui.rss.is_editing = true;
                    app_state.ui.rss.edit_buffer.clear();
                    set_rss_status(app_state, "Editing new entry");
                }
            }
            RssAction::DeleteEntry => {
                if !matches!(app_state.ui.rss.active_screen, RssScreen::Unified) {
                    continue;
                }

                let mut new_settings = settings.clone();
                match app_state.ui.rss.focused_section {
                    RssSectionFocus::Links => {
                        if let Some(idx) = selected_feed_actual_idx(
                            &new_settings,
                            app_state.ui.rss.selected_feed_index,
                        ) {
                            new_settings.rss.feeds.remove(idx);
                            app_state.ui.rss.selected_feed_index =
                                app_state.ui.rss.selected_feed_index.saturating_sub(1);
                            let _ = app_command_tx.try_send(AppCommand::UpdateConfig(new_settings));
                            set_rss_status(app_state, "Link deleted");
                        }
                    }
                    RssSectionFocus::Filters => {
                        if let Some(idx) = selected_filter_actual_idx(
                            &new_settings,
                            app_state.ui.rss.selected_filter_index,
                        ) {
                            new_settings.rss.filters.remove(idx);
                            app_state.ui.rss.selected_filter_index =
                                app_state.ui.rss.selected_filter_index.saturating_sub(1);
                            let _ = app_command_tx.try_send(AppCommand::UpdateConfig(new_settings));
                            set_rss_status(app_state, "Filter deleted");
                        }
                    }
                    RssSectionFocus::Explorer => {}
                }
            }
            RssAction::ToggleFeedEnabled => {
                if !matches!(app_state.ui.rss.active_screen, RssScreen::Unified) {
                    continue;
                }

                let mut new_settings = settings.clone();
                match app_state.ui.rss.focused_section {
                    RssSectionFocus::Links => {
                        if let Some(idx) = selected_feed_actual_idx(
                            &new_settings,
                            app_state.ui.rss.selected_feed_index,
                        ) {
                            new_settings.rss.feeds[idx].enabled =
                                !new_settings.rss.feeds[idx].enabled;
                            let enabled = new_settings.rss.feeds[idx].enabled;
                            let _ = app_command_tx.try_send(AppCommand::UpdateConfig(new_settings));
                            set_rss_status(
                                app_state,
                                if enabled {
                                    "Link enabled"
                                } else {
                                    "Link disabled"
                                },
                            );
                        }
                    }
                    RssSectionFocus::Filters => {
                        if let Some(idx) = selected_filter_actual_idx(
                            &new_settings,
                            app_state.ui.rss.selected_filter_index,
                        ) {
                            new_settings.rss.filters[idx].enabled =
                                !new_settings.rss.filters[idx].enabled;
                            let enabled = new_settings.rss.filters[idx].enabled;
                            let _ = app_command_tx.try_send(AppCommand::UpdateConfig(new_settings));
                            set_rss_status(
                                app_state,
                                if enabled {
                                    "Filter enabled"
                                } else {
                                    "Filter disabled"
                                },
                            );
                        }
                    }
                    RssSectionFocus::Explorer => {}
                }
            }
            RssAction::StartSearch => {
                if (matches!(app_state.ui.rss.active_screen, RssScreen::Unified)
                    && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer))
                    || matches!(app_state.ui.rss.active_screen, RssScreen::History)
                {
                    app_state.ui.rss.is_searching = true;
                    set_rss_status(app_state, "Search mode");
                }
            }
        }
    }
}

fn apply_pasted_text(app_state: &mut AppState, pasted_text: &str) {
    let trimmed = pasted_text.trim();
    if trimmed.is_empty() {
        return;
    }

    if app_state.ui.rss.is_editing {
        app_state.ui.rss.edit_buffer.push_str(trimmed);
        if matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters) {
            app_state.ui.rss.filter_draft = app_state.ui.rss.edit_buffer.clone();
        }
        app_state.ui.rss.status_message = Some("Pasted input".to_string());
    } else if app_state.ui.rss.is_searching {
        app_state.ui.rss.search_query.push_str(trimmed);
        app_state.ui.rss.status_message = Some("Pasted search".to_string());
    }
}

pub fn handle_event(
    event: CrosstermEvent,
    app_state: &mut AppState,
    settings: &crate::config::Settings,
    app_command_tx: &mpsc::Sender<AppCommand>,
) {
    if !matches!(app_state.mode, AppMode::Rss) {
        return;
    }

    match event {
        CrosstermEvent::Key(key) => {
            if let Some(action) = map_key_to_rss_action(key.code, key.kind, app_state) {
                let result = reduce_rss_action(action);
                execute_rss_effects(app_state, settings, app_command_tx, result.effects);
                app_state.ui.needs_redraw = true;
            }
        }
        CrosstermEvent::Paste(pasted_text) => {
            apply_pasted_text(app_state, pasted_text.as_str());
            app_state.ui.needs_redraw = true;
        }
        _ => {}
    }
}

fn draw_input_panel(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>) {
    let app_state = screen.app.state;
    let ctx = screen.theme;

    let (title, value) = if app_state.ui.rss.is_searching {
        (
            " RSS Search ".to_string(),
            app_state.ui.rss.search_query.clone(),
        )
    } else {
        let label = match app_state.ui.rss.focused_section {
            RssSectionFocus::Links => "Add Link",
            RssSectionFocus::Filters => "Add Filter",
            RssSectionFocus::Explorer => "Input",
        };
        (
            format!(" RSS {} ", label),
            app_state.ui.rss.edit_buffer.clone(),
        )
    };

    let line = Line::from(vec![
        Span::styled(
            "> ",
            ctx.apply(Style::default().fg(ctx.state_selected()).bold()),
        ),
        Span::raw(value),
        Span::styled("_", ctx.apply(Style::default().fg(ctx.state_warning()))),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(ctx.apply(Style::default().fg(ctx.state_selected())));
    f.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_shared_footer(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>) {
    let ctx = screen.theme;
    let app_state = screen.app.state;
    let mut footer_spans = vec![];
    let mut push_action = |key: &str, action: &str, key_color: Color| {
        footer_spans.push(Span::styled(
            format!("[{}]", key),
            ctx.apply(Style::default().fg(key_color).bold()),
        ));
        footer_spans.push(Span::styled(
            action.to_string(),
            ctx.apply(Style::default().fg(ctx.theme.semantic.subtext0)),
        ));
        footer_spans.push(Span::styled(
            " | ",
            ctx.apply(Style::default().fg(ctx.theme.semantic.overlay0)),
        ));
    };

    if app_state.ui.rss.is_editing {
        push_action("Enter", "save", ctx.state_success());
        push_action("Esc", "cancel", ctx.state_error());
    } else if app_state.ui.rss.is_searching {
        push_action("Enter", "apply", ctx.state_success());
        push_action("Esc", "clear", ctx.state_error());
    } else {
        push_action("Tab", "next-pane", ctx.state_selected());
        push_action("h", "history", ctx.accent_sapphire());
        push_action("s", "ync", ctx.state_warning());
        match app_state.ui.rss.active_screen {
            RssScreen::Unified => match app_state.ui.rss.focused_section {
                RssSectionFocus::Links => {
                    push_action("a", "dd", ctx.state_success());
                    push_action("d", "elete", ctx.state_error());
                    push_action("Space", "toggle", ctx.state_info());
                }
                RssSectionFocus::Filters => {
                    push_action("a", "dd", ctx.state_success());
                    push_action("d", "elete", ctx.state_error());
                    push_action("Space", "toggle", ctx.state_info());
                }
                RssSectionFocus::Explorer => {
                    push_action("/", "search", ctx.accent_sapphire());
                }
            },
            RssScreen::History => {}
        }
        push_action("Esc", "back", ctx.state_error());
    }

    if !footer_spans.is_empty() {
        footer_spans.pop();
    }

    let footer = Line::from(footer_spans);

    let p = Paragraph::new(footer)
        .style(ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)))
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}

fn sync_countdown_label(next_sync_at: &str) -> Option<String> {
    let next_sync = DateTime::parse_from_rfc3339(next_sync_at).ok()?;
    let remaining_secs = next_sync
        .with_timezone(&Utc)
        .signed_duration_since(Utc::now())
        .num_seconds();
    if remaining_secs <= 0 {
        return None;
    }

    let hours = remaining_secs / 3600;
    let minutes = (remaining_secs % 3600) / 60;
    let seconds = remaining_secs % 60;

    let label = if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    };
    Some(label)
}

fn filtered_history_entries<'a>(
    history: &'a [crate::config::RssHistoryEntry],
    search_query: &str,
) -> Vec<&'a crate::config::RssHistoryEntry> {
    let query = search_query.trim().to_lowercase();
    if query.is_empty() {
        return history.iter().collect();
    }

    history
        .iter()
        .filter(|entry| {
            entry.title.to_lowercase().contains(&query)
                || entry
                    .source
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query)
                || entry.date_iso.to_lowercase().contains(&query)
        })
        .collect()
}

fn human_readable_history_time(date_iso: &str) -> String {
    DateTime::parse_from_rfc3339(date_iso)
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| date_iso.to_string())
}

fn pane_block<'a>(title: &'a str, active: bool, ctx: &crate::theme::ThemeContext) -> Block<'a> {
    let border_style = if active {
        ctx.apply(Style::default().fg(ctx.state_selected()))
    } else {
        ctx.apply(Style::default().fg(ctx.theme.semantic.border))
    };

    Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(border_style)
}

fn draw_links(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>, active: bool) {
    let app_state = screen.app.state;
    let settings = screen.settings;
    let ctx = screen.theme;
    let selected = app_state.ui.rss.selected_feed_index;

    let sorted_indices = sorted_feed_indices(settings);
    let sync_countdown = app_state
        .rss_runtime
        .next_sync_at
        .as_deref()
        .and_then(sync_countdown_label);
    let mut lines: Vec<Line<'static>> = sorted_indices
        .iter()
        .map(|idx| {
            let feed = &settings.rss.feeds[*idx];
            let style = if feed.enabled {
                ctx.apply(Style::default().fg(ctx.theme.semantic.text))
            } else {
                ctx.apply(
                    Style::default()
                        .fg(ctx.theme.semantic.overlay0)
                        .add_modifier(Modifier::CROSSED_OUT),
                )
            };
            let mut spans = vec![Span::styled(feed.url.clone(), style)];
            if let Some(countdown) = &sync_countdown {
                spans.push(Span::styled(
                    format!(" ({})", countdown),
                    ctx.apply(Style::default().fg(ctx.theme.semantic.subtext0)),
                ));
            }
            Line::from(spans)
        })
        .collect();

    let mut feed_error_rows: Vec<_> = app_state
        .rss_runtime
        .feed_errors
        .iter()
        .map(|(url, err)| (url.clone(), err.message.clone()))
        .collect();
    feed_error_rows.sort_by(|a, b| a.0.cmp(&b.0));
    if !feed_error_rows.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Feed errors:",
            ctx.apply(Style::default().fg(ctx.state_error()).bold()),
        )]));
        for (url, message) in feed_error_rows {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}: ", url),
                    ctx.apply(Style::default().fg(ctx.theme.semantic.subtext0)),
                ),
                Span::styled(message, ctx.apply(Style::default().fg(ctx.state_error()))),
            ]));
        }
    }

    let items: Vec<ListItem<'static>> = lines.into_iter().map(ListItem::new).collect();
    let mut state = ListState::default();
    if !sorted_indices.is_empty() {
        state.select(Some(selected.min(sorted_indices.len() - 1)));
    }
    let highlight_style = if active {
        screen
            .theme
            .apply(Style::default().fg(screen.theme.state_selected()).bold())
    } else {
        screen
            .theme
            .apply(Style::default().fg(screen.theme.theme.semantic.text).bold())
    };
    f.render_stateful_widget(
        List::new(items)
            .block(pane_block("Links", active, screen.theme))
            .highlight_style(highlight_style),
        area,
        &mut state,
    );
}

fn active_filter_query(app_state: &AppState, settings: &crate::config::Settings) -> String {
    if app_state.ui.rss.is_editing
        && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters)
    {
        return app_state.ui.rss.edit_buffer.clone();
    }

    settings
        .rss
        .filters
        .get(
            selected_filter_actual_idx(settings, app_state.ui.rss.selected_filter_index)
                .unwrap_or(app_state.ui.rss.selected_filter_index),
        )
        .filter(|f| f.enabled)
        .map(|f| f.query.clone())
        .unwrap_or_default()
}

fn compute_filter_preview_items(
    preview_items: &[crate::app::RssPreviewItem],
    draft: &str,
) -> Vec<(crate::app::RssPreviewItem, bool)> {
    let draft = draft.trim();
    if draft.is_empty() {
        return preview_items
            .iter()
            .cloned()
            .map(|item| (item, true))
            .collect();
    }

    let matcher = SkimMatcherV2::default();
    let draft_lc = draft.to_lowercase();

    let mut ranked: Vec<(crate::app::RssPreviewItem, bool)> = preview_items
        .iter()
        .map(|item| {
            let is_match = matcher
                .fuzzy_match(&item.title.to_lowercase(), &draft_lc)
                .is_some();
            (item.clone(), is_match)
        })
        .collect();

    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked
}

fn draw_filters(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>, active: bool) {
    let app_state = screen.app.state;
    let settings = screen.settings;
    let ctx = screen.theme;
    let selected = app_state.ui.rss.selected_filter_index;
    let is_creating_filter = app_state.ui.rss.is_editing
        && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters);
    let draft_lc = app_state.ui.rss.edit_buffer.trim().to_lowercase();

    let mut sorted_indices = sorted_filter_indices(settings);
    if is_creating_filter && !draft_lc.is_empty() {
        sorted_indices.sort_by(|a, b| {
            let a_query = settings.rss.filters[*a].query.to_lowercase();
            let b_query = settings.rss.filters[*b].query.to_lowercase();
            let a_match = a_query.contains(&draft_lc);
            let b_match = b_query.contains(&draft_lc);
            b_match.cmp(&a_match).then_with(|| a_query.cmp(&b_query))
        });
    }
    let items: Vec<ListItem<'static>> = sorted_indices
        .iter()
        .map(|idx| {
            let filter = &settings.rss.filters[*idx];
            let filter_text = filter.query.clone();
            let is_matching_existing = is_creating_filter
                && !draft_lc.is_empty()
                && filter_text.to_lowercase().contains(&draft_lc);
            let style = if !filter.enabled {
                ctx.apply(
                    Style::default()
                        .fg(ctx.theme.semantic.overlay0)
                        .add_modifier(Modifier::CROSSED_OUT),
                )
            } else if is_matching_existing {
                ctx.apply(Style::default().fg(ctx.theme.semantic.overlay0))
            } else {
                ctx.apply(Style::default().fg(ctx.theme.semantic.text))
            };
            let live_matches =
                compute_filter_preview_items(&app_state.rss_runtime.preview_items, &filter_text)
                    .into_iter()
                    .filter(|(_, is_match)| *is_match)
                    .count();

            ListItem::new(Line::from(vec![Span::styled(
                format!("{} ({})", filter_text, live_matches),
                style,
            )]))
        })
        .collect();
    let mut state = ListState::default();
    if !sorted_indices.is_empty() {
        state.select(Some(selected.min(sorted_indices.len() - 1)));
    }
    let highlight_style = if active {
        screen
            .theme
            .apply(Style::default().fg(screen.theme.state_selected()).bold())
    } else {
        screen
            .theme
            .apply(Style::default().fg(screen.theme.theme.semantic.text).bold())
    };
    f.render_stateful_widget(
        List::new(items)
            .block(pane_block("Filters", active, screen.theme))
            .highlight_style(highlight_style),
        area,
        &mut state,
    );
}

fn compute_explorer_items(
    preview_items: &[crate::app::RssPreviewItem],
    search_query: &str,
    enabled_filters: &[String],
    draft_filter_query: &str,
    prefer_draft_sort: bool,
) -> (Vec<crate::app::RssPreviewItem>, Vec<bool>, bool) {
    let search = search_query.to_lowercase();
    let has_search = !search.is_empty();
    let draft_q = draft_filter_query.to_lowercase();
    let has_draft_query = !draft_q.is_empty();
    let matcher = SkimMatcherV2::default();

    let has_enabled_filters = !enabled_filters.is_empty();
    let prioritise_matches = has_search || has_enabled_filters || has_draft_query;

    let mut items = preview_items.to_vec();
    let mut combined_match: Vec<bool> = items
        .iter()
        .map(|item| {
            let search_hit = has_search && item.title.to_lowercase().contains(&search);
            let item_title_lc = item.title.to_lowercase();
            let enabled_filter_hit = enabled_filters
                .iter()
                .any(|q| matcher.fuzzy_match(&item_title_lc, q).is_some());
            let draft_hit =
                has_draft_query && matcher.fuzzy_match(&item_title_lc, &draft_q).is_some();
            enabled_filter_hit || search_hit || draft_hit
        })
        .collect();

    if has_search {
        let filtered: Vec<(crate::app::RssPreviewItem, bool)> = items
            .into_iter()
            .zip(combined_match)
            .filter(|(_, is_match)| *is_match)
            .collect();
        let mut filtered = filtered;
        filtered.sort_by(|a, b| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()));
        combined_match = filtered.iter().map(|p| p.1).collect();
        items = filtered.into_iter().map(|p| p.0).collect();
        return (items, combined_match, prioritise_matches);
    }

    let mut paired: Vec<(crate::app::RssPreviewItem, bool, Option<i64>)> = items
        .into_iter()
        .zip(combined_match)
        .map(|(item, is_match)| {
            let draft_score = if has_draft_query {
                matcher.fuzzy_match(&item.title.to_lowercase(), &draft_q)
            } else {
                None
            };
            (item, is_match, draft_score)
        })
        .collect();
    if prioritise_matches {
        if prefer_draft_sort && has_draft_query {
            paired.sort_by(|a, b| {
                b.2.is_some()
                    .cmp(&a.2.is_some())
                    .then_with(|| b.2.unwrap_or(0).cmp(&a.2.unwrap_or(0)))
                    .then_with(|| b.1.cmp(&a.1))
                    .then_with(|| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()))
            });
        } else {
            paired.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()))
            });
        }
    } else {
        paired.sort_by(|a, b| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()));
    }
    combined_match = paired.iter().map(|p| p.1).collect();
    items = paired.into_iter().map(|p| p.0).collect();

    (items, combined_match, prioritise_matches)
}

fn rss_item_completion_percent(item: &crate::app::RssPreviewItem, app_state: &AppState) -> f64 {
    if let Some(link) = &item.link {
        if link.starts_with("magnet:") {
            let (v1_hash, v2_hash) = crate::app::parse_hybrid_hashes(link);
            for hash in [v1_hash, v2_hash].into_iter().flatten() {
                if let Some(torrent) = app_state.torrents.get(&hash) {
                    return crate::app::torrent_completion_percent(&torrent.latest_state);
                }
            }
        }
    }

    if item.is_downloaded {
        100.0
    } else {
        0.0
    }
}

fn draw_explorer(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>, active: bool) {
    let app_state = screen.app.state;
    let settings = screen.settings;
    let ctx = screen.theme;
    let matcher = SkimMatcherV2::default();
    let selected = app_state
        .ui
        .rss
        .selected_explorer_index
        .min(app_state.rss_runtime.preview_items.len().saturating_sub(1));

    let enabled_filters = enabled_filter_queries(settings);
    let explorer_greyed_out = explorer_should_be_greyed_out(settings);
    let is_creating_filter = app_state.ui.rss.is_editing
        && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters);
    let filter_query = active_filter_query(app_state, settings);
    let (items, combined_match, prioritise_matches) = compute_explorer_items(
        &app_state.rss_runtime.preview_items,
        &app_state.ui.rss.search_query,
        &enabled_filters,
        &filter_query,
        is_creating_filter,
    );

    let list_items: Vec<ListItem<'static>> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_combined_match = combined_match.get(i).copied().unwrap_or(item.is_match);
            let item_title_lc = item.title.to_lowercase();
            let draft_lc = app_state.ui.rss.edit_buffer.trim().to_lowercase();
            let draft_hit =
                !draft_lc.is_empty() && matcher.fuzzy_match(&item_title_lc, &draft_lc).is_some();
            let existing_filter_hit = settings
                .rss
                .filters
                .iter()
                .filter(|f| f.enabled)
                .map(|f| f.query.trim().to_lowercase())
                .filter(|q| !q.is_empty())
                .any(|q| matcher.fuzzy_match(&item_title_lc, &q).is_some());

            let dim_as_other_filter_match = is_creating_filter && existing_filter_hit && !draft_hit;
            let style = if explorer_greyed_out
                || dim_as_other_filter_match
                || (prioritise_matches && !is_combined_match)
            {
                ctx.apply(Style::default().fg(ctx.theme.semantic.overlay0))
            } else {
                ctx.apply(Style::default().fg(ctx.theme.semantic.text))
            };

            let completion_pct = rss_item_completion_percent(item, app_state);
            let src = item.source.clone().unwrap_or_else(|| "unknown".to_string());
            let line_text = if !is_combined_match && !item.is_downloaded {
                format!("{} ({})", item.title, src)
            } else {
                format!("{:>5.1}% {} ({})", completion_pct, item.title, src)
            };
            ListItem::new(Line::from(vec![Span::styled(line_text, style)]))
        })
        .collect();

    let mut state = ListState::default();
    if active && !items.is_empty() {
        state.select(Some(selected.min(items.len() - 1)));
    }
    let suppress_selection_highlight = app_state.ui.rss.is_editing
        && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters);
    let highlight_style = if suppress_selection_highlight || explorer_greyed_out {
        screen.theme.apply(Style::default())
    } else if active {
        screen
            .theme
            .apply(Style::default().fg(screen.theme.state_selected()).bold())
    } else {
        screen
            .theme
            .apply(Style::default().fg(screen.theme.theme.semantic.text).bold())
    };
    f.render_stateful_widget(
        List::new(list_items)
            .block(pane_block("Explorer", active, screen.theme))
            .highlight_style(highlight_style),
        area,
        &mut state,
    );
}

fn draw_history(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>) {
    let app_state = screen.app.state;
    let ctx = screen.theme;
    let selected = app_state.ui.rss.selected_history_index;

    let filtered = filtered_history_entries(
        &app_state.rss_runtime.history,
        &app_state.ui.rss.search_query,
    );

    let lines: Vec<Line<'static>> = filtered
        .iter()
        .map(|entry| {
            let src = entry
                .source
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            Line::from(format!(
                "{} | {} | {}",
                human_readable_history_time(&entry.date_iso),
                src,
                entry.title
            ))
        })
        .collect();

    let items: Vec<ListItem<'static>> = lines.into_iter().map(ListItem::new).collect();
    let mut state = ListState::default();
    if !filtered.is_empty() {
        state.select(Some(selected.min(filtered.len() - 1)));
    }
    f.render_stateful_widget(
        List::new(items)
            .block(pane_block("History", true, ctx))
            .highlight_style(ctx.apply(Style::default().fg(ctx.state_selected()).bold())),
        area,
        &mut state,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnifiedLayout {
    Wide,
    Narrow,
}

fn unified_layout_for_width(width: u16) -> UnifiedLayout {
    if width >= 140 {
        UnifiedLayout::Wide
    } else {
        UnifiedLayout::Narrow
    }
}

fn draw_unified_body(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>, show_history: bool) {
    let app_state = screen.app.state;
    if matches!(unified_layout_for_width(area.width), UnifiedLayout::Wide) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        let right_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(cols[1]);

        if show_history {
            draw_history(f, cols[0], screen);
        } else {
            draw_explorer(
                f,
                cols[0],
                screen,
                matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer),
            );
        }
        draw_links(
            f,
            right_rows[0],
            screen,
            !show_history && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Links),
        );
        draw_filters(
            f,
            right_rows[1],
            screen,
            !show_history && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters),
        );
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(area);

        if show_history {
            draw_history(f, rows[0], screen);
        } else {
            draw_explorer(
                f,
                rows[0],
                screen,
                matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer),
            );
        }
        draw_filters(
            f,
            rows[1],
            screen,
            !show_history && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters),
        );
        draw_links(
            f,
            rows[2],
            screen,
            !show_history && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Links),
        );
    }
}

pub fn draw(f: &mut Frame, screen: &ScreenContext<'_>) {
    let area = centered_rect(88, 86, f.area());
    let app_state = screen.app.state;

    f.render_widget(Clear, area);

    let show_input_panel = app_state.ui.rss.is_editing || app_state.ui.rss.is_searching;
    let constraints = if show_input_panel {
        vec![
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ]
    } else {
        vec![Constraint::Min(5), Constraint::Length(1)]
    };

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    if show_input_panel {
        draw_input_panel(f, inner[0], screen);
    }
    let body_idx = if show_input_panel { 1 } else { 0 };
    let footer_idx = if show_input_panel { 2 } else { 1 };
    draw_unified_body(
        f,
        inner[body_idx],
        screen,
        matches!(app_state.ui.rss.active_screen, RssScreen::History),
    );
    draw_shared_footer(f, inner[footer_idx], screen);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::RssPreviewItem;

    fn base_state() -> AppState {
        AppState {
            mode: AppMode::Rss,
            ..Default::default()
        }
    }

    #[test]
    fn esc_returns_to_normal_mode() {
        let mut app_state = base_state();
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(2);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(matches!(app_state.mode, AppMode::Normal));
    }

    #[test]
    fn tab_cycles_focus_sections() {
        let mut app_state = base_state();
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(2);

        assert!(matches!(
            app_state.ui.rss.focused_section,
            RssSectionFocus::Explorer
        ));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Tab,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(
            app_state.ui.rss.focused_section,
            RssSectionFocus::Links
        ));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Tab,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(
            app_state.ui.rss.focused_section,
            RssSectionFocus::Filters
        ));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Tab,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(
            app_state.ui.rss.focused_section,
            RssSectionFocus::Explorer
        ));
    }

    #[test]
    fn h_toggles_history_and_returns_to_explorer_focus() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Links;
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(2);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('h'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(matches!(app_state.ui.rss.active_screen, RssScreen::History));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('h'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(matches!(app_state.ui.rss.active_screen, RssScreen::Unified));
        assert!(matches!(
            app_state.ui.rss.focused_section,
            RssSectionFocus::Explorer
        ));
    }

    #[test]
    fn down_moves_rows_and_left_right_do_not_change_focus() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Explorer;
        app_state.rss_runtime.preview_items.push(RssPreviewItem {
            title: "A".to_string(),
            ..Default::default()
        });
        app_state.rss_runtime.preview_items.push(RssPreviewItem {
            title: "B".to_string(),
            ..Default::default()
        });
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(2);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Down,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert_eq!(app_state.ui.rss.selected_explorer_index, 1);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Left,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(
            app_state.ui.rss.focused_section,
            RssSectionFocus::Explorer
        ));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Right,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(
            app_state.ui.rss.focused_section,
            RssSectionFocus::Explorer
        ));
    }

    #[test]
    fn sync_key_enqueues_command() {
        let mut app_state = base_state();
        let settings = crate::config::Settings::default();
        let (tx, mut rx) = mpsc::channel(2);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('s'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        let cmd = rx.try_recv().expect("expected rss sync command");
        assert!(matches!(cmd, AppCommand::RssSyncNow));
        assert_eq!(
            app_state.ui.rss.status_message.as_deref(),
            Some("RSS sync requested")
        );
    }

    #[test]
    fn sync_key_auto_enables_rss_when_disabled() {
        let mut app_state = base_state();
        let mut settings = crate::config::Settings::default();
        settings.rss.enabled = false;
        let (tx, mut rx) = mpsc::channel(4);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('s'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        let first = rx.try_recv().expect("expected first command");
        match first {
            AppCommand::UpdateConfig(s) => assert!(s.rss.enabled),
            _ => panic!("unexpected first command"),
        }

        let second = rx.try_recv().expect("expected second command");
        assert!(matches!(second, AppCommand::RssSyncNow));
    }

    #[test]
    fn sync_key_is_throttled_when_spammed() {
        let mut app_state = base_state();
        let settings = crate::config::Settings::default();
        let (tx, mut rx) = mpsc::channel(4);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('s'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(
            rx.try_recv().expect("expected first sync command"),
            AppCommand::RssSyncNow
        ));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('s'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(rx.try_recv().is_err());
        assert_eq!(
            app_state.ui.rss.status_message.as_deref(),
            Some("RSS sync throttled")
        );
    }

    #[test]
    fn add_link_dispatches_update_config() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Links;
        let settings = crate::config::Settings::default();
        let (tx, mut rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('a'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(app_state.ui.rss.is_editing);

        for c in "https://example.com/rss.xml".chars() {
            handle_event(
                CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                    KeyCode::Char(c),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                )),
                &mut app_state,
                &settings,
                &tx,
            );
        }

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        let cmd = rx.try_recv().expect("expected UpdateConfig dispatch");
        match cmd {
            AppCommand::UpdateConfig(s) => {
                assert_eq!(s.rss.feeds.len(), 1);
                assert_eq!(s.rss.feeds[0].url, "https://example.com/rss.xml");
                assert!(s.rss.feeds[0].enabled);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn invalid_feed_url_is_rejected() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Links;
        let settings = crate::config::Settings::default();
        let (tx, mut rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('a'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        for c in "javascript:alert(1)".chars() {
            handle_event(
                CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                    KeyCode::Char(c),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                )),
                &mut app_state,
                &settings,
                &tx,
            );
        }
        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(rx.try_recv().is_err());
        assert_eq!(
            app_state.ui.rss.status_message.as_deref(),
            Some("Invalid feed URL (use http/https)")
        );
        assert!(!app_state.ui.rss.is_editing);
    }

    #[test]
    fn paste_link_in_edit_mode_dispatches_update_config() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Links;
        let settings = crate::config::Settings::default();
        let (tx, mut rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('a'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        handle_event(
            CrosstermEvent::Paste("https://subsplease.org/rss/?t&r=1080".to_string()),
            &mut app_state,
            &settings,
            &tx,
        );

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        let cmd = rx.try_recv().expect("expected UpdateConfig dispatch");
        match cmd {
            AppCommand::UpdateConfig(s) => {
                assert_eq!(s.rss.feeds.len(), 1);
                assert_eq!(s.rss.feeds[0].url, "https://subsplease.org/rss/?t&r=1080");
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn delete_link_dispatches_update_config() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Links;
        let mut settings = crate::config::Settings::default();
        settings.rss.feeds.push(crate::config::RssFeed {
            url: "https://a.test/rss".to_string(),
            enabled: true,
        });
        let (tx, mut rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('d'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        let cmd = rx.try_recv().expect("expected UpdateConfig dispatch");
        match cmd {
            AppCommand::UpdateConfig(s) => assert!(s.rss.feeds.is_empty()),
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn toggle_link_dispatches_update_config() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Links;
        let mut settings = crate::config::Settings::default();
        settings.rss.feeds.push(crate::config::RssFeed {
            url: "https://a.test/rss".to_string(),
            enabled: true,
        });
        let (tx, mut rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char(' '),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        let cmd = rx.try_recv().expect("expected UpdateConfig dispatch");
        match cmd {
            AppCommand::UpdateConfig(s) => assert!(!s.rss.feeds[0].enabled),
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn toggle_filter_dispatches_update_config() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Filters;
        let mut settings = crate::config::Settings::default();
        settings.rss.filters.push(crate::config::RssFilter {
            query: "ubuntu".to_string(),
            enabled: true,
        });
        let (tx, mut rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char(' '),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        let cmd = rx.try_recv().expect("expected UpdateConfig dispatch");
        match cmd {
            AppCommand::UpdateConfig(s) => assert!(!s.rss.filters[0].enabled),
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn explorer_search_mode_sets_and_clears_status() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Explorer;
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('/'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(app_state.ui.rss.is_searching);
        assert_eq!(
            app_state.ui.rss.status_message.as_deref(),
            Some("Search mode")
        );

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(!app_state.ui.rss.is_searching);
        assert_eq!(
            app_state.ui.rss.status_message.as_deref(),
            Some("Search cleared")
        );
    }

    #[test]
    fn history_search_mode_sets_and_clears_status() {
        let mut app_state = base_state();
        app_state.ui.rss.active_screen = RssScreen::History;
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('/'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(app_state.ui.rss.is_searching);
        assert_eq!(
            app_state.ui.rss.status_message.as_deref(),
            Some("Search mode")
        );

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(!app_state.ui.rss.is_searching);
        assert_eq!(
            app_state.ui.rss.status_message.as_deref(),
            Some("Search cleared")
        );
    }

    #[test]
    fn backspace_does_not_exit_search_mode_when_query_becomes_empty() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Explorer;
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('/'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('x'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Backspace,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(app_state.ui.rss.is_searching);
        assert!(app_state.ui.rss.search_query.is_empty());
    }

    #[test]
    fn explorer_compute_filters_out_non_matches_when_search_active() {
        let items = vec![
            RssPreviewItem {
                title: "Ubuntu LTS".to_string(),
                is_match: true,
                ..Default::default()
            },
            RssPreviewItem {
                title: "Fedora".to_string(),
                is_match: false,
                ..Default::default()
            },
        ];

        let (sorted, combined, prioritise) =
            compute_explorer_items(&items, "ubuntu", &[], "", false);
        assert!(prioritise);
        assert_eq!(sorted.len(), 1);
        assert_eq!(combined.len(), 1);
        assert_eq!(sorted[0].title, "Ubuntu LTS");
    }

    #[test]
    fn search_enter_keeps_mode_active_when_query_non_empty() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Explorer;
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(8);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('/'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('f'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Enter,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(app_state.ui.rss.is_searching);
    }

    #[test]
    fn explorer_compute_sorts_matches_first_only_when_active() {
        let items = vec![
            RssPreviewItem {
                title: "Non match".to_string(),
                is_match: false,
                ..Default::default()
            },
            RssPreviewItem {
                title: "Match".to_string(),
                is_match: true,
                ..Default::default()
            },
        ];

        let (inactive_sorted, _, inactive_prioritise) =
            compute_explorer_items(&items, "", &[], "", false);
        assert!(!inactive_prioritise);
        assert_eq!(inactive_sorted[0].title, "Match");

        let enabled = vec!["match".to_string()];
        let (active_sorted, _, active_prioritise) =
            compute_explorer_items(&items, "", &enabled, "", false);
        assert!(active_prioritise);
        assert_eq!(active_sorted[0].title, "Match");
    }

    #[test]
    fn explorer_compute_prefers_draft_matches_while_creating_filter() {
        let items = vec![
            RssPreviewItem {
                title: "Series Beta".to_string(),
                ..Default::default()
            },
            RssPreviewItem {
                title: "Series Alpha".to_string(),
                ..Default::default()
            },
        ];

        let enabled = vec!["jigokuraku".to_string()];
        let (sorted, _, prioritise) =
            compute_explorer_items(&items, "", &enabled, "juju", true);
        assert!(prioritise);
        assert_eq!(sorted[0].title, "Series Alpha");
    }

    #[test]
    fn filter_preview_keeps_all_items_and_sorts_matches_first() {
        let items = vec![
            RssPreviewItem {
                title: "Fedora".to_string(),
                ..Default::default()
            },
            RssPreviewItem {
                title: "Ubuntu LTS".to_string(),
                ..Default::default()
            },
        ];

        let ranked = compute_filter_preview_items(&items, "ubuntu");
        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].1);
        assert_eq!(ranked[0].0.title, "Ubuntu LTS");
        assert!(!ranked[1].1);
        assert_eq!(ranked[1].0.title, "Fedora");
    }

    #[test]
    fn filter_preview_with_empty_draft_still_shows_full_list() {
        let items = vec![
            RssPreviewItem {
                title: "Fedora".to_string(),
                ..Default::default()
            },
            RssPreviewItem {
                title: "Ubuntu".to_string(),
                ..Default::default()
            },
        ];

        let ranked = compute_filter_preview_items(&items, "");
        assert_eq!(ranked.len(), 2);
        assert!(ranked.iter().all(|(_, is_match)| *is_match));
    }

    #[test]
    fn active_filter_query_uses_selected_filter_in_nav_mode() {
        let mut app_state = base_state();
        app_state.ui.rss.active_screen = RssScreen::Unified;
        app_state.ui.rss.focused_section = RssSectionFocus::Filters;
        app_state.ui.rss.is_editing = false;
        app_state.ui.rss.filter_draft.clear();
        app_state.ui.rss.selected_filter_index = 1;

        let mut settings = crate::config::Settings::default();
        settings.rss.filters.push(crate::config::RssFilter {
            query: "ubuntu".to_string(),
            enabled: true,
        });
        settings.rss.filters.push(crate::config::RssFilter {
            query: "fedora".to_string(),
            enabled: true,
        });

        let query = active_filter_query(&app_state, &settings);
        assert_eq!(query, "ubuntu");
    }

    #[test]
    fn active_filter_query_ignores_disabled_selected_filter() {
        let mut app_state = base_state();
        app_state.ui.rss.active_screen = RssScreen::Unified;
        app_state.ui.rss.focused_section = RssSectionFocus::Filters;
        app_state.ui.rss.is_editing = false;
        app_state.ui.rss.filter_draft.clear();
        app_state.ui.rss.selected_filter_index = 0;

        let mut settings = crate::config::Settings::default();
        settings.rss.filters.push(crate::config::RssFilter {
            query: "vigilante".to_string(),
            enabled: false,
        });

        let query = active_filter_query(&app_state, &settings);
        assert_eq!(query, "");
    }

    #[test]
    fn active_filter_query_ignores_stale_draft_when_not_editing() {
        let mut app_state = base_state();
        app_state.ui.rss.active_screen = RssScreen::Unified;
        app_state.ui.rss.focused_section = RssSectionFocus::Explorer;
        app_state.ui.rss.is_editing = false;
        app_state.ui.rss.edit_buffer.clear();
        app_state.ui.rss.filter_draft = "jigokuraku".to_string();

        let settings = crate::config::Settings::default();
        let query = active_filter_query(&app_state, &settings);
        assert_eq!(query, "");
    }

    #[test]
    fn explorer_greyed_out_when_no_filters_exist() {
        let settings = crate::config::Settings::default();
        assert!(explorer_should_be_greyed_out(&settings));
    }

    #[test]
    fn explorer_greyed_out_when_all_filters_disabled() {
        let mut settings = crate::config::Settings::default();
        settings.rss.filters.push(crate::config::RssFilter {
            query: "ubuntu".to_string(),
            enabled: false,
        });
        settings.rss.filters.push(crate::config::RssFilter {
            query: "fedora".to_string(),
            enabled: false,
        });
        assert!(explorer_should_be_greyed_out(&settings));
    }

    #[test]
    fn explorer_not_greyed_out_when_any_filter_enabled() {
        let mut settings = crate::config::Settings::default();
        settings.rss.filters.push(crate::config::RssFilter {
            query: "ubuntu".to_string(),
            enabled: false,
        });
        settings.rss.filters.push(crate::config::RssFilter {
            query: "fedora".to_string(),
            enabled: true,
        });
        assert!(!explorer_should_be_greyed_out(&settings));
    }

    #[test]
    fn sync_countdown_label_formats_minutes_and_seconds() {
        let future = (Utc::now() + chrono::Duration::seconds(274)).to_rfc3339();
        let label = sync_countdown_label(&future).expect("expected countdown");
        assert!(label.ends_with('s'));
        assert!(label.contains('m'));
    }

    #[test]
    fn sync_countdown_label_returns_none_for_past_timestamp() {
        let past = (Utc::now() - chrono::Duration::seconds(5)).to_rfc3339();
        assert!(sync_countdown_label(&past).is_none());
    }

    #[test]
    fn filtered_history_entries_respects_search_query() {
        let entries = vec![
            crate::config::RssHistoryEntry {
                title: "Series Alpha".to_string(),
                source: Some("ExampleFeed".to_string()),
                date_iso: "2026-02-17T10:00:00Z".to_string(),
                ..Default::default()
            },
            crate::config::RssHistoryEntry {
                title: "Series Gamma".to_string(),
                source: Some("ExampleFeed".to_string()),
                date_iso: "2026-02-16T10:00:00Z".to_string(),
                ..Default::default()
            },
        ];

        let filtered = filtered_history_entries(&entries, "juju");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Series Alpha");
    }

    #[test]
    fn human_readable_history_time_formats_rfc3339() {
        let ts = "2026-02-17T10:05:00Z";
        assert_eq!(human_readable_history_time(ts).len(), 16);
    }

    #[test]
    fn unified_layout_is_narrow_below_boundary() {
        assert!(matches!(
            unified_layout_for_width(139),
            UnifiedLayout::Narrow
        ));
    }

    #[test]
    fn unified_layout_is_wide_at_boundary() {
        assert!(matches!(unified_layout_for_width(140), UnifiedLayout::Wide));
    }
}
