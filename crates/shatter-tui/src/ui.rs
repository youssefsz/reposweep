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
        Screen::Results | Screen::ConfirmDelete | Screen::Summary => render_results(frame, model),
    }

    if matches!(model.screen, Screen::ConfirmDelete) {
        render_confirm(frame, model);
    }

    if matches!(model.screen, Screen::Summary) {
        render_summary(frame, model);
    }
}

fn render_home(frame: &mut Frame<'_>, model: &mut AppModel) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_brand_header(
        frame,
        chunks[0],
        "Clean project caches, builds, and dependencies without leaving the terminal",
        false,
    );

    let mode_label = match model.home.mode {
        HomeMode::PathEntry => "Path Entry",
        HomeMode::Browser => "Browse",
    };
    let scan_path_title = format!("Scan Path  ·  {mode_label}");
    let input = Paragraph::new(model.home.input.clone()).block(
        panel(&scan_path_title).border_style(if model.home.mode == HomeMode::PathEntry {
            focus_style(true)
        } else {
            chrome()
        }),
    );
    frame.render_widget(input, chunks[1]);

    match model.home.mode {
        HomeMode::PathEntry => {
            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
                .split(chunks[2]);

            let intro = Paragraph::new(vec![
                Line::from(Span::styled(
                    "Default flow",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("1. Type the folder path you want to scan."),
                Line::from("2. Press Enter to start the scan."),
                Line::from("3. Review results and delete from the results screen."),
                Line::from(""),
                Line::from("Press b if you prefer browsing folders instead of typing."),
            ])
            .block(panel("Path Mode"))
            .wrap(Wrap { trim: true });
            frame.render_widget(intro, body[0]);

            let recent_lines: Vec<Line<'_>> = if model.home.recent_paths.is_empty() {
                vec![muted_line("No recent scans yet.")]
            } else {
                model
                    .home
                    .recent_paths
                    .iter()
                    .take(8)
                    .map(|path| {
                        Line::from(vec![
                            Span::styled("• ", Style::default().fg(PURPLE)),
                            Span::styled(path.display().to_string(), Style::default().fg(TEXT)),
                        ])
                    })
                    .collect()
            };
            let recent = Paragraph::new(recent_lines)
                .block(panel("Recent Scans"))
                .wrap(Wrap { trim: true });
            frame.render_widget(recent, body[1]);
        }
        HomeMode::Browser => {
            let browser_items: Vec<ListItem<'_>> = model
                .home
                .browser_entries
                .iter()
                .map(|entry| ListItem::new(entry.label.clone()))
                .collect();
            let browser_title = format!("Browser  {}", model.home.browser_path.display());
            let browser = List::new(browser_items)
                .block(panel(&browser_title).border_style(focus_style(true)))
                .highlight_style(Style::default().bg(SURFACE_HI).fg(TEXT))
                .highlight_symbol("> ");
            let browser_viewport = chunks[2].height.saturating_sub(2) as usize;
            sync_list_offset(
                &mut model.home.browser_offset,
                model.home.browser_selected,
                model.home.browser_entries.len(),
                browser_viewport,
            );
            let mut browser_state = ListState::default()
                .with_offset(model.home.browser_offset)
                .with_selected(Some(model.home.browser_selected));
            frame.render_stateful_widget(browser, chunks[2], &mut browser_state);
        }
    }

    let footer = match model.home.mode {
        HomeMode::PathEntry => status_bar(vec![
            Span::styled(
                "Path mode",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            keycap("Enter"),
            Span::raw(" scan  "),
            keycap("b"),
            Span::raw(" browse  "),
            keycap("q"),
            Span::raw(" quit"),
        ]),
        HomeMode::Browser => status_bar(vec![
            Span::styled(
                "Browse mode",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  arrows move  "),
            keycap("Enter"),
            Span::raw(" open  "),
            keycap("s"),
            Span::raw(" scan  "),
            keycap("p"),
            Span::raw(" path mode"),
        ]),
    };
    frame.render_widget(footer, chunks[3]);

    if let Some(error) = &model.last_error {
        let area = centered_rect(70, 25, frame.area());
        frame.render_widget(Clear, area);
        let popup = Paragraph::new(error.as_str())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(panel("Error"));
        frame.render_widget(popup, area);
    }
}

fn render_scanning(frame: &mut Frame<'_>, model: &mut AppModel) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let spinner = scanner_spinner(model.tick);
    render_brand_header(
        frame,
        chunks[0],
        &format!(
            "{spinner}  scanning {}",
            model
                .scan
                .root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "…".into())
        ),
        false,
    );

    let scanner = Paragraph::new(vec![
        Line::from(Span::styled(
            "SCANNER FEED",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("CURRENT  ", Style::default().fg(MUTED)),
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
        Line::from(vec![
            Span::styled("STATUS   ", Style::default().fg(MUTED)),
            Span::styled(scan_meter(model.tick), Style::default().fg(PURPLE)),
        ]),
        Line::from(vec![
            Span::styled("FOUND    ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{} candidate(s)", model.scan.matched_items),
                Style::default().fg(ACCENT),
            ),
        ]),
    ])
    .block(panel("Scanner"))
    .wrap(Wrap { trim: false });
    frame.render_widget(scanner, chunks[1]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(chunks[2]);

    let warning_lines: Vec<ListItem<'_>> = if model.scan.warnings.is_empty() {
        vec![ListItem::new("No warnings so far")]
    } else {
        model
            .scan
            .warnings
            .iter()
            .rev()
            .take(8)
            .map(|warning| ListItem::new(warning.clone()))
            .collect()
    };
    let warnings = List::new(warning_lines).block(panel("Warnings"));
    frame.render_widget(warnings, body[0]);

    let stats = Paragraph::new(vec![
        metric_line("Directories", model.scan.scanned_dirs.to_string()),
        metric_line("Matches", model.scan.matched_items.to_string()),
        metric_line("Warnings", model.scan.warnings.len().to_string()),
        Line::from(""),
        muted_line("Results open automatically when scanning finishes."),
    ])
    .block(panel("Live Stats"))
    .wrap(Wrap { trim: true });
    frame.render_widget(stats, body[1]);

    let footer = status_bar(vec![
        Span::styled(
            "Scanner",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        keycap("c"),
        Span::raw(" cancel  "),
        keycap("q"),
        Span::raw(" quit"),
    ]);
    frame.render_widget(footer, chunks[3]);
}

fn render_results(frame: &mut Frame<'_>, model: &mut AppModel) {
    if matches!(model.screen, Screen::Summary) {
        render_summary(frame, model);
        return;
    }

    let Some(results) = &model.results else {
        let empty = Paragraph::new("No results yet.").block(panel("Results"));
        frame.render_widget(empty, frame.area());
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .split(frame.area());

    render_brand_header(
        frame,
        chunks[0],
        &format!(
            "{} items • {} reclaimable • {} warnings",
            results.report.totals.items,
            format_bytes(results.report.totals.bytes),
            results.report.warnings.len()
        ),
        true,
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(chunks[1]);

    render_results_table(frame, body[0], results);

    let sidebar = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(10)])
        .split(body[1]);

    render_details(frame, sidebar[0], results);
    render_actions(frame, sidebar[1], results);

    let footer = status_bar(vec![
        Span::styled(
            "Results",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        keycap("Space"),
        Span::raw(" toggle  "),
        keycap("Enter"),
        Span::raw(" trash  "),
        keycap("Shift+d"),
        Span::raw(" delete"),
    ]);
    frame.render_widget(footer, chunks[2]);
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
            Line::from(vec![
                Span::styled("Path: ", Style::default().fg(PURPLE)),
                Span::raw(item.path.display().to_string()),
            ]),
            Line::from(vec![
                Span::styled("Rule: ", Style::default().fg(PURPLE)),
                Span::raw(item.rule_name.clone()),
            ]),
            Line::from(vec![
                Span::styled("Kind: ", Style::default().fg(PURPLE)),
                Span::raw(item.kind.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Ecosystem: ", Style::default().fg(PURPLE)),
                Span::raw(item.ecosystem.clone()),
            ]),
            Line::from(vec![
                Span::styled("Size: ", Style::default().fg(PURPLE)),
                Span::raw(item.bytes.map(format_bytes).unwrap_or_else(|| "n/a".into())),
            ]),
            Line::from(vec![
                Span::styled("Project Root: ", Style::default().fg(PURPLE)),
                Span::raw(
                    item.project_root
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "unknown".into()),
                ),
            ]),
        ]
    } else {
        vec![Line::from("No item selected")]
    };

    let paragraph = Paragraph::new(details)
        .block(panel("Selected Item"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_actions(frame: &mut Frame<'_>, area: Rect, results: &ResultsState) {
    let actions = controls_block(
        "Actions",
        vec![
            metric_line("Selected", results.selected_count().to_string()),
            metric_line("Reclaim", format_bytes(results.checked_bytes())),
            metric_line("Filter", filter_label(results.filter).to_string()),
            metric_line("Sort", sort_label(results.sort).to_string()),
            shortcut_line(&[("Enter", "trash"), ("Delete", "trash")]),
            shortcut_line(&[("x", "trash"), ("Shift+d", "delete")]),
            muted_line(&crate::state::footer_hint(results)),
        ],
    );
    frame.render_widget(actions, area);
}

fn render_confirm(frame: &mut Frame<'_>, model: &mut AppModel) {
    let Some(results) = &model.results else {
        return;
    };
    let Some(pending) = &results.pending_delete else {
        return;
    };

    let area = centered_rect(60, 30, frame.area());
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
        shortcut_line(&[("Enter", "confirm"), ("y", "confirm")]),
        shortcut_line(&[("Esc", "cancel"), ("n", "cancel")]),
    ])
    .alignment(Alignment::Center)
    .block(panel("Confirm Delete"))
    .wrap(Wrap { trim: true });
    frame.render_widget(popup, area);
}

fn render_summary(frame: &mut Frame<'_>, model: &mut AppModel) {
    let Some(summary) = &model.summary else {
        return;
    };

    let area = centered_rect(70, 40, frame.area());
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
    let popup = Paragraph::new(format!(
        "{}\nRecovered {}\n\nFailures:\n{}\n\nr rescan this folder\nh return home\nq quit",
        summary.title(),
        format_bytes(summary.result.reclaimed_bytes),
        failures
    ))
    .alignment(Alignment::Left)
    .block(panel("Cleanup Summary"))
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

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
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

fn controls_block<'a>(title: &'a str, lines: Vec<Line<'a>>) -> Paragraph<'a> {
    Paragraph::new(lines)
        .block(panel(title))
        .wrap(Wrap { trim: true })
}

fn panel<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(chrome())
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
        ))
}

fn render_brand_header(frame: &mut Frame<'_>, area: Rect, subtitle: &str, compact: bool) {
    let lines = if compact || area.width < 72 {
        vec![
            Line::from(vec![
                Span::styled(
                    "SHATTER",
                    Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled("v0.1.0", Style::default().fg(MUTED)),
            ]),
            Line::from(Span::styled(subtitle, Style::default().fg(TEXT))),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                "  ____  _           _   _             ",
                Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                " / ___|| |__   __ _| |_| |_ ___ _ __  ",
                Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                " \\___ \\| '_ \\ / _` | __| __/ _ \\ '__| ",
                Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  ___) | | | | (_| | |_| ||  __/ |    ",
                Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                " |____/|_| |_|\\__,_|\\__|\\__\\___|_|    ",
                Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled("  developer cleanup engine", Style::default().fg(ACCENT)),
                Span::raw("  "),
                Span::styled("v0.1.0", Style::default().fg(MUTED)),
                Span::raw("  "),
                Span::styled(subtitle, Style::default().fg(TEXT)),
            ]),
        ]
    };

    let header = Paragraph::new(lines)
        .block(panel("Shatter"))
        .wrap(Wrap { trim: false });
    frame.render_widget(header, area);
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

fn metric_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            value,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn muted_line(message: &str) -> Line<'static> {
    Line::from(Span::styled(
        message.to_string(),
        Style::default().fg(MUTED),
    ))
}

fn status_bar<'a>(spans: Vec<Span<'a>>) -> Paragraph<'a> {
    Paragraph::new(Line::from(spans)).block(panel("Status"))
}

fn chrome() -> Style {
    Style::default().fg(PURPLE)
}

const SURFACE_HI: Color = Color::Rgb(50, 37, 68);
const PURPLE: Color = Color::Rgb(199, 120, 221);
const ACCENT: Color = Color::Rgb(199, 120, 221);
const TEXT: Color = Color::Rgb(235, 235, 245);
const MUTED: Color = Color::Rgb(140, 146, 172);

fn scanner_spinner(tick: u64) -> &'static str {
    match tick % 4 {
        0 => "[|]",
        1 => "[/]",
        2 => "[-]",
        _ => "[\\]",
    }
}

fn scan_meter(tick: u64) -> String {
    const WIDTH: usize = 18;
    let filled = (tick as usize % (WIDTH + 1)).min(WIDTH);
    let mut meter = String::from("[");
    meter.push_str(&"#".repeat(filled));
    meter.push_str(&".".repeat(WIDTH - filled));
    meter.push(']');
    meter
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
