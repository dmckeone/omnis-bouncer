use deadpool_redis::redis::cmd;
use deadpool_redis::{Config, Connection, Pool, Runtime};

use crate::errors::{Error, Result};

pub fn create_redis_pool(uri: impl Into<String>) -> Result<Pool> {
    let cfg = Config::from_url(uri.into());
    let pool = cfg.create_pool(Some(Runtime::Tokio1))?;
    Ok(pool)
}

pub async fn get_connection(pool: &Pool) -> Result<Connection> {
    Ok(pool.get().await?)
}

// Get current time from server
pub async fn current_time(conn: &mut Connection) -> Result<usize> {
    let result: (Option<usize>, Option<usize>) = cmd("TIME").query_async(conn).await?;
    match result.0 {
        Some(t) => Ok(t),
        None => Err(Error::QueueSyncTimestampOutOfRange(String::from("<nil>"))),
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use std::env;
    use tracing::warn;

    pub fn create_test_pool() -> Option<Pool> {
        let uri = match env::var("TEST_REDIS_URI") {
            Ok(u) => u,
            Err(e) => {
                warn!("TEST_REDIS_URI error: {:?}", e);
                return None;
            }
        };

        let uri = uri.trim();

        let pool = match create_redis_pool(uri.trim()) {
            Ok(p) => p,
            Err(e) => {
                warn!("Redis server not available: {:?}", e);
                return None;
            }
        };

        Some(pool)
    }
}
