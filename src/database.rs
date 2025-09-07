use deadpool_redis::redis::cmd;
use deadpool_redis::{Config, Pool, Runtime};
use uuid::Uuid;

use crate::errors::Result;

pub fn create_redis_pool(uri: impl Into<String>) -> Result<Pool> {
    let cfg = Config::from_url(uri.into());
    let pool = cfg.create_pool(Some(Runtime::Tokio1))?;
    Ok(pool)
}

pub fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

pub async fn get_key(pool: &Pool) -> Result<String> {
    let key = "test_key";
    let mut conn = pool.get().await?;
    let result = cmd("GET").arg(&[key]).query_async(&mut conn).await?;
    Ok(result)
}
