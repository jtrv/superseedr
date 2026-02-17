// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::{AppCommand, AppMode, AppState, RssScreen, RssSectionFocus};
use crate::tui::formatters::centered_rect;
use crate::tui::screen_context::ScreenContext;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEventKind};
use ratatui::{prelude::*, widgets::*};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub enum RssAction {
    ToNormal,
    SwitchScreen(RssScreen),
    FocusNext,
    FocusPrev,
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
    SeedFilterFromSelectedTitle,
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
        KeyCode::Char('H') => Some(RssAction::SwitchScreen(RssScreen::History)),
        KeyCode::Tab => Some(RssAction::FocusNext),
        KeyCode::BackTab => Some(RssAction::FocusPrev),
        KeyCode::Char('h') | KeyCode::Left => Some(RssAction::FocusPrev),
        KeyCode::Char('l') | KeyCode::Right => Some(RssAction::FocusNext),
        KeyCode::Char('S') => Some(RssAction::TriggerSync),
        KeyCode::Char('a') => Some(RssAction::AddEntry),
        KeyCode::Char('d') => Some(RssAction::DeleteEntry),
        KeyCode::Char('x') => Some(RssAction::ToggleFeedEnabled),
        KeyCode::Char('/') => Some(RssAction::StartSearch),
        KeyCode::Char('F') => Some(RssAction::SeedFilterFromSelectedTitle),
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

fn prev_focus(current: RssSectionFocus) -> RssSectionFocus {
    match current {
        RssSectionFocus::Links => RssSectionFocus::Explorer,
        RssSectionFocus::Filters => RssSectionFocus::Links,
        RssSectionFocus::Explorer => RssSectionFocus::Filters,
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
        return app_state.rss_runtime.history.len();
    }

    match app_state.ui.rss.focused_section {
        RssSectionFocus::Links => settings.rss.feeds.len(),
        RssSectionFocus::Filters => settings.rss.filters.len(),
        RssSectionFocus::Explorer => app_state.rss_runtime.preview_items.len(),
    }
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
            RssAction::SwitchScreen(screen) => {
                app_state.ui.rss.active_screen = screen;
            }
            RssAction::FocusNext => {
                if matches!(app_state.ui.rss.active_screen, RssScreen::Unified) {
                    app_state.ui.rss.focused_section = next_focus(app_state.ui.rss.focused_section);
                }
            }
            RssAction::FocusPrev => {
                if matches!(app_state.ui.rss.active_screen, RssScreen::Unified) {
                    app_state.ui.rss.focused_section = prev_focus(app_state.ui.rss.focused_section);
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
                                new_settings.rss.enabled = true;
                                new_settings.rss.feeds.push(crate::config::RssFeed {
                                    url: value,
                                    enabled: true,
                                });
                                set_rss_status(app_state, "Link added");
                            }
                            RssSectionFocus::Filters => {
                                new_settings.rss.filters.push(crate::config::RssFilter {
                                    regex: value,
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
                    app_state.ui.rss.is_searching = false;
                    set_rss_status(app_state, "Search applied");
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
                        if !new_settings.rss.feeds.is_empty() {
                            let idx = app_state
                                .ui
                                .rss
                                .selected_feed_index
                                .min(new_settings.rss.feeds.len() - 1);
                            new_settings.rss.feeds.remove(idx);
                            app_state.ui.rss.selected_feed_index =
                                app_state.ui.rss.selected_feed_index.saturating_sub(1);
                            let _ = app_command_tx.try_send(AppCommand::UpdateConfig(new_settings));
                            set_rss_status(app_state, "Link deleted");
                        }
                    }
                    RssSectionFocus::Filters => {
                        if !new_settings.rss.filters.is_empty() {
                            let idx = app_state
                                .ui
                                .rss
                                .selected_filter_index
                                .min(new_settings.rss.filters.len() - 1);
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
                if !matches!(app_state.ui.rss.active_screen, RssScreen::Unified)
                    || !matches!(app_state.ui.rss.focused_section, RssSectionFocus::Links)
                {
                    continue;
                }

                let mut new_settings = settings.clone();
                if !new_settings.rss.feeds.is_empty() {
                    let idx = app_state
                        .ui
                        .rss
                        .selected_feed_index
                        .min(new_settings.rss.feeds.len() - 1);
                    new_settings.rss.feeds[idx].enabled = !new_settings.rss.feeds[idx].enabled;
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
            RssAction::StartSearch => {
                if matches!(app_state.ui.rss.active_screen, RssScreen::Unified)
                    && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer)
                {
                    app_state.ui.rss.is_searching = true;
                    set_rss_status(app_state, "Search mode");
                }
            }
            RssAction::SeedFilterFromSelectedTitle => {
                if matches!(app_state.ui.rss.active_screen, RssScreen::Unified)
                    && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer)
                {
                    let idx = app_state.ui.rss.selected_explorer_index;
                    if let Some(item) = app_state.rss_runtime.preview_items.get(idx) {
                        app_state.ui.rss.focused_section = RssSectionFocus::Filters;
                        app_state.ui.rss.is_editing = true;
                        app_state.ui.rss.edit_buffer = item.title.clone();
                        app_state.ui.rss.filter_draft = app_state.ui.rss.edit_buffer.clone();
                        set_rss_status(app_state, "Editing new filter from selection");
                    }
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

fn focus_name(focus: RssSectionFocus) -> &'static str {
    match focus {
        RssSectionFocus::Links => "Links",
        RssSectionFocus::Filters => "Filters",
        RssSectionFocus::Explorer => "Explorer",
    }
}

fn draw_shared_header(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>) {
    let app_state = screen.app.state;
    let ctx = screen.theme;
    let current = match app_state.ui.rss.active_screen {
        RssScreen::Unified => "Unified",
        RssScreen::History => "History",
    };
    let last_sync = app_state
        .rss_runtime
        .last_sync_at
        .clone()
        .unwrap_or_else(|| "never".to_string());
    let next_sync = app_state
        .rss_runtime
        .next_sync_at
        .clone()
        .unwrap_or_else(|| "n/a".to_string());
    let mode = if app_state.ui.rss.is_editing {
        "EDIT"
    } else if app_state.ui.rss.is_searching {
        "SEARCH"
    } else {
        "NAV"
    };

    let header = Line::from(vec![
        Span::styled(
            format!("RSS / {}", current),
            ctx.apply(Style::default().fg(ctx.state_selected()).bold()),
        ),
        Span::raw("  |  "),
        Span::raw(format!("Focus: {}", focus_name(app_state.ui.rss.focused_section))),
        Span::raw("  |  "),
        Span::raw(format!("Mode: {}", mode)),
        Span::raw("  |  "),
        Span::raw(format!("Last: {}", last_sync)),
        Span::raw("  |  "),
        Span::raw(format!("Next: {}", next_sync)),
    ]);

    let p = Paragraph::new(header)
        .style(ctx.apply(Style::default().fg(ctx.theme.semantic.text)))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_shared_footer(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>) {
    let ctx = screen.theme;
    let app_state = screen.app.state;
    let mut footer_spans = vec![
        Span::styled(
            "[Tab/Shift+Tab/h/l/←/→] focus [H] history [S]yncNow ",
            ctx.apply(Style::default().fg(ctx.accent_sapphire())),
        ),
    ];

    if app_state.ui.rss.is_editing {
        footer_spans.push(Span::styled(
            "[Enter] save [Esc] cancel ",
            ctx.apply(Style::default().fg(ctx.state_complete())),
        ));
    } else if app_state.ui.rss.is_searching {
        footer_spans.push(Span::styled(
            "[type] search [Enter] apply [Esc] clear ",
            ctx.apply(Style::default().fg(ctx.state_complete())),
        ));
    } else {
        match app_state.ui.rss.active_screen {
            RssScreen::Unified => match app_state.ui.rss.focused_section {
                RssSectionFocus::Links => footer_spans.push(Span::styled(
                    "Links: [a] add [d] delete [x] toggle [j/k] move ",
                    ctx.apply(Style::default().fg(ctx.state_info())),
                )),
                RssSectionFocus::Filters => footer_spans.push(Span::styled(
                    "Filters: [a] add [d] delete [j/k] move ",
                    ctx.apply(Style::default().fg(ctx.state_info())),
                )),
                RssSectionFocus::Explorer => footer_spans.push(Span::styled(
                    "Explorer: [/] search [j/k] move [F] seed filter ",
                    ctx.apply(Style::default().fg(ctx.state_info())),
                )),
            },
            RssScreen::History => footer_spans.push(Span::styled(
                "History: [j/k] move ",
                ctx.apply(Style::default().fg(ctx.state_info())),
            )),
        }
        footer_spans.push(Span::styled(
            "[Esc/q] back",
            ctx.apply(Style::default().fg(ctx.state_error())),
        ));
    }

    if let Some(status) = &app_state.ui.rss.status_message {
        footer_spans.push(Span::raw("  |  "));
        footer_spans.push(Span::styled(
            status.clone(),
            ctx.apply(Style::default().fg(ctx.state_success())),
        ));
    }

    let footer = Line::from(footer_spans);

    let p = Paragraph::new(footer)
        .style(ctx.apply(Style::default().fg(ctx.theme.semantic.subtext1)))
        .alignment(Alignment::Center);
    f.render_widget(p, area);
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
    let selected = app_state.ui.rss.selected_feed_index;

    let mut lines: Vec<Line<'static>> = settings
        .rss
        .feeds
        .iter()
        .enumerate()
        .map(|(i, feed)| {
            let cursor = if i == selected { "> " } else { "  " };
            let enabled = if feed.enabled { "[x]" } else { "[ ]" };
            Line::from(format!("{}{} {}", cursor, enabled, feed.url))
        })
        .collect();

    if app_state.ui.rss.is_editing && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Links)
    {
        lines.push(Line::from(""));
        lines.push(Line::from(format!("Draft: {}", app_state.ui.rss.edit_buffer)));
    }

    let items: Vec<ListItem<'static>> = lines.into_iter().map(ListItem::new).collect();
    f.render_widget(
        List::new(items).block(pane_block("Links", active, screen.theme)),
        area,
    );
}

fn active_filter_query(app_state: &AppState, settings: &crate::config::Settings) -> String {
    if app_state.ui.rss.is_editing && matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters)
    {
        return app_state.ui.rss.edit_buffer.clone();
    }

    let draft = app_state.ui.rss.filter_draft.trim();
    if !draft.is_empty() {
        return app_state.ui.rss.filter_draft.clone();
    }

    settings
        .rss
        .filters
        .get(app_state.ui.rss.selected_filter_index)
        .map(|f| f.regex.clone())
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
    let selected = app_state.ui.rss.selected_filter_index;

    let mut lines: Vec<Line<'static>> = settings
        .rss
        .filters
        .iter()
        .enumerate()
        .map(|(i, filter)| {
            let cursor = if i == selected { "> " } else { "  " };
            Line::from(format!("{}{}", cursor, filter.regex))
        })
        .collect();

    let draft = active_filter_query(app_state, settings);
    let ranked_preview = compute_filter_preview_items(&app_state.rss_runtime.preview_items, &draft);
    let match_count = ranked_preview
        .iter()
        .filter(|(_, is_match)| *is_match)
        .count();

    lines.push(Line::from(""));
    lines.push(Line::from(format!("Draft: {}", draft)));
    lines.push(Line::from(format!("Live matches: {}", match_count)));

    let items: Vec<ListItem<'static>> = lines.into_iter().map(ListItem::new).collect();
    f.render_widget(
        List::new(items).block(pane_block("Filters", active, screen.theme)),
        area,
    );
}

fn compute_explorer_items(
    preview_items: &[crate::app::RssPreviewItem],
    search_query: &str,
    filter_query: &str,
    has_filters: bool,
) -> (Vec<crate::app::RssPreviewItem>, Vec<bool>, bool) {
    let search = search_query.to_lowercase();
    let has_search = !search.is_empty();
    let filter_q = filter_query.to_lowercase();
    let has_filter_query = !filter_q.is_empty();
    let matcher = SkimMatcherV2::default();

    let prioritise_matches = has_search || has_filters || has_filter_query;

    let mut items = preview_items.to_vec();
    let mut combined_match: Vec<bool> = items
        .iter()
        .map(|item| {
            let search_hit = has_search && item.title.to_lowercase().contains(&search);
            let filter_hit = has_filter_query
                && matcher
                    .fuzzy_match(&item.title.to_lowercase(), &filter_q)
                    .is_some();
            item.is_match || search_hit || filter_hit
        })
        .collect();

    if prioritise_matches {
        let mut paired: Vec<(crate::app::RssPreviewItem, bool)> =
            items.into_iter().zip(combined_match).collect();
        paired.sort_by(|a, b| b.1.cmp(&a.1));
        combined_match = paired.iter().map(|p| p.1).collect();
        items = paired.into_iter().map(|p| p.0).collect();
    }

    (items, combined_match, prioritise_matches)
}

fn draw_explorer(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>, active: bool) {
    let app_state = screen.app.state;
    let settings = screen.settings;
    let ctx = screen.theme;
    let selected = app_state
        .ui
        .rss
        .selected_explorer_index
        .min(app_state.rss_runtime.preview_items.len().saturating_sub(1));

    let has_filters = !settings.rss.filters.is_empty();
    let filter_query = active_filter_query(app_state, settings);
    let (items, combined_match, prioritise_matches) = compute_explorer_items(
        &app_state.rss_runtime.preview_items,
        &app_state.ui.rss.search_query,
        &filter_query,
        has_filters,
    );

    let mut list_items: Vec<ListItem<'static>> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let cursor = if i == selected { "> " } else { "  " };
            let is_combined_match = combined_match.get(i).copied().unwrap_or(item.is_match);
            let style = if prioritise_matches && !is_combined_match {
                ctx.apply(Style::default().fg(ctx.theme.semantic.overlay0))
            } else {
                ctx.apply(Style::default().fg(ctx.theme.semantic.text))
            };

            let mut badges = String::new();
            if is_combined_match {
                badges.push('M');
            }
            if item.is_downloaded {
                badges.push('D');
            }
            if prioritise_matches && !is_combined_match {
                badges.push('d');
            }
            let src = item.source.clone().unwrap_or_else(|| "unknown".to_string());
            ListItem::new(Line::from(vec![Span::styled(
                format!("{}[{}] {} ({})", cursor, badges, item.title, src),
                style,
            )]))
        })
        .collect();

    if app_state.ui.rss.is_searching || !app_state.ui.rss.search_query.is_empty() {
        list_items.insert(
            0,
            ListItem::new(Line::from(format!("Search: {}", app_state.ui.rss.search_query))),
        );
    }

    f.render_widget(
        List::new(list_items).block(pane_block("Explorer", active, screen.theme)),
        area,
    );
}

fn draw_history(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>) {
    let app_state = screen.app.state;
    let ctx = screen.theme;
    let selected = app_state.ui.rss.selected_history_index;

    let lines: Vec<Line<'static>> = app_state
        .rss_runtime
        .history
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let cursor = if i == selected { "> " } else { "  " };
            let src = entry
                .source
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let via = match entry.added_via {
                crate::config::RssAddedVia::Auto => "auto",
                crate::config::RssAddedVia::Manual => "manual",
            };
            Line::from(format!(
                "{}{} | {} | {} | {}",
                cursor, entry.date_iso, via, src, entry.title
            ))
        })
        .collect();

    let items: Vec<ListItem<'static>> = lines.into_iter().map(ListItem::new).collect();
    f.render_widget(
        List::new(items).block(pane_block("History", true, ctx)),
        area,
    );
}

fn draw_unified_body(f: &mut Frame, area: Rect, screen: &ScreenContext<'_>) {
    let app_state = screen.app.state;
    if area.width >= 140 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        let right_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(cols[1]);

        draw_explorer(
            f,
            cols[0],
            screen,
            matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer),
        );
        draw_links(
            f,
            right_rows[0],
            screen,
            matches!(app_state.ui.rss.focused_section, RssSectionFocus::Links),
        );
        draw_filters(
            f,
            right_rows[1],
            screen,
            matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters),
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

        draw_explorer(
            f,
            rows[0],
            screen,
            matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer),
        );
        draw_filters(
            f,
            rows[1],
            screen,
            matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters),
        );
        draw_links(
            f,
            rows[2],
            screen,
            matches!(app_state.ui.rss.focused_section, RssSectionFocus::Links),
        );
    }
}

pub fn draw(f: &mut Frame, screen: &ScreenContext<'_>) {
    let area = centered_rect(88, 86, f.area());
    let app_state = screen.app.state;
    let ctx = screen.theme;

    f.render_widget(Clear, area);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(" RSS ")
            .border_style(ctx.apply(Style::default().fg(ctx.theme.semantic.border))),
        area,
    );

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    draw_shared_header(f, inner[0], screen);
    match app_state.ui.rss.active_screen {
        RssScreen::Unified => draw_unified_body(f, inner[1], screen),
        RssScreen::History => draw_history(f, inner[1], screen),
    }
    draw_shared_footer(f, inner[2], screen);
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

        assert!(matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Tab,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(app_state.ui.rss.focused_section, RssSectionFocus::Links));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::BackTab,
                ratatui::crossterm::event::KeyModifiers::SHIFT,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer));
    }

    #[test]
    fn shift_h_switches_to_history_screen() {
        let mut app_state = base_state();
        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(2);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('H'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(matches!(app_state.ui.rss.active_screen, RssScreen::History));
    }

    #[test]
    fn hjkl_and_arrows_navigate_focus_and_rows() {
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
                KeyCode::Char('h'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters));

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Right,
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );
        assert!(matches!(app_state.ui.rss.focused_section, RssSectionFocus::Explorer));
    }

    #[test]
    fn sync_key_enqueues_command() {
        let mut app_state = base_state();
        let settings = crate::config::Settings::default();
        let (tx, mut rx) = mpsc::channel(2);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('S'),
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
                KeyCode::Char('S'),
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
    fn explorer_shift_f_seeds_filter_and_focuses_filters() {
        let mut app_state = base_state();
        app_state.ui.rss.focused_section = RssSectionFocus::Explorer;
        app_state.rss_runtime.preview_items.push(RssPreviewItem {
            title: "Ubuntu (LTS) [x64]".to_string(),
            ..Default::default()
        });

        let settings = crate::config::Settings::default();
        let (tx, _rx) = mpsc::channel(2);

        handle_event(
            CrosstermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
                KeyCode::Char('F'),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )),
            &mut app_state,
            &settings,
            &tx,
        );

        assert!(matches!(app_state.ui.rss.focused_section, RssSectionFocus::Filters));
        assert_eq!(app_state.ui.rss.filter_draft, "Ubuntu (LTS) [x64]");
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
                KeyCode::Char('x'),
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
    fn explorer_compute_keeps_non_matches_visible_when_search_active() {
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

        let (sorted, combined, prioritise) = compute_explorer_items(&items, "ubuntu", "", false);
        assert!(prioritise);
        assert_eq!(sorted.len(), 2);
        assert_eq!(combined.len(), 2);
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

        let (inactive_sorted, _, inactive_prioritise) = compute_explorer_items(&items, "", "", false);
        assert!(!inactive_prioritise);
        assert_eq!(inactive_sorted[0].title, "Non match");

        let (active_sorted, _, active_prioritise) = compute_explorer_items(&items, "", "", true);
        assert!(active_prioritise);
        assert_eq!(active_sorted[0].title, "Match");
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
            regex: "ubuntu".to_string(),
            enabled: true,
        });
        settings.rss.filters.push(crate::config::RssFilter {
            regex: "fedora".to_string(),
            enabled: true,
        });

        let query = active_filter_query(&app_state, &settings);
        assert_eq!(query, "fedora");
    }
}
