use deadpool_redis::{redis, Connection, Pool as RedisPool};
use redis::{pipe, AsyncTypedCommands};
use uuid::Uuid;

use crate::database::{current_time, get_connection};
use crate::errors::Result;
use crate::queue::models::{QueueRotate, StoreCapacity};
use crate::queue::scripts::Scripts;
use crate::queue::util::{from_redis_bool, to_redis_bool};

pub(crate) struct QueueControl {
    pool: RedisPool,
    scripts: Scripts,
}

impl QueueControl {
    pub(crate) fn new(pool: RedisPool) -> Result<Self> {
        let queue = Self {
            pool,
            scripts: Scripts::new()?,
        };

        Ok(queue)
    }

    pub(crate) async fn init(&self) -> Result<()> {
        let mut conn = self.conn().await?;
        self.scripts.init(&mut conn).await?;
        Ok(())
    }

    // Create a new ID for use in the Queue
    pub(crate) fn new_id(&self) -> Uuid {
        Uuid::new_v4()
    }

    async fn conn(&self) -> Result<Connection> {
        get_connection(&self.pool).await
    }

    /// Set the current status of the queue
    pub(crate) async fn queue_status(
        &self,
        prefix: impl Into<String>,
    ) -> Result<(bool, StoreCapacity, usize)> {
        let prefix = prefix.into();
        let enabled_key = format!("{}::queue_enabled", &prefix);
        let capacity_key = format!("{}::store_capacity", &prefix);
        let time_key = format!("{}::queue_sync_timestamp", &prefix);

        // Set all values in single pipeline to ensure atomic consistency
        let mut conn = self.conn().await?;
        let (enabled, capacity, time): (isize, isize, usize) = pipe()
            .get(enabled_key)
            .get(capacity_key)
            .get(time_key)
            .query_async(&mut conn)
            .await?;

        Ok((
            from_redis_bool(enabled, false),
            StoreCapacity::from(capacity),
            time,
        ))
    }

    /// Set the current status of the queue
    pub(crate) async fn set_queue_status(
        &self,
        prefix: impl Into<String>,
        enabled: bool,
        capacity: StoreCapacity,
    ) -> Result<()> {
        let prefix = prefix.into();
        let enabled_key = format!("{}::queue_enabled", &prefix);
        let capacity_key = format!("{}::store_capacity", &prefix);
        let time_key = format!("{}::queue_sync_timestamp", &prefix);

        let mut conn = self.conn().await?;
        let current_time = current_time(&mut conn).await?;

        // Set all values in single pipeline to ensure atomic consistency
        let _: String = pipe()
            .set(enabled_key, to_redis_bool(enabled))
            .ignore()
            .set::<String, isize>(capacity_key, capacity.into())
            .ignore()
            .set(time_key, current_time)
            .ignore()
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Current size of the queue
    pub(crate) async fn queue_enabled(&self, prefix: impl Into<String>) -> Result<bool> {
        let mut conn = self.conn().await?;
        let key = format!("{}::queue_enabled", prefix.into());
        let result = conn.get(&key).await?;

        let default = false;
        match result {
            Some(r) => Ok(from_redis_bool(r.parse()?, default)),
            None => Ok(default),
        }
    }

    /// Current size of the queue
    pub(crate) async fn queue_size(&self, prefix: impl Into<String>) -> Result<usize> {
        let mut conn = self.conn().await?;
        let key = format!("{}::queue_ids", prefix.into());
        let result = conn.llen(key).await?;
        Ok(result)
    }

    /// Current capacity of the store
    pub(crate) async fn store_capacity(&self, prefix: impl Into<String>) -> Result<StoreCapacity> {
        let mut conn = self.conn().await?;
        let key = format!("{}::store_capacity", prefix.into());
        let result = conn.get(key).await?;

        let capacity = match result {
            Some(r) => StoreCapacity::from(r.parse::<isize>()?),
            None => StoreCapacity::Unlimited,
        };
        Ok(capacity)
    }

    /// Current size of the store
    pub(crate) async fn store_size(&self, prefix: impl Into<String>) -> Result<usize> {
        let mut conn = self.conn().await?;
        let key = format!("{}::store_ids", prefix.into());
        let result = conn.llen(key).await?;
        Ok(result)
    }

    pub(crate) async fn waiting_page(&self, prefix: impl Into<String>) -> Result<Option<String>> {
        let mut conn = self.conn().await?;
        let key = format!("{}::waiting_page", prefix.into());
        let result = conn.get(key).await?;
        Ok(result)
    }

    pub(crate) async fn set_waiting_page(
        &self,
        prefix: impl Into<String>,
        waiting_page: impl Into<String>,
    ) -> Result<()> {
        let mut conn = self.conn().await?;
        let key = format!("{}::waiting_page", prefix.into());
        conn.set(key, waiting_page.into()).await?;
        Ok(())
    }

    /// Check that all keys required for syncing the queue/store are available
    pub(crate) async fn check_sync_keys(&self) -> Result<bool> {
        let mut conn = self.conn().await?;
        self.scripts.check_sync_keys(&mut conn).await
    }

    /// Return true if the store or queue has any UUIDs, false if both the queue and store are empty
    pub(crate) async fn has_ids(&self) -> Result<bool> {
        let mut conn = self.conn().await?;
        self.scripts.has_ids(&mut conn).await
    }

    /// Add a UUID to the queue/store with expiration times, returning queue position
    pub(crate) async fn id_add(
        &self,
        prefix: String,
        id: Uuid,
        time: usize,
        validated_expiry: usize,
        quarantine_expiry: usize,
    ) -> Result<usize> {
        let mut conn = self.conn().await?;
        self.scripts
            .id_add(
                &mut conn,
                prefix,
                id,
                time,
                validated_expiry,
                quarantine_expiry,
            )
            .await
    }

    /// Return the position of a UUID in the queue, or add the UUID to the queue and then
    /// return the position if the UUID does not already exist in the queue
    pub(crate) async fn id_position(
        &self,
        prefix: String,
        id: Uuid,
        time: usize,
        validated_expiry: usize,
        quarantine_expiry: usize,
    ) -> Result<usize> {
        let mut conn = self.conn().await?;
        self.scripts
            .id_position(
                &mut conn,
                prefix,
                id,
                time,
                validated_expiry,
                quarantine_expiry,
            )
            .await
    }

    /// Remove a given UUID from the queue/store
    pub(crate) async fn id_remove(&self, prefix: String, id: Uuid) -> Result<()> {
        let mut conn = self.conn().await?;
        self.scripts.id_remove(&mut conn, prefix, id).await
    }

    /// Full queue rotation using scripts in a pipeline
    pub(crate) async fn rotate_full(
        &self,
        prefix: String,
        batch_size: usize,
    ) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        self.scripts
            .rotate_full(&mut conn, prefix, batch_size)
            .await
    }

    /// Partial queue rotation that only expires IDs, but doesn't promote IDs from queue to store
    pub(crate) async fn rotate_expire(&self, prefix: String) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        self.scripts.rotate_expire(&mut conn, prefix).await
    }
}

mod test {}
