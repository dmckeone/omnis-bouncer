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
