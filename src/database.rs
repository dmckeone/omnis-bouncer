use deadpool_redis::redis::cmd;
use deadpool_redis::{Config, Connection, Pool, Runtime};

use crate::errors::Result;

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
    let (unix_timestamp, _): (usize, usize) = cmd("TIME").query_async(conn).await?;
    Ok(unix_timestamp)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use std::env;
    use std::result::Result;

    pub fn create_test_pool() -> Result<Pool, String> {
        let uri = match env::var("TEST_REDIS_URI") {
            Ok(u) => u,
            Err(e) => {
                let msg = format!("TEST_REDIS_URI error: {:?}", e);
                return Err(msg);
            }
        };

        let uri = uri.trim();

        let pool = match create_redis_pool(uri.trim()) {
            Ok(p) => p,
            Err(e) => {
                let msg = format!("Redis server not available: {:?}", e);
                return Err(msg);
            }
        };

        Ok(pool)
    }
}
