use std::fs;
use std::path::Path;

use crate::domain::DeleteStrategy;
use crate::error::{RepoSweepError, Result};

pub trait DeletionBackend: Send + Sync {
    fn delete(&self, path: &Path, strategy: DeleteStrategy) -> Result<()>;
}

#[derive(Clone, Debug, Default)]
pub struct FsDeletionBackend;

impl DeletionBackend for FsDeletionBackend {
    fn delete(&self, path: &Path, strategy: DeleteStrategy) -> Result<()> {
        match strategy {
            DeleteStrategy::Trash => {
                trash::delete(path).map_err(|error| RepoSweepError::Delete {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                })?;
            }
            DeleteStrategy::Permanent => {
                if path.is_dir() {
                    fs::remove_dir_all(path)
                        .map_err(|error| RepoSweepError::io("remove dir", path, error))?;
                } else {
                    fs::remove_file(path)
                        .map_err(|error| RepoSweepError::io("remove file", path, error))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::domain::{DeleteRequest, DeleteStrategy, ScanItem};
    use crate::services::DeleteService;

    use super::*;

    #[derive(Default)]
    struct MockDeletionBackend {
        deleted: std::sync::Mutex<Vec<String>>,
    }

    impl DeletionBackend for MockDeletionBackend {
        fn delete(&self, path: &Path, _strategy: DeleteStrategy) -> Result<()> {
            self.deleted
                .lock()
                .expect("lock")
                .push(path.display().to_string());
            Ok(())
        }
    }

    #[test]
    fn delete_service_uses_backend() {
        let backend = MockDeletionBackend::default();
        let service = DeleteService::new(backend);
        let item = ScanItem {
            path: Path::new("/tmp/test-cache").to_path_buf(),
            kind: crate::domain::ArtifactKind::Cache,
            ecosystem: "generic".into(),
            rule_name: "cache".into(),
            bytes: Some(256),
            last_modified: None,
            project_root: None,
            notes: vec![],
        };

        let result = service.delete(DeleteRequest {
            items: vec![item],
            strategy: DeleteStrategy::Trash,
        });

        assert!(result.failed.is_empty());
        assert_eq!(result.reclaimed_bytes, 256);
    }
}
