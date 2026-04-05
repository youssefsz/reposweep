use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap,
};
use shatter_core::format_bytes;

use crate::state::{AppModel, FilterMode, HomeMode, ResultsState, Screen, SortMode};

pub fn render(frame: &mut Frame<'_>, model: &mut AppModel) {
    match model.screen {
        Screen::Home | Screen::Error => render_home(frame, model),
        Screen::Scanning => render_scanning(frame, model),
        Screen::Results => render_results(frame, model),
    }

    if model
        .results
        .as_ref()
        .and_then(|results| results.pending_delete.as_ref())
        .is_some()
    {
        render_confirm(frame, model);
    }

    if model.delete.in_progress {
        render_delete_progress(frame, model);
    }

    if model.summary.is_some() {
        render_summary(frame, model);
    }
}

fn render_home(frame: &mut Frame<'_>, model: &mut AppModel) {
    let shell = render_app_shell(frame, "Shatter");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .margin(1)
        .split(shell);

    render_header(
        frame,
        chunks[0],
        "Shatter",
        "Clean project caches, builds, and dependencies without leaving the terminal",
        match model.home.mode {
            HomeMode::PathEntry => "HOME",
            HomeMode::Browser => "BROWSER",
        },
    );

    let mode_tabs = Paragraph::new(Line::from(vec![
        mode_tab("Path Entry", model.home.mode == HomeMode::PathEntry),
        Span::raw("  "),
        mode_tab("Browse", model.home.mode == HomeMode::Browser),
        Span::raw("  "),
        Span::styled(
            "Tab",
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" switches mode"),
    ]))
    .block(panel("Workspace Mode"));
    frame.render_widget(mode_tabs, chunks[1]);

    let mut input_text = model.home.input.clone();
    if model.home.mode == HomeMode::PathEntry {
        if model.tick % 8 < 4 {
            input_text.push('█');
        } else {
            input_text.push(' ');
        }
    }

    let input = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Target Path", Style::default().fg(MUTED)),
            Span::raw("  "),
            Span::styled(
                if model.home.mode == HomeMode::PathEntry {
                    "typing"
                } else {
                    "synced from browser"
                },
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            input_text,
            if model.home.mode == HomeMode::PathEntry {
                focus_style(true)
            } else {
                Style::default().fg(TEXT)
            },
        )),
    ])
    .block(panel("Scan Target"))
    .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[2]);

    match model.home.mode {
        HomeMode::PathEntry => {
            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
                .spacing(1)
                .split(chunks[3]);

            let intro = Paragraph::new(vec![
                section_heading("Ready To Scan"),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Enter", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                    Span::raw(" starts a scan for the path above."),
                ]),
                Line::from(vec![
                    Span::styled("Tab", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                    Span::raw(" switches to the directory browser."),
                ]),
                Line::from(vec![
                    Span::styled("Esc", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                    Span::raw(" exits the app."),
                ]),
                Line::from(""),
                muted_line("Shatter scans common cache, build, and dependency directories so you can review them before deleting anything."),
            ])
            .block(panel("Overview"))
            .wrap(Wrap { trim: true });
            frame.render_widget(intro, body[0]);

            let mut lines = vec![section_heading("Recent Scans"), Line::from("")];

            if model.home.recent_paths.is_empty() {
                lines.push(muted_line("No recent scans yet."));
            } else {
                lines.extend(model.home.recent_paths.iter().take(8).map(|path| {
                    Line::from(vec![
                        Span::styled("• ", Style::default().fg(ACCENT)),
                        Span::styled(path.display().to_string(), Style::default().fg(TEXT)),
                    ])
                }));
            }

            let recent = Paragraph::new(lines)
                .block(panel("Recent"))
                .wrap(Wrap { trim: true });
            frame.render_widget(recent, body[1]);
        }
        HomeMode::Browser => {
            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
                .spacing(1)
                .split(chunks[3]);

            let browser_items: Vec<ListItem<'_>> = model
                .home
                .browser_entries
                .iter()
                .map(|entry| ListItem::new(entry.label.clone()))
                .collect();
            let browser = List::new(browser_items)
                .block(panel("Directory Browser"))
                .highlight_style(Style::default().bg(SURFACE_HI).fg(TEXT))
                .highlight_symbol("› ");
            let browser_viewport = body[0].height.saturating_sub(2) as usize;
            sync_list_offset(
                &mut model.home.browser_offset,
                model.home.browser_selected,
                model.home.browser_entries.len(),
                browser_viewport,
            );
            let mut browser_state = ListState::default()
                .with_offset(model.home.browser_offset)
                .with_selected(Some(model.home.browser_selected));
            frame.render_stateful_widget(browser, body[0], &mut browser_state);

            let sidebar = Paragraph::new(vec![
                section_heading("Current Directory"),
                Line::from(Span::styled(
                    model.home.browser_path.display().to_string(),
                    Style::default().fg(TEXT),
                )),
                Line::from(""),
                muted_line("Enter opens the selected folder."),
                muted_line("s scans the current folder immediately."),
                muted_line("Tab returns to direct path entry."),
            ])
            .block(panel("Context"))
            .wrap(Wrap { trim: true });
            frame.render_widget(sidebar, body[1]);
        }
    }

    let footer = match model.home.mode {
        HomeMode::PathEntry => status_bar(vec![
            keycap("Enter"),
            Span::raw(" scan   "),
            keycap("Tab"),
            Span::raw(" browse   "),
            keycap("Esc"),
            Span::raw(" quit"),
        ]),
        HomeMode::Browser => status_bar(vec![
            keycap("Arrows"),
            Span::raw(" move   "),
            keycap("Enter"),
            Span::raw(" open   "),
            keycap("s"),
            Span::raw(" scan   "),
            keycap("Tab"),
            Span::raw(" type path"),
        ]),
    };
    frame.render_widget(footer, chunks[4]);

    if let Some(error) = &model.last_error {
        let area = centered_box(frame.area(), 58, 7);
        frame.render_widget(Clear, area);
        let popup = Paragraph::new(error.as_str())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(dialog("Error"));
        frame.render_widget(popup, area);
    }
}

fn render_scanning(frame: &mut Frame<'_>, model: &mut AppModel) {
    let shell = render_app_shell(frame, "Shatter");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(8),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .margin(1)
        .split(shell);

    render_header(
        frame,
        chunks[0],
        "Active Scan",
        &model
            .scan
            .root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Preparing scan target".into()),
        "SCANNING",
    );

    let scanner = Paragraph::new(vec![
        section_heading("Scanner Feed"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Loading  ", Style::default().fg(MUTED)),
            Span::styled(
                indeterminate_bar(model.tick, 28),
                Style::default().fg(ACCENT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Matches  ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{} candidate(s)", model.scan.matched_items),
                Style::default().fg(ACCENT),
            ),
        ]),
        if model.scan.stalled_ticks > 10 {
            Line::from(vec![
                Span::styled("Status   ", Style::default().fg(MUTED)),
                Span::styled(
                    "finalizing results...",
                    Style::default().fg(ACCENT).add_modifier(Modifier::ITALIC),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("Status   ", Style::default().fg(MUTED)),
                Span::styled(
                    "actively scanning",
                    Style::default().fg(TEXT).add_modifier(Modifier::ITALIC),
                ),
            ])
        },
        Line::from(vec![
            Span::styled("Current  ", Style::default().fg(MUTED)),
            Span::styled(
                model
                    .scan
                    .current_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "waiting for first directory...".into()),
                Style::default().fg(TEXT),
            ),
        ]),
    ])
    .block(panel("Progress"))
    .wrap(Wrap { trim: false });
    frame.render_widget(scanner, chunks[1]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .spacing(1)
        .split(chunks[2]);

    let warning_lines: Vec<ListItem<'_>> = if model.scan.warnings.is_empty() {
        vec![ListItem::new(muted_line("No warnings so far."))]
    } else {
        model
            .scan
            .warnings
            .iter()
            .rev()
            .take(body[0].height.saturating_sub(2) as usize)
            .map(|warning| ListItem::new(warning.clone()))
            .collect()
    };
    let warnings = List::new(warning_lines).block(panel("Warnings"));
    frame.render_widget(warnings, body[0]);

    let stats = Paragraph::new(vec![
        section_heading("Live Stats"),
        Line::from(""),
        metric_line("Directories", model.scan.scanned_dirs.to_string()),
        metric_line("Matches", model.scan.matched_items.to_string()),
        metric_line("Warnings", model.scan.warnings.len().to_string()),
        Line::from(""),
        muted_line("Results open automatically when the scan completes."),
    ])
    .block(panel("Session"))
    .wrap(Wrap { trim: true });
    frame.render_widget(stats, body[1]);

    let footer = status_bar(vec![
        keycap("c"),
        Span::raw(" cancel   "),
        keycap("q"),
        Span::raw(" quit"),
    ]);
    frame.render_widget(footer, chunks[3]);
}

fn render_results(frame: &mut Frame<'_>, model: &mut AppModel) {
    let Some(results) = &model.results else {
        let shell = render_app_shell(frame, "Shatter");
        frame.render_widget(
            Paragraph::new("No results yet.")
                .alignment(Alignment::Center)
                .block(panel("Results")),
            shell,
        );
        return;
    };

    let shell = render_app_shell(frame, "Shatter");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(5),
            Constraint::Length(3),
        ])
        .margin(1)
        .split(shell);

    render_header(
        frame,
        chunks[0],
        "Scan Results",
        "Review candidates before moving them to trash or deleting permanently",
        "RESULTS",
    );

    let summary = Paragraph::new(vec![
        Line::from(vec![
            metric_span("Items", results.report.totals.items.to_string()),
            Span::raw("   "),
            metric_span("Reclaimable", format_bytes(results.report.totals.bytes)),
            Span::raw("   "),
            metric_span("Warnings", results.report.warnings.len().to_string()),
        ]),
        Line::from(vec![
            metric_span("Filter", filter_label(results.filter).to_string()),
            Span::raw("   "),
            metric_span("Sort", sort_label(results.sort).to_string()),
            Span::raw("   "),
            metric_span("Selected", results.selected_count().to_string()),
        ]),
    ])
    .block(panel("Summary"))
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, chunks[1]);
    render_results_table(frame, chunks[2], results);
    render_details(frame, chunks[3], results);

    let footer = status_bar(vec![
        keycap("Space"),
        Span::raw(" toggle   "),
        keycap("f"),
        Span::raw(" filter   "),
        keycap("s"),
        Span::raw(" sort   "),
        keycap("Enter"),
        Span::raw(" trash   "),
        keycap("Shift+d"),
        Span::raw(" delete"),
    ]);
    frame.render_widget(footer, chunks[4]);
}

fn render_results_table(frame: &mut Frame<'_>, area: Rect, results: &ResultsState) {
    let visible = results.visible_indices();
    let rows: Vec<Row<'_>> = visible
        .iter()
        .map(|index| {
            let item = &results.report.items[*index];
            let checked = if results.checked.contains(index) {
                "[x]"
            } else {
                "[ ]"
            };
            Row::new(vec![
                Cell::from(checked.to_string()),
                Cell::from(
                    item.path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("?")
                        .to_string(),
                ),
                Cell::from(item.kind.to_string()),
                Cell::from(item.ecosystem.clone()),
                Cell::from(item.bytes.map(format_bytes).unwrap_or_else(|| "n/a".into())),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(35),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(12),
        ],
    )
    .header(
        Row::new(vec!["Pick", "Path", "Kind", "Ecosystem", "Size"])
            .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    )
    .block(panel("Candidates"))
    .column_spacing(1)
    .row_highlight_style(Style::default().bg(SURFACE_HI).fg(TEXT));

    let mut state = TableState::default();
    state.select(Some(
        results
            .selected_visible
            .min(visible.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_details(frame: &mut Frame<'_>, area: Rect, results: &ResultsState) {
    let details = if let Some(item) = results.selected_item() {
        vec![
            section_heading("Selected"),
            Line::from(""),
            Line::from(Span::styled(
                item.path.display().to_string(),
                Style::default().fg(TEXT),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("rule ", Style::default().fg(MUTED)),
                Span::styled(item.rule_name.clone(), Style::default().fg(ACCENT)),
                Span::raw("   "),
                Span::styled("kind ", Style::default().fg(MUTED)),
                Span::raw(item.kind.to_string()),
                Span::raw("   "),
                Span::styled("eco ", Style::default().fg(MUTED)),
                Span::raw(item.ecosystem.clone()),
                Span::raw("   "),
                Span::styled("size ", Style::default().fg(MUTED)),
                Span::raw(item.bytes.map(format_bytes).unwrap_or_else(|| "n/a".into())),
            ]),
            Line::from(vec![
                Span::styled("marked ", Style::default().fg(MUTED)),
                Span::styled(
                    results.selected_count().to_string(),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled("reclaim ", Style::default().fg(MUTED)),
                Span::styled(
                    format_bytes(results.checked_bytes()),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled("filter ", Style::default().fg(MUTED)),
                Span::raw(filter_label(results.filter)),
                Span::raw("   "),
                Span::styled("sort ", Style::default().fg(MUTED)),
                Span::raw(sort_label(results.sort)),
            ]),
            Line::from(""),
            shortcut_line(&[("Enter", "trash"), ("Shift+d", "delete"), ("h", "home")]),
        ]
    } else {
        vec![
            section_heading("Selected"),
            muted_line("No item selected."),
            Line::from(""),
            muted_line(&crate::state::footer_hint(results)),
        ]
    };

    let paragraph = Paragraph::new(details)
        .block(panel("Selection"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_confirm(frame: &mut Frame<'_>, model: &mut AppModel) {
    let Some(results) = &model.results else {
        return;
    };
    let Some(pending) = &results.pending_delete else {
        return;
    };

    let area = centered_box(frame.area(), 56, 10);
    frame.render_widget(Clear, area);
    let popup = Paragraph::new(vec![
        Line::from(format!(
            "{} {} item(s) for {}",
            if pending.strategy == shatter_core::DeleteStrategy::Trash {
                "Move"
            } else {
                "Permanently delete"
            },
            pending.item_indices.len(),
            format_bytes(pending.total_bytes)
        )),
        Line::from(""),
        muted_line("This confirmation stays on top of the current results list."),
        Line::from(""),
        shortcut_line(&[("Enter", "confirm"), ("y", "confirm")]),
        shortcut_line(&[("Esc", "cancel"), ("n", "cancel")]),
    ])
    .alignment(Alignment::Center)
    .block(dialog("Confirm delete"))
    .wrap(Wrap { trim: true });
    frame.render_widget(popup, area);
}

fn render_summary(frame: &mut Frame<'_>, model: &mut AppModel) {
    let Some(summary) = &model.summary else {
        return;
    };

    let area = centered_box(frame.area(), 72, 15);
    frame.render_widget(Clear, area);
    let failures = if summary.result.failed.is_empty() {
        "No failures".into()
    } else {
        summary
            .result
            .failed
            .iter()
            .take(4)
            .map(|failure| format!("{}: {}", failure.path.display(), failure.message))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let popup = Paragraph::new(vec![
        Line::from(Span::styled(
            summary.title(),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Recovered ", Style::default().fg(MUTED)),
            Span::styled(
                format_bytes(summary.result.reclaimed_bytes),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Deleted ", Style::default().fg(MUTED)),
            Span::styled(
                summary.result.deleted.len().to_string(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Failed ", Style::default().fg(MUTED)),
            Span::styled(
                summary.result.failed.len().to_string(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled("Failures:", Style::default().fg(MUTED))),
        Line::from(failures),
        Line::from(""),
        shortcut_line(&[("Enter", "close"), ("Esc", "close"), ("h", "home")]),
        shortcut_line(&[("r", "rescan"), ("q", "quit")]),
    ])
    .alignment(Alignment::Left)
    .block(dialog("Cleanup summary"))
    .wrap(Wrap { trim: true });
    frame.render_widget(popup, area);
}

fn render_delete_progress(frame: &mut Frame<'_>, model: &mut AppModel) {
    let area = centered_box(frame.area(), 52, 8);
    frame.render_widget(Clear, area);

    let message = format!(
        "{} {} item(s)...",
        if model.delete.strategy == shatter_core::DeleteStrategy::Trash {
            "Moving to trash"
        } else {
            "Deleting"
        },
        model.delete.item_count
    );

    let popup = Paragraph::new(vec![
        Line::from(Span::styled(
            indeterminate_bar(model.tick, 20),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        muted_line("Please wait while Shatter finishes the cleanup."),
    ])
    .alignment(Alignment::Center)
    .block(dialog("Working"))
    .wrap(Wrap { trim: true });
    frame.render_widget(popup, area);
}

fn focus_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        chrome()
    }
}

fn centered_box(area: Rect, desired_width: u16, desired_height: u16) -> Rect {
    let max_width = area.width.saturating_sub(6).max(1);
    let max_height = area.height.saturating_sub(4).max(1);

    let width = desired_width.min(max_width);
    let height = desired_height.min(max_height);

    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn filter_label(mode: FilterMode) -> &'static str {
    match mode {
        FilterMode::All => "all",
        FilterMode::CacheAndBuild => "cache/build",
        FilterMode::Dependencies => "deps",
    }
}

fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::LargestFirst => "largest",
        SortMode::PathAscending => "path",
    }
}

fn dialog<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(PANEL_BG))
        .border_style(chrome())
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
}

fn render_app_shell(frame: &mut Frame<'_>, title: &str) -> Rect {
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND)),
        frame.area(),
    );

    let shell = centered_app_rect(frame.area());
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(SHELL_BG))
        .border_style(chrome())
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(shell);
    frame.render_widget(Clear, shell);
    frame.render_widget(block, shell);
    inner
}

fn render_header(frame: &mut Frame<'_>, area: Rect, title: &str, subtitle: &str, badge: &str) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(area);

    let left = Paragraph::new(vec![
        Line::from(Span::styled(
            title,
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(subtitle, Style::default().fg(MUTED))),
    ])
    .wrap(Wrap { trim: true });
    frame.render_widget(left, chunks[0]);

    let right = Paragraph::new(vec![
        Line::from(vec![badge_span(badge)]),
        Line::from(Span::styled(
            "developer cleanup suite",
            Style::default().fg(MUTED),
        )),
    ])
    .alignment(Alignment::Right)
    .wrap(Wrap { trim: true });
    frame.render_widget(right, chunks[1]);
}

fn panel<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(PANEL_BG))
        .border_style(chrome())
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
}

fn centered_app_rect(area: Rect) -> Rect {
    let width = if area.width > 124 {
        120
    } else {
        area.width.saturating_sub(2).max(1)
    };
    let height = if area.height > 42 {
        38
    } else {
        area.height.saturating_sub(1).max(1)
    };

    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn section_heading(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    ))
}

fn keycap(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(TEXT)
            .bg(SURFACE_HI)
            .add_modifier(Modifier::BOLD),
    )
}

fn badge_span(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(BACKGROUND)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD),
    )
}

fn mode_tab(label: &str, active: bool) -> Span<'static> {
    let style = if active {
        Style::default()
            .fg(BACKGROUND)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(TEXT)
            .bg(SURFACE_HI)
            .add_modifier(Modifier::BOLD)
    };
    Span::styled(format!(" {label} "), style)
}

fn metric_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            value,
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn metric_span(label: &str, value: String) -> Span<'static> {
    Span::styled(
        format!("{label}: {value}"),
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    )
}

fn shortcut_line<'a>(items: &[(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::new();
    for (index, (key, label)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(keycap(key));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*label, Style::default().fg(TEXT)));
    }
    Line::from(spans)
}

fn muted_line(message: &str) -> Line<'static> {
    Line::from(Span::styled(
        message.to_string(),
        Style::default().fg(MUTED),
    ))
}

fn status_bar<'a>(spans: Vec<Span<'a>>) -> Paragraph<'a> {
    Paragraph::new(Line::from(spans))
        .block(panel("Controls"))
        .style(Style::default().fg(TEXT))
}

fn chrome() -> Style {
    Style::default().fg(PURPLE)
}

const BACKGROUND: Color = Color::Rgb(7, 10, 18);
const SHELL_BG: Color = Color::Rgb(13, 17, 29);
const PANEL_BG: Color = Color::Rgb(22, 27, 38);
const SURFACE_HI: Color = Color::Rgb(45, 57, 78);
const PURPLE: Color = Color::Rgb(94, 108, 136);
const ACCENT: Color = Color::Rgb(126, 189, 255);
const TEXT: Color = Color::Rgb(234, 238, 246);
const MUTED: Color = Color::Rgb(150, 160, 181);

fn indeterminate_bar(tick: u64, width: usize) -> String {
    let width = width.max(12);
    let segment = (width / 4).max(4).min(width.saturating_sub(2));
    let travel = width.saturating_sub(segment);
    let cycle = travel.saturating_mul(2).max(1);
    let step = (tick as usize) % cycle;
    let offset = if step <= travel { step } else { cycle - step };

    let mut chars = vec![' '; width];
    for slot in chars.iter_mut().skip(offset).take(segment) {
        *slot = '=';
    }

    format!("[{}]", chars.into_iter().collect::<String>())
}

fn sync_list_offset(offset: &mut usize, selected: usize, len: usize, viewport: usize) {
    if len == 0 || viewport == 0 {
        *offset = 0;
        return;
    }

    let max_offset = len.saturating_sub(viewport);
    *offset = (*offset).min(max_offset);

    if selected < *offset {
        *offset = selected;
    } else if selected >= offset.saturating_add(viewport) {
        *offset = selected + 1 - viewport;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use shatter_core::{ArtifactKind, ScanItem, ScanReport, ScanTotals};

    use crate::state::{AppModel, ResultsState, Screen};
    use crate::storage::AppState;

    use super::render;

    #[test]
    fn render_results_smoke_test() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut model = AppModel::new(Some(PathBuf::from(".")), &AppState::default());
        model.screen = Screen::Results;
        model.results = Some(ResultsState::new(ScanReport {
            items: vec![ScanItem {
                path: PathBuf::from("/tmp/node_modules"),
                kind: ArtifactKind::Dependency,
                ecosystem: "javascript".into(),
                rule_name: "node_modules".into(),
                bytes: Some(1024),
                last_modified: None,
                project_root: None,
                notes: vec![],
            }],
            totals: ScanTotals::default(),
            warnings: vec![],
            duration: Duration::from_secs(1),
            cancelled: false,
        }));

        terminal
            .draw(|frame| render(frame, &mut model))
            .expect("draw");
    }
}
