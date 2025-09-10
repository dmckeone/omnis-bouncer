use deadpool_redis::{redis, Connection, Pool as RedisPool};
use redis::{pipe, AsyncTypedCommands};
use uuid::Uuid;

use crate::database::{current_time, get_connection};
use crate::errors::Result;
use crate::queue::models::{
    QueueEnabled, QueueRotate, QueueSettings, QueueStatus, QueueSyncTimestamp, StoreCapacity,
};
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
    pub async fn queue_status(&self, prefix: impl Into<String>) -> Result<QueueStatus> {
        let prefix = prefix.into();

        let enabled_key = format!("{}:queue_enabled", &prefix);
        let capacity_key = format!("{}:store_capacity", &prefix);
        let store_ids_key = format!("{}:store_ids", &prefix);
        let queue_ids_key = format!("{}:queue_ids", &prefix);
        let time_key = format!("{}:queue_sync_timestamp", &prefix);

        // Set all values in single pipeline to ensure atomic consistency
        let mut conn = self.conn().await?;
        let result: (
            Option<isize>,
            Option<isize>,
            Option<usize>,
            Option<usize>,
            Option<isize>,
        ) = pipe()
            .atomic()
            .get(enabled_key)
            .get(capacity_key)
            .scard(store_ids_key)
            .llen(queue_ids_key)
            .get(time_key)
            .query_async(&mut conn)
            .await?;

        let status = QueueStatus {
            enabled: match result.0 {
                Some(enabled) => QueueEnabled::try_from(enabled)?.into(),
                None => false,
            },
            capacity: match result.1 {
                Some(capacity) => StoreCapacity::try_from(capacity)?,
                None => StoreCapacity::Unlimited,
            },
            store_size: result.2.unwrap_or_else(|| 0),
            queue_size: result.3.unwrap_or_else(|| 0),
            sync_timestamp: match result.4 {
                Some(timestamp) => QueueSyncTimestamp::from(timestamp).into(),
                None => 0,
            },
        };

        Ok(status)
    }

    /// Set the current status of the queue
    pub async fn queue_settings(&self, prefix: impl Into<String>) -> Result<QueueSettings> {
        let prefix = prefix.into();

        let enabled_key = format!("{}:queue_enabled", &prefix);
        let capacity_key = format!("{}:store_capacity", &prefix);
        let time_key = format!("{}:queue_sync_timestamp", &prefix);

        // Set all values in single pipeline to ensure atomic consistency
        let mut conn = self.conn().await?;
        let result: (Option<isize>, Option<isize>, Option<usize>) = pipe()
            .atomic()
            .get(enabled_key)
            .get(capacity_key)
            .get(time_key)
            .query_async(&mut conn)
            .await?;

        let settings = QueueSettings {
            enabled: match result.0 {
                Some(enabled) => QueueEnabled::try_from(enabled)?.into(),
                None => false,
            },
            capacity: match result.1 {
                Some(capacity) => StoreCapacity::try_from(capacity)?,
                None => StoreCapacity::Unlimited,
            },
            sync_timestamp: match result.2 {
                Some(timestamp) => QueueSyncTimestamp::from(timestamp).into(),
                None => 0,
            },
        };

        Ok(settings)
    }

    /// Set the current status of the queue
    pub async fn set_queue_settings(
        &self,
        prefix: impl Into<String>,
        enabled: impl Into<QueueEnabled>,
        capacity: impl Into<StoreCapacity>,
    ) -> Result<()> {
        let prefix = prefix.into();
        let enabled = enabled.into();
        let capacity = capacity.into();

        let enabled_key = format!("{}:queue_enabled", &prefix);
        let capacity_key = format!("{}:store_capacity", &prefix);
        let time_key = format!("{}:queue_sync_timestamp", &prefix);

        let mut conn = self.conn().await?;
        let current_time = current_time(&mut conn).await?;

        // Set all values in single pipeline to ensure atomic consistency
        let _: (Option<String>, Option<String>, Option<String>) = pipe()
            .set(enabled_key, isize::from(enabled))
            .set(capacity_key, isize::from(capacity))
            .set(time_key, current_time)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Current size of the queue
    pub async fn queue_enabled(&self, prefix: impl Into<String>) -> Result<bool> {
        let prefix = prefix.into();

        let key = format!("{}:queue_enabled", prefix);

        let mut conn = self.conn().await?;
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
        let prefix = prefix.into();

        let key = format!("{}:queue_ids", prefix);

        let mut conn = self.conn().await?;
        let result = conn.llen(key).await?;

        Ok(result)
    }

    /// Current capacity of the store
    pub async fn store_capacity(&self, prefix: impl Into<String>) -> Result<StoreCapacity> {
        let prefix = prefix.into();

        let key = format!("{}:store_capacity", prefix);

        let mut conn = self.conn().await?;
        let result = conn.get(key).await?;

        let capacity = match result {
            Some(r) => StoreCapacity::try_from(r)?,
            None => StoreCapacity::Unlimited,
        };
        Ok(capacity)
    }

    /// Current size of the store
    pub async fn store_size(&self, prefix: impl Into<String>) -> Result<usize> {
        let prefix = prefix.into();

        let key = format!("{}:store_ids", prefix);

        let mut conn = self.conn().await?;
        let result = conn.scard(key).await?;

        Ok(result)
    }

    pub async fn waiting_page(&self, prefix: impl Into<String>) -> Result<Option<String>> {
        let prefix = prefix.into();

        let key = format!("{}:waiting_page", prefix);

        let mut conn = self.conn().await?;
        let result = conn.get(key).await?;

        Ok(result)
    }

    pub async fn set_waiting_page(
        &self,
        prefix: impl Into<String>,
        waiting_page: impl Into<String>,
    ) -> Result<()> {
        let prefix = prefix.into();
        let waiting_page = waiting_page.into();

        let key = format!("{}:waiting_page", prefix);

        let mut conn = self.conn().await?;
        conn.set(key, waiting_page).await?;

        Ok(())
    }

    /// Check that all keys required for syncing the queue/store are available
    pub async fn check_sync_keys(&self, prefix: impl Into<String>) -> Result<bool> {
        let mut conn = self.conn().await?;
        self.scripts.check_sync_keys(&mut conn, prefix).await
    }

    /// Return true if the store or queue has any UUIDs, false if both the queue and store are empty
    pub async fn has_ids(&self, prefix: impl Into<String>) -> Result<bool> {
        let mut conn = self.conn().await?;
        self.scripts.has_ids(&mut conn, prefix).await
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
        self.scripts.rotate_expire(&mut conn, prefix).await
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

    fn generate_ids(queue: &QueueControl, count: usize) -> Vec<String> {
        let mut vec = Vec::new();
        for _ in 0..count {
            vec.push(queue.new_id().to_string());
        }
        vec
    }

    async fn clear_store(prefix: impl Into<String>, conn: &mut Connection) {
        let prefix = prefix.into();

        let keys = &[
            format!("{}:queue_ids", prefix),
            format!("{}:queue_position_cache", prefix),
            format!("{}:queue_expiry_secs", prefix),
            format!("{}:store_ids", prefix),
            format!("{}:store_expiry_secs", prefix),
        ];

        for key in keys {
            conn.del(&key)
                .await
                .expect(format!("Failed to delete {}", key).as_ref());
        }
    }

    async fn add_queue_ids(
        prefix: impl Into<String>,
        queue: &QueueControl,
        conn: &mut Connection,
        count: usize,
    ) {
        let prefix = prefix.into();

        let key = format!("{}:queue_ids", &prefix);
        conn.del(&key)
            .await
            .expect(format!("Failed to delete {}", key).as_ref());

        let ids = generate_ids(&queue, count);

        conn.lpush(&key, &ids)
            .await
            .expect(format!("Failed to queue ids: {:?}", ids).as_ref());
    }

    async fn add_store_ids(
        prefix: impl Into<String>,
        queue: &QueueControl,
        conn: &mut Connection,
        count: usize,
    ) {
        let prefix = prefix.into();

        let key = format!("{}:store_ids", &prefix);
        conn.del(&key)
            .await
            .expect(format!("Failed to delete {}", key).as_ref());

        let ids = generate_ids(&queue, count);

        conn.sadd(&key, &ids)
            .await
            .expect(format!("Failed to store ids: {:?}", ids).as_ref());
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
    async fn test_queue_status() {
        let prefix = "test_queue_status_read";

        let expected_enabled: bool = true;
        let raw_capacity: isize = 4321;
        let expected_capacity = StoreCapacity::try_from(raw_capacity).unwrap();
        let expected_store_size = 2;
        let expected_queue_size = 5;
        let expected_timestamp: usize = 1757438630;

        let (queue, mut conn) = test_queue_conn().await;
        add_store_ids(prefix, &queue, &mut conn, expected_store_size).await;
        add_queue_ids(prefix, &queue, &mut conn, expected_queue_size).await;

        queue
            .set_queue_settings(prefix, expected_enabled, expected_capacity.clone())
            .await
            .expect("QueueControl::set_queue_settings failed");

        // Read status
        let result = queue
            .queue_status(prefix)
            .await
            .expect("Failed to read queue status");

        assert_eq!(result.enabled, expected_enabled);
        assert_eq!(result.capacity, expected_capacity);
        assert_eq!(result.store_size, expected_store_size);
        assert_eq!(result.queue_size, expected_queue_size);
        assert!(result.sync_timestamp > 0);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_settings() {
        let prefix = "test_queue_settings_read";

        let expected_enabled: bool = true;
        let raw_capacity: isize = 4321;
        let expected_capacity = StoreCapacity::try_from(raw_capacity).unwrap();
        let expected_timestamp: usize = 1757438630;

        let (queue, mut conn) = test_queue_conn().await;

        // Prepare keys
        conn.set(format!("{}:queue_enabled", prefix), expected_enabled)
            .await
            .expect("Failed to set ::queue_enabled");

        conn.set(format!("{}:store_capacity", prefix), raw_capacity)
            .await
            .expect("Failed to set :store_capacity");

        conn.set(
            format!("{}:queue_sync_timestamp", prefix),
            expected_timestamp,
        )
        .await
        .expect("Failed to set ::queue_sync_timestamp");

        // Read status
        let result = queue
            .queue_settings(prefix)
            .await
            .expect("Failed to read queue status");

        assert_eq!(result.enabled, expected_enabled);
        assert_eq!(result.capacity, StoreCapacity::from(expected_capacity));
        assert_eq!(result.sync_timestamp, expected_timestamp);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_settings_default() {
        let prefix = "test_queue_settings_default";

        let (queue, mut conn) = test_queue_conn().await;

        // Prepare keys
        for key in &["queue_enabled", "store_capacity", "queue_sync_timestamp"] {
            let full_key = format!("{}:{}", prefix, key);
            conn.del(&full_key)
                .await
                .expect(format!("Failed to delete: {}", full_key).as_ref());
        }

        // Read status
        let status = queue
            .queue_settings(prefix)
            .await
            .expect("Failed to get queue status");

        assert_eq!(status.enabled, false);
        assert_eq!(status.capacity, StoreCapacity::Unlimited);
        assert_eq!(status.sync_timestamp, 0);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_set_queue_settings() {
        let prefix = "test_set_queue_status";

        let enabled = true;
        let capacity = StoreCapacity::Sized(50);

        let queue = test_queue();
        queue
            .set_queue_settings(prefix, enabled, capacity.clone())
            .await
            .expect("Failed to read queue");

        let status = queue
            .queue_settings(prefix)
            .await
            .expect("Failed to get queue status");

        assert_eq!(status.enabled, enabled);
        assert_eq!(status.capacity, capacity);
        assert!(status.sync_timestamp > 0);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_enabled() {
        let prefix = "test_queue_enabled";

        let (queue, mut conn) = test_queue_conn().await;

        let key = format!("{}:queue_enabled", &prefix);
        conn.set(&key, 1)
            .await
            .expect(format!("Failed to set {}", key).as_ref());

        let actual = queue
            .queue_enabled(prefix)
            .await
            .expect("Failed to call queue_enabled");

        assert_eq!(actual, true);

        conn.set(&key, 0)
            .await
            .expect(format!("Failed to set {}", key).as_ref());

        let actual = queue
            .queue_enabled(prefix)
            .await
            .expect("Failed to call queue_enabled");

        assert_eq!(actual, false);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_size() {
        let prefix = "test_queue_size";

        let (queue, mut conn) = test_queue_conn().await;

        let key = format!("{}:queue_ids", &prefix);
        conn.del(&key)
            .await
            .expect(format!("Failed to delete {}", key).as_ref());

        let count = 3;
        let ids = generate_ids(&queue, count);
        conn.lpush(&key, &ids)
            .await
            .expect(format!("Failed to push ids: {:?}", ids).as_ref());

        let actual = queue
            .queue_size(prefix)
            .await
            .expect("Failed to call queue_size");

        assert_eq!(actual, count);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_store_capacity_sized() {
        let prefix = "test_store_capacity_sized";

        let (queue, mut conn) = test_queue_conn().await;

        let key = format!("{}:store_capacity", &prefix);
        conn.set(&key, isize::MAX)
            .await
            .expect(format!("Failed to set {}", key).as_ref());

        let actual = queue
            .store_capacity(prefix)
            .await
            .expect("Failed to call store_capacity");

        let expected = StoreCapacity::Sized(isize::MAX as usize);
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_store_capacity_unlimited() {
        let prefix = "test_store_capacity_unlimited";

        let (queue, mut conn) = test_queue_conn().await;

        let key = format!("{}:store_capacity", &prefix);
        conn.set(&key, -1)
            .await
            .expect(format!("Failed to set {}", key).as_ref());

        let actual = queue
            .store_capacity(prefix)
            .await
            .expect("Failed to call store_capacity");

        let expected = StoreCapacity::Unlimited;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_store_size() {
        let prefix = "test_store_size";

        let (queue, mut conn) = test_queue_conn().await;

        let key = format!("{}:store_ids", &prefix);
        conn.del(&key)
            .await
            .expect(format!("Failed to delete {}", key).as_ref());

        let count = 4;
        let ids = generate_ids(&queue, count);

        conn.sadd(&key, &ids)
            .await
            .expect(format!("Failed to push ids: {:?}", ids).as_ref());

        let actual = queue
            .store_size(prefix)
            .await
            .expect("Failed to call store_size");

        assert_eq!(actual, count);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_waiting_page() {
        let prefix = "test_store_capacity_sized";

        let queue = test_queue();

        let expected = "My Waiting Page";

        queue
            .set_waiting_page(prefix, expected)
            .await
            .expect("Failed to call set_waiting_page");

        let actual = queue
            .waiting_page(prefix)
            .await
            .expect("Failed to call waiting_page")
            .expect("waiting page is None");

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_sync_keys_false() {
        let prefix = "test_sync_keys_false";

        let (queue, mut conn) = test_queue_conn().await;

        let keys = &[
            format!("{}:queue_enabled", prefix),
            format!("{}:store_capacity", prefix),
            format!("{}:queue_waiting_page", prefix),
            format!("{}:queue_sync_timestamp", prefix),
        ];

        conn.del(&keys).await.expect("Failed to delete keys");

        let actual = queue
            .check_sync_keys(prefix)
            .await
            .expect("Failed to call check_sync_keys");

        assert_eq!(actual, false);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_sync_keys_true() {
        let prefix = "test_sync_keys_true";

        let (queue, mut conn) = test_queue_conn().await;

        conn.set(format!("{}:queue_enabled", prefix), 1)
            .await
            .expect("Failed to set :queue_enabled");

        conn.set(format!("{}:store_capacity", prefix), 5)
            .await
            .expect("Failed to set :store_capacity");

        conn.set(format!("{}:queue_waiting_page", prefix), "Waiting Page")
            .await
            .expect("Failed to set :queue_waiting_page");

        conn.set(format!("{}:queue_sync_timestamp", prefix), 1757463125)
            .await
            .expect("Failed to set :queue_sync_timestamp");

        let actual = queue
            .check_sync_keys(prefix)
            .await
            .expect("Failed to call check_sync_keys");

        assert_eq!(actual, true);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_true_queue() {
        let prefix = "test_has_ids_true_queue";

        let (queue, mut conn) = test_queue_conn().await;
        add_queue_ids(prefix, &queue, &mut conn, 1).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");

        assert!(actual);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_true_store() {
        let prefix = "test_has_ids_true_store";

        let (queue, mut conn) = test_queue_conn().await;
        add_store_ids(prefix, &queue, &mut conn, 1).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");
        assert_eq!(actual, true);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_true_both() {
        let prefix = "test_has_ids_true_both";

        let (queue, mut conn) = test_queue_conn().await;
        add_store_ids(prefix, &queue, &mut conn, 1).await;
        add_queue_ids(prefix, &queue, &mut conn, 1).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");
        assert_eq!(actual, true);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_false() {
        let prefix = "test_has_ids_false";

        let (queue, mut conn) = test_queue_conn().await;
        clear_store(prefix, &mut conn).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");
        assert_eq!(actual, false);
    }

    #[tokio::test]
    #[traced_test]
    async fn test_id_add() {
        let prefix = "test_id_add";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 1
        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(1))
            .await
            .expect("Failed to set queue status");

        // Ensure correct starting environment
        assert_eq!(queue.store_size(prefix).await.unwrap(), 0);
        assert_eq!(queue.queue_size(prefix).await.unwrap(), 0);

        // Add new ID to store
        let id = queue.new_id();
        queue
            .id_add(
                prefix, id, 1757510637, // unix time
                600,        // seconds
                45,         // seconds
            )
            .await
            .expect("Failed to add new ID to store");

        assert_eq!(queue.store_size(prefix).await.unwrap(), 1);
        assert_eq!(queue.queue_size(prefix).await.unwrap(), 0);

        let id = queue.new_id();
        queue
            .id_add(
                prefix, id, 1757510637, // unix time
                600,        // seconds
                45,         // seconds
            )
            .await
            .expect("Failed to add new ID to queue");

        assert_eq!(queue.store_size(prefix).await.unwrap(), 1);
        assert_eq!(queue.queue_size(prefix).await.unwrap(), 1);
    }
}
