use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

use rayon::prelude::*;

use crate::domain::{CancellationToken, ScanEvent, ScanItem, ScanRequest, ScanWarning, SizeMode};
use crate::error::{Result, ShatterError};
use crate::rules::RuleSet;

#[derive(Clone, Debug)]
pub struct DiscoveredItem {
    pub item: ScanItem,
}

#[derive(Clone, Debug, Default)]
pub struct DiscoveryOutput {
    pub items: Vec<DiscoveredItem>,
    pub warnings: Vec<ScanWarning>,
    pub scanned_dirs: usize,
}

#[derive(Clone, Debug, Default)]
pub struct FsDirectoryWalker;

#[derive(Clone, Debug, Default)]
pub struct ParallelSizer;

impl FsDirectoryWalker {
    pub fn discover(
        &self,
        request: &ScanRequest,
        rules: &RuleSet,
        sender: Option<&Sender<ScanEvent>>,
        cancel: &CancellationToken,
    ) -> Result<DiscoveryOutput> {
        let mut output = DiscoveryOutput::default();

        for root in &request.roots {
            if cancel.is_cancelled() {
                break;
            }

            if !root.exists() {
                let warning = ScanWarning {
                    path: Some(root.clone()),
                    message: "root does not exist".into(),
                };
                push_warning(&mut output.warnings, sender, warning);
                continue;
            }

            let canonical_root = root
                .canonicalize()
                .map_err(|error| ShatterError::io("canonicalize root", root, error))?;
            let mut queue = VecDeque::from([canonical_root.clone()]);

            while let Some(dir) = queue.pop_front() {
                if cancel.is_cancelled() {
                    break;
                }

                output.scanned_dirs += 1;
                send_event(sender, ScanEvent::EnteredPath { path: dir.clone() });

                if is_shatterignore(&dir) {
                    let warning = ScanWarning {
                        path: Some(dir.clone()),
                        message: "skipped because .shatterignore is present".into(),
                    };
                    push_warning(&mut output.warnings, sender, warning);
                    continue;
                }

                if rules.is_protected_path(&canonical_root, &dir) && dir != canonical_root {
                    let warning = ScanWarning {
                        path: Some(dir.clone()),
                        message: "skipped because path is protected by config".into(),
                    };
                    push_warning(&mut output.warnings, sender, warning);
                    continue;
                }

                if let Some(rule_match) = rules.match_directory(&dir, &canonical_root) {
                    if request.scope.includes(rule_match.rule.kind) {
                        let metadata = fs::metadata(&dir)
                            .map_err(|error| ShatterError::io("metadata", &dir, error))?;
                        let modified = metadata.modified().ok();
                        if matches_age_filter(request.age_filter, modified) {
                            send_event(
                                sender,
                                ScanEvent::MatchFound {
                                    path: dir.clone(),
                                    kind: rule_match.rule.kind,
                                    rule_name: rule_match.rule.name.clone(),
                                },
                            );
                            output.items.push(DiscoveredItem {
                                item: ScanItem {
                                    path: dir,
                                    kind: rule_match.rule.kind,
                                    ecosystem: rule_match.rule.ecosystem,
                                    rule_name: rule_match.rule.name,
                                    bytes: None,
                                    last_modified: modified,
                                    project_root: rule_match.project_root,
                                    notes: vec![],
                                },
                            });
                        }
                    }

                    continue;
                }

                let read_dir = match fs::read_dir(&dir) {
                    Ok(entries) => entries,
                    Err(error) => {
                        let warning = ScanWarning {
                            path: Some(dir.clone()),
                            message: format!("failed to read directory: {error}"),
                        };
                        push_warning(&mut output.warnings, sender, warning);
                        continue;
                    }
                };

                for entry in read_dir {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            let warning = ScanWarning {
                                path: Some(dir.clone()),
                                message: format!("failed to read directory entry: {error}"),
                            };
                            push_warning(&mut output.warnings, sender, warning);
                            continue;
                        }
                    };

                    let path = entry.path();
                    let file_type = match entry.file_type() {
                        Ok(file_type) => file_type,
                        Err(error) => {
                            let warning = ScanWarning {
                                path: Some(path.clone()),
                                message: format!("failed to read file type: {error}"),
                            };
                            push_warning(&mut output.warnings, sender, warning);
                            continue;
                        }
                    };

                    if file_type.is_symlink() || !file_type.is_dir() {
                        continue;
                    }

                    queue.push_back(path);
                }
            }
        }

        Ok(output)
    }
}

impl ParallelSizer {
    pub fn size(
        &self,
        size_mode: SizeMode,
        items: Vec<DiscoveredItem>,
        sender: Option<&Sender<ScanEvent>>,
        cancel: &CancellationToken,
    ) -> Vec<ScanItem> {
        if size_mode == SizeMode::Skip {
            return items.into_iter().map(|item| item.item).collect();
        }

        items
            .into_par_iter()
            .map(|mut discovered| {
                if cancel.is_cancelled() {
                    return discovered.item;
                }

                let bytes = compute_size(&discovered.item.path, cancel);
                if let Some(bytes) = bytes {
                    discovered.item.bytes = Some(bytes);
                    send_event(
                        sender,
                        ScanEvent::Sized {
                            path: discovered.item.path.clone(),
                            bytes,
                        },
                    );
                }
                discovered.item
            })
            .collect()
    }
}

fn matches_age_filter(
    age_filter: Option<std::time::Duration>,
    modified: Option<SystemTime>,
) -> bool {
    match (age_filter, modified) {
        (Some(age_filter), Some(modified)) => modified
            .elapsed()
            .map(|elapsed| elapsed >= age_filter)
            .unwrap_or(true),
        (Some(_), None) => true,
        (None, _) => true,
    }
}

fn compute_size(path: &Path, cancel: &CancellationToken) -> Option<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if cancel.is_cancelled() {
            break;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Some(total)
}

fn is_shatterignore(path: &Path) -> bool {
    path.join(".shatterignore").exists()
}

fn push_warning(
    warnings: &mut Vec<ScanWarning>,
    sender: Option<&Sender<ScanEvent>>,
    warning: ScanWarning,
) {
    send_event(sender, ScanEvent::Warning(warning.clone()));
    warnings.push(warning);
}

fn send_event(sender: Option<&Sender<ScanEvent>>, event: ScanEvent) {
    if let Some(sender) = sender {
        let _ = sender.send(event);
    }
}
