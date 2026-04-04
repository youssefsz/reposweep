use std::sync::mpsc::Sender;
use std::time::Instant;

use crate::config::Config;
use crate::domain::{CancellationToken, ScanEvent, ScanReport, ScanRequest, ScanTotals};
use crate::error::Result;
use crate::infrastructure::{FsDirectoryWalker, ParallelSizer};
use crate::rules::RuleSet;

#[derive(Clone, Debug)]
pub struct ScanService<W = FsDirectoryWalker, S = ParallelSizer> {
    walker: W,
    sizer: S,
    rules: RuleSet,
}

impl ScanService<FsDirectoryWalker, ParallelSizer> {
    pub fn from_config(config: Config) -> Self {
        Self::new(
            FsDirectoryWalker,
            ParallelSizer,
            RuleSet::from_config(&config),
        )
    }
}

impl<W, S> ScanService<W, S>
where
    W: Clone,
    S: Clone,
{
    pub fn new(walker: W, sizer: S, rules: RuleSet) -> Self {
        Self {
            walker,
            sizer,
            rules,
        }
    }
}

impl ScanService<FsDirectoryWalker, ParallelSizer> {
    pub fn scan(
        &self,
        request: ScanRequest,
        sender: Option<Sender<ScanEvent>>,
        cancel: CancellationToken,
    ) -> Result<ScanReport> {
        request.validate()?;
        let started_at = Instant::now();
        if let Some(sender) = &sender {
            let _ = sender.send(ScanEvent::Started {
                roots: request.roots.clone(),
            });
        }

        let discovery = self
            .walker
            .discover(&request, &self.rules, sender.as_ref(), &cancel)?;
        let items = self
            .sizer
            .size(request.size_mode, discovery.items, sender.as_ref(), &cancel);

        let mut totals = ScanTotals {
            items: items.len(),
            ..ScanTotals::default()
        };
        for item in &items {
            let bytes = item.bytes.unwrap_or(0);
            totals.bytes = totals.bytes.saturating_add(bytes);
            *totals.by_kind.entry(item.kind).or_default() += bytes;
        }

        if let Some(sender) = &sender {
            let _ = sender.send(ScanEvent::Finished {
                cancelled: cancel.is_cancelled(),
                scanned_dirs: discovery.scanned_dirs,
                matched_items: items.len(),
            });
        }

        Ok(ScanReport {
            items,
            totals,
            warnings: discovery.warnings,
            duration: started_at.elapsed(),
            cancelled: cancel.is_cancelled(),
        })
    }
}
