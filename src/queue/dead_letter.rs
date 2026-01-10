//! Dead letter queue management.

use super::job::EncodeJob;
use super::redis::QueueManager;
use crate::error::QueueError;

/// Handles dead letter queue operations.
pub struct DeadLetterHandler<'a> {
    queue: &'a mut QueueManager,
    max_attempts: u32,
}

impl<'a> DeadLetterHandler<'a> {
    /// Creates a new dead letter handler.
    pub fn new(queue: &'a mut QueueManager, max_attempts: u32) -> Self {
        Self { queue, max_attempts }
    }

    /// Handles a failed job, either retrying or moving to dead letter.
    pub async fn handle_failure(
        &mut self,
        mut job: EncodeJob,
        error: String,
    ) -> Result<FailureAction, QueueError> {
        job.fail(error.clone());

        if job.attempt_count < self.max_attempts {
            // Retry the job
            job.retry();
            self.queue.retry_job(&job).await?;
            Ok(FailureAction::Retrying {
                attempt: job.attempt_count,
                max_attempts: self.max_attempts,
            })
        } else {
            // Move to dead letter queue
            job.dead_letter(format!(
                "Exhausted {} attempts. Last error: {}",
                self.max_attempts, error
            ));
            self.queue.dead_letter(&job).await?;
            Ok(FailureAction::DeadLettered { reason: error })
        }
    }
}

/// Result of handling a job failure.
#[derive(Debug)]
pub enum FailureAction {
    /// Job is being retried.
    Retrying { attempt: u32, max_attempts: u32 },
    /// Job was moved to dead letter queue.
    DeadLettered { reason: String },
}
