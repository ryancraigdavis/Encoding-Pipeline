//! Redis queue operations.

use anyhow::Result;
use redis::AsyncCommands;

use super::job::{EncodeJob, JobStatus};
use crate::error::QueueError;

const QUEUE_KEY: &str = "encode:queue";
const PROCESSING_KEY: &str = "encode:processing";
const DEAD_LETTER_KEY: &str = "encode:dead_letter";
const JOB_PREFIX: &str = "encode:job:";

/// Manages the encoding queue in Redis.
#[derive(Clone)]
pub struct QueueManager {
    connection: redis::aio::ConnectionManager,
}

impl QueueManager {
    /// Creates a new QueueManager connected to the specified Redis URL.
    pub async fn new(redis_url: &str) -> Result<Self, QueueError> {
        let client = redis::Client::open(redis_url).map_err(|e| QueueError::ConnectionFailed {
            url: redis_url.to_string(),
            message: e.to_string(),
        })?;

        let connection = client
            .get_connection_manager()
            .await
            .map_err(|e| QueueError::ConnectionFailed {
                url: redis_url.to_string(),
                message: e.to_string(),
            })?;

        Ok(Self { connection })
    }

    /// Adds a job to the queue.
    pub async fn enqueue(&mut self, job: &EncodeJob) -> Result<(), QueueError> {
        let job_json =
            serde_json::to_string(job).map_err(|e| QueueError::SerializationFailed(e.to_string()))?;

        let job_key = format!("{}{}", JOB_PREFIX, job.id);

        // Store the job data
        self.connection
            .set::<_, _, ()>(&job_key, &job_json)
            .await
            .map_err(|e| QueueError::EnqueueFailed(e.to_string()))?;

        // Add job ID to the queue
        self.connection
            .rpush::<_, _, ()>(QUEUE_KEY, &job.id)
            .await
            .map_err(|e| QueueError::EnqueueFailed(e.to_string()))?;

        Ok(())
    }

    /// Dequeues a job for processing (moves to processing set).
    pub async fn dequeue(&mut self) -> Result<Option<EncodeJob>, QueueError> {
        // Atomically move from queue to processing
        let job_id: Option<String> = self
            .connection
            .lpop(QUEUE_KEY, None)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        let job_id = match job_id {
            Some(id) => id,
            None => return Ok(None),
        };

        // Add to processing set
        self.connection
            .sadd::<_, _, ()>(PROCESSING_KEY, &job_id)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        // Get the job data
        self.get_job(&job_id).await
    }

    /// Gets a job by its ID.
    pub async fn get_job(&mut self, job_id: &str) -> Result<Option<EncodeJob>, QueueError> {
        let job_key = format!("{}{}", JOB_PREFIX, job_id);

        let job_json: Option<String> = self
            .connection
            .get(&job_key)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        match job_json {
            Some(json) => {
                let job: EncodeJob = serde_json::from_str(&json)
                    .map_err(|e| QueueError::SerializationFailed(e.to_string()))?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Updates a job's data in Redis.
    pub async fn update_job(&mut self, job: &EncodeJob) -> Result<(), QueueError> {
        let job_json =
            serde_json::to_string(job).map_err(|e| QueueError::SerializationFailed(e.to_string()))?;

        let job_key = format!("{}{}", JOB_PREFIX, job.id);

        self.connection
            .set::<_, _, ()>(&job_key, &job_json)
            .await
            .map_err(|e| QueueError::EnqueueFailed(e.to_string()))?;

        Ok(())
    }

    /// Marks a job as completed and removes from processing.
    pub async fn complete_job(&mut self, job: &EncodeJob) -> Result<(), QueueError> {
        // Update job data
        self.update_job(job).await?;

        // Remove from processing set
        self.connection
            .srem::<_, _, ()>(PROCESSING_KEY, &job.id)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        Ok(())
    }

    /// Moves a failed job back to the queue for retry.
    pub async fn retry_job(&mut self, job: &EncodeJob) -> Result<(), QueueError> {
        // Update job data
        self.update_job(job).await?;

        // Remove from processing set
        self.connection
            .srem::<_, _, ()>(PROCESSING_KEY, &job.id)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        // Add back to queue (at the front for immediate retry)
        self.connection
            .lpush::<_, _, ()>(QUEUE_KEY, &job.id)
            .await
            .map_err(|e| QueueError::EnqueueFailed(e.to_string()))?;

        Ok(())
    }

    /// Moves a job to the dead letter queue.
    pub async fn dead_letter(&mut self, job: &EncodeJob) -> Result<(), QueueError> {
        // Update job data
        self.update_job(job).await?;

        // Remove from processing set
        self.connection
            .srem::<_, _, ()>(PROCESSING_KEY, &job.id)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        // Add to dead letter queue
        self.connection
            .rpush::<_, _, ()>(DEAD_LETTER_KEY, &job.id)
            .await
            .map_err(|e| QueueError::EnqueueFailed(e.to_string()))?;

        Ok(())
    }

    /// Returns the number of jobs in the queue.
    pub async fn queue_length(&mut self) -> Result<usize, QueueError> {
        let len: usize = self
            .connection
            .llen(QUEUE_KEY)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;
        Ok(len)
    }

    /// Returns the number of jobs currently being processed.
    pub async fn processing_count(&mut self) -> Result<usize, QueueError> {
        let count: usize = self
            .connection
            .scard(PROCESSING_KEY)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;
        Ok(count)
    }

    /// Returns the number of jobs in the dead letter queue.
    pub async fn dead_letter_count(&mut self) -> Result<usize, QueueError> {
        let len: usize = self
            .connection
            .llen(DEAD_LETTER_KEY)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;
        Ok(len)
    }

    /// Lists all jobs in the queue.
    pub async fn list_queue(&mut self) -> Result<Vec<EncodeJob>, QueueError> {
        let job_ids: Vec<String> = self
            .connection
            .lrange(QUEUE_KEY, 0, -1)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        let mut jobs = Vec::new();
        for id in job_ids {
            if let Some(job) = self.get_job(&id).await? {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    /// Lists all jobs in the dead letter queue.
    pub async fn list_dead_letter(&mut self) -> Result<Vec<EncodeJob>, QueueError> {
        let job_ids: Vec<String> = self
            .connection
            .lrange(DEAD_LETTER_KEY, 0, -1)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        let mut jobs = Vec::new();
        for id in job_ids {
            if let Some(job) = self.get_job(&id).await? {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    /// Clears all jobs from the queue (does not affect processing or dead letter).
    pub async fn clear_queue(&mut self) -> Result<usize, QueueError> {
        let len = self.queue_length().await?;
        self.connection
            .del::<_, ()>(QUEUE_KEY)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;
        Ok(len)
    }

    /// Moves a job from dead letter back to the queue.
    pub async fn retry_dead_letter(&mut self, job_id: &str) -> Result<(), QueueError> {
        // Remove from dead letter queue
        self.connection
            .lrem::<_, _, ()>(DEAD_LETTER_KEY, 1, job_id)
            .await
            .map_err(|e| QueueError::DequeueFailed(e.to_string()))?;

        // Get the job and reset its status
        if let Some(mut job) = self.get_job(job_id).await? {
            job.retry();
            self.update_job(&job).await?;

            // Add back to queue
            self.connection
                .rpush::<_, _, ()>(QUEUE_KEY, job_id)
                .await
                .map_err(|e| QueueError::EnqueueFailed(e.to_string()))?;
            Ok(())
        } else {
            Err(QueueError::JobNotFound {
                job_id: job_id.to_string(),
            })
        }
    }
}
