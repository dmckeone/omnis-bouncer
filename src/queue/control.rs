use deadpool_redis::{redis, Connection, Pool as RedisPool};
use redis::{pipe, AsyncTypedCommands};
use uuid::Uuid;

use crate::database::{current_time, get_connection};
use crate::errors::Result;
use crate::queue::models::{QueueEnabled, QueueRotate, QueueSyncTimestamp, StoreCapacity};
use crate::queue::scripts::Scripts;

pub struct QueueControl {
    pool: RedisPool,
    scripts: Scripts,
}

impl QueueControl {
    pub fn new(pool: RedisPool) -> Result<Self> {
        let queue = Self {
            pool,
            scripts: Scripts::new()?,
        };

        Ok(queue)
    }

    pub async fn init(&self) -> Result<()> {
        let mut conn = self.conn().await?;
        self.scripts.init(&mut conn).await?;
        Ok(())
    }

    // Create a new ID for use in the Queue
    pub fn new_id(&self) -> Uuid {
        Uuid::new_v4()
    }

    async fn conn(&self) -> Result<Connection> {
        get_connection(&self.pool).await
    }

    /// Set the current status of the queue
    pub async fn queue_status(
        &self,
        prefix: impl Into<String>,
    ) -> Result<(QueueEnabled, StoreCapacity, QueueSyncTimestamp)> {
        let prefix = prefix.into();
        let enabled_key = format!("{}::queue_enabled", &prefix);
        let capacity_key = format!("{}::store_capacity", &prefix);
        let time_key = format!("{}::queue_sync_timestamp", &prefix);

        // Set all values in single pipeline to ensure atomic consistency
        let mut conn = self.conn().await?;
        let result: (Option<isize>, Option<isize>, Option<usize>) = pipe()
            .atomic()
            .get(enabled_key)
            .get(capacity_key)
            .get(time_key)
            .query_async(&mut conn)
            .await?;

        let enabled = match result.0 {
            Some(enabled) => QueueEnabled::try_from(enabled)?,
            None => QueueEnabled(false),
        };
        let capacity = match result.1 {
            Some(capacity) => StoreCapacity::try_from(capacity)?,
            None => StoreCapacity::Unlimited,
        };
        let timestamp = match result.2 {
            Some(timestamp) => QueueSyncTimestamp::try_from(timestamp)?,
            None => QueueSyncTimestamp(0),
        };

        Ok((enabled, capacity, timestamp))
    }

    /// Set the current status of the queue
    pub async fn set_queue_status(
        &self,
        prefix: impl Into<String>,
        enabled: impl Into<QueueEnabled>,
        capacity: impl Into<StoreCapacity>,
    ) -> Result<()> {
        let prefix = prefix.into();
        let enabled = enabled.into();
        let capacity = capacity.into();

        let enabled_key = format!("{}::queue_enabled", &prefix);
        let capacity_key = format!("{}::store_capacity", &prefix);
        let time_key = format!("{}::queue_sync_timestamp", &prefix);

        let mut conn = self.conn().await?;
        let current_time = current_time(&mut conn).await?;

        // Set all values in single pipeline to ensure atomic consistency
        let _: (Option<String>, Option<String>, Option<String>) = pipe()
            .set(enabled_key, isize::from(enabled))
            .set(capacity_key, isize::from(capacity))
            .set(time_key, usize::from(current_time))
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Current size of the queue
    pub async fn queue_enabled(&self, prefix: impl Into<String>) -> Result<bool> {
        let mut conn = self.conn().await?;
        let key = format!("{}::queue_enabled", prefix.into());
        let enabled = conn.get(&key).await?;

        let default = false;
        match enabled {
            Some(e) => match QueueEnabled::try_from(e) {
                Ok(qe) => Ok(qe.into()),
                Err(_) => Ok(default),
            },
            None => Ok(default),
        }
    }

    /// Current size of the queue
    pub async fn queue_size(&self, prefix: impl Into<String>) -> Result<usize> {
        let mut conn = self.conn().await?;
        let key = format!("{}::queue_ids", prefix.into());
        let result = conn.llen(key).await?;
        Ok(result)
    }

    /// Current capacity of the store
    pub async fn store_capacity(&self, prefix: impl Into<String>) -> Result<StoreCapacity> {
        let mut conn = self.conn().await?;
        let key = format!("{}::store_capacity", prefix.into());
        let result = conn.get(key).await?;

        let capacity = match result {
            Some(r) => StoreCapacity::try_from(r)?,
            None => StoreCapacity::Unlimited,
        };
        Ok(capacity)
    }

    /// Current size of the store
    pub async fn store_size(&self, prefix: impl Into<String>) -> Result<usize> {
        let mut conn = self.conn().await?;
        let key = format!("{}::store_ids", prefix.into());
        let result = conn.llen(key).await?;
        Ok(result)
    }

    pub async fn waiting_page(&self, prefix: impl Into<String>) -> Result<Option<String>> {
        let mut conn = self.conn().await?;
        let key = format!("{}::waiting_page", prefix.into());
        let result = conn.get(key).await?;
        Ok(result)
    }

    pub async fn set_waiting_page(
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
    pub async fn check_sync_keys(&self) -> Result<bool> {
        let mut conn = self.conn().await?;
        self.scripts.check_sync_keys(&mut conn).await
    }

    /// Return true if the store or queue has any UUIDs, false if both the queue and store are empty
    pub async fn has_ids(&self) -> Result<bool> {
        let mut conn = self.conn().await?;
        self.scripts.has_ids(&mut conn).await
    }

    /// Add a UUID to the queue/store with expiration times, returning queue position
    pub async fn id_add(
        &self,
        prefix: impl Into<String>,
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
    pub async fn id_position(
        &self,
        prefix: impl Into<String>,
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
    pub async fn id_remove(&self, prefix: impl Into<String>, id: Uuid) -> Result<()> {
        let mut conn = self.conn().await?;
        self.scripts.id_remove(&mut conn, prefix, id).await
    }

    /// Full queue rotation using scripts in a pipeline
    pub async fn rotate_full(
        &self,
        prefix: impl Into<String>,
        batch_size: usize,
    ) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        self.scripts
            .rotate_full(&mut conn, prefix, batch_size)
            .await
    }

    /// Partial queue rotation that only expires IDs, but doesn't promote IDs from queue to store
    pub async fn rotate_expire(&self, prefix: impl Into<String>) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        self.scripts.rotate_expire(&mut conn, prefix.into()).await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::database::test::create_test_pool;
    use redis::AsyncTypedCommands;
    use tracing_test::traced_test;

    fn test_queue() -> QueueControl {
        let pool = create_test_pool().expect("Failed to create test pool");
        QueueControl::new(pool).expect("Failed to create test QueueControl")
    }

    async fn test_queue_conn() -> (QueueControl, Connection) {
        let queue = test_queue();
        let pool = queue.pool.clone();
        let conn = get_connection(&pool)
            .await
            .expect("Redis connection failed");

        (queue, conn)
    }

    #[test]
    #[traced_test]
    fn test_construct() {
        let Some(pool) = create_test_pool() else {
            return;
        };

        QueueControl::new(pool).expect("QueueControl::new() failed");
    }

    #[tokio::test]
    #[traced_test]
    async fn test_init() {
        let queue = test_queue();
        queue.init().await.expect("QueueControl::init() failed");
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_status_read() {
        let prefix = "test_queue_status_read";

        let expected_enabled: bool = true;
        let raw_capacity: isize = 4321;
        let expected_capacity = StoreCapacity::try_from(raw_capacity).unwrap();
        let expected_timestamp: usize = 1757438630;

        let (queue, mut conn) = test_queue_conn().await;

        // Prepare keys
        conn.set(format!("{}::queue_enabled", prefix), expected_enabled)
            .await
            .expect("Failed to set ::queue_enabled");

        conn.set(format!("{}::store_capacity", prefix), raw_capacity)
            .await
            .expect("Failed to set ::store_capacity");

        conn.set(
            format!("{}::queue_sync_timestamp", prefix),
            expected_timestamp,
        )
        .await
        .expect("Failed to set ::queue_sync_timestamp");

        // Read status
        let result = queue
            .queue_status(prefix)
            .await
            .expect("Failed to read queue status");

        assert_eq!(result.0, QueueEnabled::from(expected_enabled));
        assert_eq!(result.1, StoreCapacity::from(expected_capacity));
        assert_eq!(result.2, QueueSyncTimestamp::from(expected_timestamp));
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_status_default() {
        let prefix = "test_queue_status_default";

        let (queue, mut conn) = test_queue_conn().await;

        // Prepare keys
        for key in &["queue_enabled", "store_capacity", "queue_sync_timestamp"] {
            let full_key = format!("{}::{}", prefix, key);
            conn.del(&full_key)
                .await
                .expect(format!("Failed to delete: {}", full_key).as_ref());
        }

        // Read status
        let status = queue
            .queue_status(prefix)
            .await
            .expect("Failed to get queue status");

        assert_eq!(status.0, QueueEnabled(false));
        assert_eq!(status.1, StoreCapacity::Unlimited);
        assert_eq!(status.2, QueueSyncTimestamp(0));
    }

    #[tokio::test]
    #[traced_test]
    async fn test_set_queue_status() {
        let prefix = "test_set_queue_status";

        let enabled = QueueEnabled(true);
        let capacity = StoreCapacity::Sized(50);

        let queue = test_queue();
        queue
            .set_queue_status(prefix, enabled.clone(), capacity.clone())
            .await
            .expect("Failed to read queue");

        let status = queue
            .queue_status(prefix)
            .await
            .expect("Failed to get queue status");

        assert_eq!(status.0, enabled);
        assert_eq!(status.1, capacity);
        assert!(status.2.0 > 0);
    }
}
