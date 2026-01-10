//! Redis configuration caching.

use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use sha2::{Digest, Sha256};

use super::model::AppConfig;
use crate::error::ConfigError;

const CONFIG_KEY: &str = "config:current";
const CONFIG_HASH_KEY: &str = "config:hash";
const CONFIG_TIMESTAMP_KEY: &str = "config:last_validated";

/// Stores the validated configuration in Redis.
pub async fn store_config(
    redis: &mut redis::aio::ConnectionManager,
    config: &AppConfig,
) -> Result<(), ConfigError> {
    let json = serde_json::to_string(config).map_err(|e| ConfigError::CacheFailed(e.to_string()))?;

    let hash = compute_hash(&json);
    let timestamp = Utc::now().timestamp();

    redis
        .set::<_, _, ()>(CONFIG_KEY, &json)
        .await
        .map_err(|e| ConfigError::CacheFailed(e.to_string()))?;

    redis
        .set::<_, _, ()>(CONFIG_HASH_KEY, &hash)
        .await
        .map_err(|e| ConfigError::CacheFailed(e.to_string()))?;

    redis
        .set::<_, _, ()>(CONFIG_TIMESTAMP_KEY, timestamp)
        .await
        .map_err(|e| ConfigError::CacheFailed(e.to_string()))?;

    Ok(())
}

/// Loads the cached configuration from Redis.
pub async fn load_config(
    redis: &mut redis::aio::ConnectionManager,
) -> Result<Option<AppConfig>, ConfigError> {
    let json: Option<String> = redis
        .get(CONFIG_KEY)
        .await
        .map_err(|e| ConfigError::CacheFailed(e.to_string()))?;

    match json {
        Some(json) => {
            let config: AppConfig =
                serde_json::from_str(&json).map_err(|e| ConfigError::CacheFailed(e.to_string()))?;
            Ok(Some(config))
        }
        None => Ok(None),
    }
}

/// Checks if the cached configuration matches the given config by hash.
pub async fn config_matches(
    redis: &mut redis::aio::ConnectionManager,
    config: &AppConfig,
) -> Result<bool, ConfigError> {
    let json = serde_json::to_string(config).map_err(|e| ConfigError::CacheFailed(e.to_string()))?;

    let current_hash = compute_hash(&json);

    let cached_hash: Option<String> = redis
        .get(CONFIG_HASH_KEY)
        .await
        .map_err(|e| ConfigError::CacheFailed(e.to_string()))?;

    Ok(cached_hash.as_ref() == Some(&current_hash))
}

/// Computes the SHA256 hash of the given content.
fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
