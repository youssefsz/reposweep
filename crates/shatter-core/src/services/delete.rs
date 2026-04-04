use crate::domain::{DeleteRequest, DeleteResult};
use crate::infrastructure::DeletionBackend;

#[derive(Clone, Debug)]
pub struct DeleteService<B> {
    backend: B,
}

impl<B> DeleteService<B>
where
    B: DeletionBackend,
{
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn delete(&self, request: DeleteRequest) -> DeleteResult {
        let mut result = DeleteResult::default();

        for item in request.items {
            match self.backend.delete(&item.path, request.strategy) {
                Ok(()) => {
                    result.reclaimed_bytes = result
                        .reclaimed_bytes
                        .saturating_add(item.bytes.unwrap_or(0));
                    result.deleted.push(item.path);
                }
                Err(error) => result.failed.push(crate::domain::DeleteFailure {
                    path: item.path,
                    message: error.to_string(),
                }),
            }
        }

        result
    }
}
