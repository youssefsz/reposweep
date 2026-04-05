use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use shatter_core::{
    CancellationToken, DeleteResult, DeleteStrategy, Result as CoreResult, ScanEvent, ScanItem,
    ScanReport, format_bytes,
};

use crate::storage::AppState;

pub struct AppModel {
    pub should_quit: bool,
    pub tick: u64,
    pub screen: Screen,
    pub home: HomeState,
    pub scan: ScanState,
    pub results: Option<ResultsState>,
    pub summary: Option<SummaryState>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Screen {
    Home,
    Scanning,
    Results,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeMode {
    PathEntry,
    Browser,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortMode {
    LargestFirst,
    PathAscending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterMode {
    All,
    CacheAndBuild,
    Dependencies,
}

#[derive(Clone, Debug)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct HomeState {
    pub input: String,
    pub browser_path: PathBuf,
    pub browser_entries: Vec<BrowserEntry>,
    pub browser_selected: usize,
    pub browser_offset: usize,
    pub browser_history: HashMap<PathBuf, BrowserPosition>,
    pub recent_paths: Vec<PathBuf>,
    pub mode: HomeMode,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserPosition {
    pub selected: usize,
    pub offset: usize,
}

pub struct ScanState {
    pub root: Option<PathBuf>,
    pub current_path: Option<PathBuf>,
    pub scanned_dirs: usize,
    pub matched_items: usize,
    pub warnings: Vec<String>,
    pub event_rx: Option<Receiver<ScanEvent>>,
    pub result_rx: Option<Receiver<CoreResult<ScanReport>>>,
    pub cancel: Option<CancellationToken>,
}

pub struct ResultsState {
    pub report: ScanReport,
    pub checked: BTreeSet<usize>,
    pub selected_visible: usize,
    pub sort: SortMode,
    pub filter: FilterMode,
    pub pending_delete: Option<PendingDelete>,
}

#[derive(Clone, Debug)]
pub struct PendingDelete {
    pub strategy: DeleteStrategy,
    pub item_indices: Vec<usize>,
    pub total_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct SummaryState {
    pub result: DeleteResult,
    pub strategy: DeleteStrategy,
}

impl AppModel {
    pub fn new(initial_path: Option<PathBuf>, app_state: &AppState) -> Self {
        let browser_path = initial_path
            .clone()
            .or_else(|| app_state.last_browser_path.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let input = initial_path
            .filter(|path| path.exists())
            .unwrap_or_else(|| browser_path.clone())
            .display()
            .to_string();

        let mut model = Self {
            should_quit: false,
            tick: 0,
            screen: Screen::Home,
            home: HomeState {
                input,
                browser_entries: vec![],
                browser_selected: 0,
                browser_offset: 0,
                browser_history: HashMap::new(),
                browser_path,
                recent_paths: app_state.recent_paths.clone(),
                mode: HomeMode::PathEntry,
            },
            scan: ScanState::default(),
            results: None,
            summary: None,
            last_error: None,
        };
        model.refresh_browser();
        model
    }

    pub fn refresh_browser(&mut self) {
        self.home.browser_entries = load_browser_entries(&self.home.browser_path);
        self.restore_browser_position();
    }

    pub fn switch_home_mode(&mut self, mode: HomeMode) {
        self.home.mode = mode;
    }

    pub fn move_home_selection(&mut self, delta: isize) {
        if self.home.mode == HomeMode::Browser {
            self.home.browser_selected = shift_index(
                self.home.browser_selected,
                self.home.browser_entries.len(),
                delta,
            );
        }
    }

    pub fn enter_browser(&mut self) {
        self.save_browser_position();
        if let Some(entry) = self.home.browser_entries.get(self.home.browser_selected) {
            self.home.browser_path = entry.path.clone();
            self.home.input = self.home.browser_path.display().to_string();
            self.refresh_browser();
        }
    }

    pub fn remember_recent_path(&mut self, path: &Path) {
        self.home.recent_paths.retain(|item| item != path);
        self.home.recent_paths.insert(0, path.to_path_buf());
        self.home.recent_paths.truncate(10);
    }

    fn save_browser_position(&mut self) {
        self.home.browser_history.insert(
            self.home.browser_path.clone(),
            BrowserPosition {
                selected: self.home.browser_selected,
                offset: self.home.browser_offset,
            },
        );
    }

    fn restore_browser_position(&mut self) {
        let position = self
            .home
            .browser_history
            .get(&self.home.browser_path)
            .copied()
            .unwrap_or_default();

        self.home.browser_selected = position
            .selected
            .min(self.home.browser_entries.len().saturating_sub(1));
        self.home.browser_offset = position
            .offset
            .min(self.home.browser_entries.len().saturating_sub(1));
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
        self.screen = Screen::Error;
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
        self.screen = Screen::Home;
    }

    pub fn begin_scan(
        &mut self,
        root: PathBuf,
        event_rx: Receiver<ScanEvent>,
        result_rx: Receiver<CoreResult<ScanReport>>,
        cancel: CancellationToken,
    ) {
        self.screen = Screen::Scanning;
        self.summary = None;
        self.results = None;
        self.last_error = None;
        self.scan = ScanState {
            root: Some(root),
            current_path: None,
            scanned_dirs: 0,
            matched_items: 0,
            warnings: vec![],
            event_rx: Some(event_rx),
            result_rx: Some(result_rx),
            cancel: Some(cancel),
        };
    }

    pub fn finish_scan(&mut self, report: ScanReport) {
        self.results = Some(ResultsState::new(report));
        self.scan = ScanState::default();
        self.screen = Screen::Results;
    }

    pub fn finish_delete(&mut self, result: DeleteResult, strategy: DeleteStrategy) {
        self.summary = Some(SummaryState { result, strategy });
        if let Some(results) = &mut self.results {
            if let Some(summary) = &self.summary {
                results.apply_delete_result(&summary.result);
            }
            results.pending_delete = None;
        }
        self.screen = Screen::Results;
    }
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            root: None,
            current_path: None,
            scanned_dirs: 0,
            matched_items: 0,
            warnings: vec![],
            event_rx: None,
            result_rx: None,
            cancel: None,
        }
    }
}

impl ResultsState {
    pub fn new(report: ScanReport) -> Self {
        Self {
            report,
            checked: BTreeSet::new(),
            selected_visible: 0,
            sort: SortMode::LargestFirst,
            filter: FilterMode::All,
            pending_delete: None,
        }
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .report
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| match self.filter {
                FilterMode::All => true,
                FilterMode::CacheAndBuild => item.kind != shatter_core::ArtifactKind::Dependency,
                FilterMode::Dependencies => item.kind == shatter_core::ArtifactKind::Dependency,
            })
            .map(|(index, _)| index)
            .collect();

        indices.sort_by(|left, right| {
            let left_item = &self.report.items[*left];
            let right_item = &self.report.items[*right];
            match self.sort {
                SortMode::LargestFirst => right_item
                    .bytes
                    .unwrap_or(0)
                    .cmp(&left_item.bytes.unwrap_or(0))
                    .then_with(|| left_item.path.cmp(&right_item.path)),
                SortMode::PathAscending => left_item.path.cmp(&right_item.path),
            }
        });
        indices
    }

    pub fn selected_item_index(&self) -> Option<usize> {
        self.visible_indices().get(self.selected_visible).copied()
    }

    pub fn selected_item(&self) -> Option<&ScanItem> {
        self.selected_item_index()
            .and_then(|index| self.report.items.get(index))
    }

    pub fn move_selection(&mut self, delta: isize) {
        let visible = self.visible_indices();
        self.selected_visible = shift_index(self.selected_visible, visible.len(), delta);
    }

    pub fn toggle_selected(&mut self) {
        if let Some(index) = self.selected_item_index() {
            if !self.checked.insert(index) {
                self.checked.remove(&index);
            }
        }
    }

    pub fn toggle_all_visible(&mut self) {
        let visible = self.visible_indices();
        let all_visible_checked = visible.iter().all(|index| self.checked.contains(index));

        if all_visible_checked {
            for index in visible {
                self.checked.remove(&index);
            }
        } else {
            for index in visible {
                self.checked.insert(index);
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.checked.clear();
    }

    pub fn cycle_sort(&mut self) {
        self.sort = match self.sort {
            SortMode::LargestFirst => SortMode::PathAscending,
            SortMode::PathAscending => SortMode::LargestFirst,
        };
        self.selected_visible = 0;
    }

    pub fn cycle_filter(&mut self) {
        self.filter = match self.filter {
            FilterMode::All => FilterMode::CacheAndBuild,
            FilterMode::CacheAndBuild => FilterMode::Dependencies,
            FilterMode::Dependencies => FilterMode::All,
        };
        self.selected_visible = 0;
    }

    pub fn begin_delete(&mut self, strategy: DeleteStrategy) -> bool {
        let item_indices: Vec<usize> = if self.checked.is_empty() {
            self.selected_item_index().into_iter().collect()
        } else {
            self.checked.iter().copied().collect()
        };
        if item_indices.is_empty() {
            return false;
        }

        let total_bytes = item_indices
            .iter()
            .filter_map(|index| self.report.items.get(*index).and_then(|item| item.bytes))
            .sum();

        self.pending_delete = Some(PendingDelete {
            strategy,
            item_indices,
            total_bytes,
        });
        true
    }

    pub fn selected_count(&self) -> usize {
        if self.checked.is_empty() {
            usize::from(self.selected_item_index().is_some())
        } else {
            self.checked.len()
        }
    }

    pub fn checked_bytes(&self) -> u64 {
        let indices: Vec<usize> = if self.checked.is_empty() {
            self.selected_item_index().into_iter().collect()
        } else {
            self.checked.iter().copied().collect()
        };

        indices
            .iter()
            .filter_map(|index| self.report.items.get(*index).and_then(|item| item.bytes))
            .sum()
    }

    pub fn delete_items(&self) -> Vec<ScanItem> {
        self.pending_delete
            .as_ref()
            .map(|pending| {
                pending
                    .item_indices
                    .iter()
                    .filter_map(|index| self.report.items.get(*index).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn apply_delete_result(&mut self, result: &DeleteResult) {
        if result.deleted.is_empty() {
            return;
        }

        let deleted_paths: BTreeSet<_> = result.deleted.iter().cloned().collect();
        self.report
            .items
            .retain(|item| !deleted_paths.contains(&item.path));
        self.checked.clear();
        self.report.totals = recalculate_totals(&self.report.items);

        let visible_len = self.visible_indices().len();
        self.selected_visible = self.selected_visible.min(visible_len.saturating_sub(1));
    }
}

impl SummaryState {
    pub fn title(&self) -> String {
        format!(
            "{} completed: {} deleted, {} failed",
            self.strategy,
            self.result.deleted.len(),
            self.result.failed.len()
        )
    }
}

pub fn handle_scan_event(model: &mut AppModel, event: ScanEvent) {
    match event {
        ScanEvent::Started { .. } => {}
        ScanEvent::EnteredPath { path } => {
            model.scan.scanned_dirs = model.scan.scanned_dirs.saturating_add(1);
            model.scan.current_path = Some(path);
        }
        ScanEvent::MatchFound { .. } => {
            model.scan.matched_items = model.scan.matched_items.saturating_add(1);
        }
        ScanEvent::Sized { .. } => {}
        ScanEvent::Warning(warning) => {
            model.scan.warnings.push(if let Some(path) = warning.path {
                format!("{}: {}", path.display(), warning.message)
            } else {
                warning.message
            });
        }
        ScanEvent::Finished { .. } => {}
    }
}

fn load_browser_entries(path: &Path) -> Vec<BrowserEntry> {
    let mut entries = vec![];

    if let Some(parent) = path.parent() {
        entries.push(BrowserEntry {
            path: parent.to_path_buf(),
            label: "..".into(),
        });
    }

    if let Ok(read_dir) = fs::read_dir(path) {
        let mut directories: Vec<BrowserEntry> = read_dir
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_dir() || file_type.is_symlink() {
                    return None;
                }
                Some(BrowserEntry {
                    label: entry.file_name().to_string_lossy().to_string(),
                    path: entry.path(),
                })
            })
            .collect();
        directories.sort_by(|left, right| left.label.cmp(&right.label));
        entries.extend(directories);
    }

    entries
}

fn shift_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.clamp(0, (len.saturating_sub(1)) as isize) as usize
}

pub fn footer_hint(results: &ResultsState) -> String {
    format!(
        "selected {} • {}",
        results.selected_count(),
        format_bytes(results.checked_bytes())
    )
}

fn recalculate_totals(items: &[ScanItem]) -> shatter_core::ScanTotals {
    let mut totals = shatter_core::ScanTotals {
        items: items.len(),
        bytes: 0,
        by_kind: BTreeMap::new(),
    };

    for item in items {
        let bytes = item.bytes.unwrap_or(0);
        totals.bytes += bytes;
        *totals.by_kind.entry(item.kind).or_insert(0) += bytes;
    }

    totals
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use shatter_core::{ArtifactKind, ScanItem, ScanReport, ScanTotals, ScanWarning};

    use super::*;

    fn sample_results() -> ResultsState {
        ResultsState::new(ScanReport {
            items: vec![
                ScanItem {
                    path: PathBuf::from("/tmp/a"),
                    kind: ArtifactKind::Cache,
                    ecosystem: "generic".into(),
                    rule_name: ".cache".into(),
                    bytes: Some(100),
                    last_modified: None,
                    project_root: None,
                    notes: vec![],
                },
                ScanItem {
                    path: PathBuf::from("/tmp/b"),
                    kind: ArtifactKind::Dependency,
                    ecosystem: "javascript".into(),
                    rule_name: "node_modules".into(),
                    bytes: Some(1000),
                    last_modified: None,
                    project_root: None,
                    notes: vec![],
                },
            ],
            totals: ScanTotals::default(),
            warnings: vec![ScanWarning {
                path: None,
                message: "warning".into(),
            }],
            duration: Duration::from_secs(1),
            cancelled: false,
        })
    }

    #[test]
    fn recent_paths_are_deduplicated() {
        let state = AppState {
            recent_paths: vec![PathBuf::from("/tmp/a")],
            last_browser_path: None,
        };
        let mut model = AppModel::new(None, &state);
        model.remember_recent_path(Path::new("/tmp/a"));
        model.remember_recent_path(Path::new("/tmp/b"));
        assert_eq!(model.home.recent_paths[0], PathBuf::from("/tmp/b"));
        assert_eq!(model.home.recent_paths.len(), 2);
    }

    #[test]
    fn filters_visible_items() {
        let mut results = sample_results();
        results.filter = FilterMode::Dependencies;
        let visible = results.visible_indices();
        assert_eq!(visible, vec![1]);
    }

    #[test]
    fn toggle_all_visible_selects_then_clears_visible_items() {
        let mut results = sample_results();

        results.toggle_all_visible();
        assert_eq!(
            results.checked.iter().copied().collect::<Vec<_>>(),
            vec![0, 1]
        );

        results.toggle_all_visible();
        assert!(results.checked.is_empty());
    }

    #[test]
    fn toggle_all_visible_only_affects_current_filter() {
        let mut results = sample_results();
        results.checked.insert(0);
        results.filter = FilterMode::Dependencies;

        results.toggle_all_visible();
        assert_eq!(
            results.checked.iter().copied().collect::<Vec<_>>(),
            vec![0, 1]
        );

        results.toggle_all_visible();
        assert_eq!(results.checked.iter().copied().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn apply_delete_result_removes_deleted_items_and_updates_totals() {
        let mut results = sample_results();
        results.checked.insert(0);
        results.checked.insert(1);

        let result = DeleteResult {
            deleted: vec![PathBuf::from("/tmp/b")],
            failed: vec![],
            reclaimed_bytes: 1000,
        };

        results.apply_delete_result(&result);

        assert_eq!(results.report.items.len(), 1);
        assert_eq!(results.report.items[0].path, PathBuf::from("/tmp/a"));
        assert_eq!(results.report.totals.items, 1);
        assert_eq!(results.report.totals.bytes, 100);
        assert!(results.checked.is_empty());
    }

    #[test]
    fn browser_restores_parent_selection_after_returning() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir(root.join("alpha")).expect("alpha");
        std::fs::create_dir(root.join("beta")).expect("beta");
        std::fs::create_dir(root.join("gamma")).expect("gamma");
        std::fs::create_dir(root.join("gamma").join("child")).expect("child");

        let mut model = AppModel::new(Some(root.to_path_buf()), &AppState::default());
        let gamma_index = model
            .home
            .browser_entries
            .iter()
            .position(|entry| entry.label == "gamma")
            .expect("gamma index");
        model.home.browser_selected = gamma_index;
        model.home.browser_offset = gamma_index.saturating_sub(1);

        model.enter_browser();
        assert_eq!(model.home.browser_path, root.join("gamma"));

        let parent_index = model
            .home
            .browser_entries
            .iter()
            .position(|entry| entry.label == "..")
            .expect("parent index");
        model.home.browser_selected = parent_index;
        model.enter_browser();

        assert_eq!(model.home.browser_path, root);
        assert_eq!(model.home.browser_selected, gamma_index);
        assert_eq!(model.home.browser_offset, gamma_index.saturating_sub(1));
    }
}
