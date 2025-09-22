use chrono::{DateTime, Utc};
use deadpool_redis::{redis, Connection, Pool as RedisPool};
use lazy_static::lazy_static;
use minify_html_onepass::{copy as minify, Cfg};
use redis::{pipe, AsyncTypedCommands};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, RwLock};
use tracing::error;
use uuid::Uuid;

use crate::constants::{DEFAULT_WAITING_ROOM_PAGE, HTML_TEMPLATE_DIR};
use crate::database::{current_time, get_connection};
use crate::errors::Result;
use crate::queue::models::{
    QueueEnabled, QueueEvent, QueuePosition, QueueRotate, QueueSettings, QueueStatus, StoreCapacity,
};
use crate::queue::scripts::{
    queue_enabled_key, queue_ids_key, queue_sync_timestamp_key, store_capacity_key, store_ids_key,
    waiting_page_key, Scripts,
};

lazy_static! {
    static ref minfiy_cfg: Cfg = Cfg {
        minify_js: true,
        minify_css: true,
    };
    static ref DefaultWaitingPage: String = String::from_utf8(
        minify(
            HTML_TEMPLATE_DIR
                .get_file(DEFAULT_WAITING_ROOM_PAGE)
                .expect("Failed to read bundled waiting room page")
                .contents(),
            &minfiy_cfg
        )
        .expect("Failed to minify bundled default waiting page")
    )
    .expect("Failed to convert bundled waiting page to string");
}

pub struct QueueControl {
    pool: RedisPool,
    quarantine_expiry: Duration,
    validated_expiry: Duration,
    scripts: Scripts,
    broadcast: broadcast::Sender<QueueEvent>,
    _receiver: broadcast::Receiver<QueueEvent>,
    throttle_buffer: RwLock<HashMap<QueueEvent, Instant>>,
    emit_throttle: Duration,
    waiting_page_cache: RwLock<HashMap<String, String>>,
}

impl QueueControl {
    pub fn new(
        pool: RedisPool,
        quarantine_expiry: Duration,
        validated_expiry: Duration,
        emit_throttle: Duration,
    ) -> Result<Self> {
        let (broadcast, receiver) = broadcast::channel::<QueueEvent>(50);

        let queue = Self {
            pool,
            quarantine_expiry,
            validated_expiry,
            scripts: Scripts::new()?,
            broadcast,
            _receiver: receiver, // Keep a single receiver around so the channel doesn't close
            throttle_buffer: RwLock::new(HashMap::new()),
            emit_throttle,
            waiting_page_cache: RwLock::new(HashMap::new()),
        };

        Ok(queue)
    }

    pub async fn init(
        &self,
        prefix: impl Into<String>,
        enabled: bool,
        store_capacity: StoreCapacity,
    ) -> Result<()> {
        let prefix = prefix.into();

        let mut conn = self.conn().await?;
        self.scripts.init(&mut conn).await?;
        self.verify_keys(&prefix, enabled, store_capacity).await?;
        self.verify_waiting_page(&prefix).await;
        Ok(())
    }

    /// Verify that startup keys are correct
    pub async fn verify_keys(
        &self,
        prefix: impl Into<String>,
        enabled: bool,
        store_capacity: StoreCapacity,
    ) -> Result<()> {
        let prefix = prefix.into();

        let result = self.check_sync_keys(&prefix).await;
        if let Ok(has_keys) = result
            && !has_keys
        {
            self.set_queue_settings(&prefix, enabled, store_capacity)
                .await?;
        }

        Ok(())
    }

    // Create a new ID for use in the Queue
    pub fn new_id(&self) -> Uuid {
        Uuid::new_v4()
    }

    async fn conn(&self) -> Result<Connection> {
        get_connection(&self.pool).await
    }

    /// Return a broadcast receiver that emits events from the queue
    pub fn subscribe(&self) -> broadcast::Receiver<QueueEvent> {
        self.broadcast.subscribe()
    }

    /// Emit an event from this QueueControl, throttled by the emit limit
    pub async fn emit(&self, event: QueueEvent, now: Option<Instant>) {
        // Check if event needs to be throttled
        {
            let guard = self.throttle_buffer.read().await;
            let now = now.unwrap_or(Instant::now());
            let instant = (*guard).get(&event);

            if let Some(instant) = instant
                && now.duration_since(*instant) < self.emit_throttle
            {
                // EARLY EXIT: Event was already emitted recently
                return;
            }
        }

        // Otherwise emit event and insert the event into the event throttle buffer
        if let Err(error) = self.broadcast.send(event.clone()) {
            // EARLY EXIT: Failed to emit the event
            error!("Failed to send queue event - \"{:?}\": {}", event, error);
            return;
        }

        // Write successful event to the event throttle buffer
        let now = now.unwrap_or(Instant::now());
        let mut guard = self.throttle_buffer.write().await;
        (*guard).insert(event, now);
    }

    /// Flush the event throttle buffer of any stale events
    pub async fn flush_event_throttle_buffer(&self, now: Option<Instant>) {
        let mut guard = self.throttle_buffer.write().await;
        let now = now.unwrap_or(Instant::now());
        (*guard).retain(|_, instant| now.duration_since(*instant) < self.emit_throttle);
    }

    /// Set the current status of the queue
    pub async fn queue_status(&self, prefix: impl Into<String>) -> Result<QueueStatus> {
        let prefix = prefix.into();

        // Set all values in single pipeline to ensure atomic consistency
        let mut conn = self.conn().await?;
        type Result = (
            Option<isize>,
            Option<isize>,
            Option<usize>,
            Option<usize>,
            Option<i64>,
        );
        let result: Result = pipe()
            .atomic()
            .get(queue_enabled_key(&prefix))
            .get(store_capacity_key(&prefix))
            .scard(store_ids_key(&prefix))
            .llen(queue_ids_key(&prefix))
            .get(queue_sync_timestamp_key(&prefix))
            .query_async(&mut conn)
            .await?;

        let status = QueueStatus {
            enabled: match result.0 {
                Some(enabled) => QueueEnabled::try_from(enabled)?.into(),
                None => false,
            },
            capacity: StoreCapacity::try_from(result.1)?,
            store_size: result.2.unwrap_or(0),
            queue_size: result.3.unwrap_or(0),
            updated: DateTime::from_timestamp_secs(result.4.unwrap_or(0)),
        };

        Ok(status)
    }

    /// Set the current status of the queue
    pub async fn queue_settings(&self, prefix: impl Into<String>) -> Result<QueueSettings> {
        let prefix = prefix.into();

        // Set all values in single pipeline to ensure atomic consistency
        let mut conn = self.conn().await?;
        let result: (Option<isize>, Option<isize>, Option<i64>) = pipe()
            .atomic()
            .get(queue_enabled_key(&prefix))
            .get(store_capacity_key(&prefix))
            .get(queue_sync_timestamp_key(&prefix))
            .query_async(&mut conn)
            .await?;

        let settings = QueueSettings {
            enabled: QueueEnabled::try_from(result.0)?.into(),
            capacity: StoreCapacity::try_from(result.1)?,
            updated: DateTime::from_timestamp_secs(result.2.unwrap_or(0)),
        };

        Ok(settings)
    }

    /// Set the current status of the queue
    pub async fn set_queue_settings(
        &self,
        prefix: impl Into<String>,
        enabled: bool,
        capacity: impl Into<StoreCapacity>,
    ) -> Result<()> {
        let prefix = prefix.into();
        let capacity = capacity.into();

        let mut conn = self.conn().await?;
        let now = current_time(&mut conn).await?;

        // Set all values in single pipeline to ensure atomic consistency
        let _: (Option<String>, Option<String>, Option<String>) = pipe()
            .atomic()
            .set(queue_enabled_key(&prefix), isize::from(enabled))
            .set(store_capacity_key(&prefix), isize::from(capacity))
            .set(queue_sync_timestamp_key(&prefix), now.timestamp())
            .query_async(&mut conn)
            .await?;

        self.emit(QueueEvent::SettingsChanged, None).await;

        Ok(())
    }

    /// Set the queue enabled status
    pub async fn set_queue_enabled(&self, prefix: impl Into<String>, enabled: bool) -> Result<()> {
        let prefix = prefix.into();

        let mut conn = self.conn().await?;
        let now = current_time(&mut conn).await?;

        // Set all values in single pipeline to ensure atomic consistency
        let _: (Option<String>, Option<String>) = pipe()
            .atomic()
            .set(queue_enabled_key(&prefix), isize::from(enabled))
            .set(queue_sync_timestamp_key(&prefix), now.timestamp())
            .query_async(&mut conn)
            .await?;

        self.emit(QueueEvent::SettingsChanged, None).await;

        Ok(())
    }

    /// Set the queue enabled status
    pub async fn set_store_capacity(
        &self,
        prefix: impl Into<String>,
        capacity: impl Into<StoreCapacity>,
    ) -> Result<()> {
        let prefix = prefix.into();
        let capacity = capacity.into();

        let mut conn = self.conn().await?;
        let now = current_time(&mut conn).await?;

        // Set all values in single pipeline to ensure atomic consistency
        let _: (Option<String>, Option<String>) = pipe()
            .atomic()
            .set(store_capacity_key(&prefix), isize::from(capacity))
            .set(queue_sync_timestamp_key(&prefix), now.timestamp())
            .query_async(&mut conn)
            .await?;

        self.emit(QueueEvent::SettingsChanged, None).await;

        Ok(())
    }

    /// Current size of the queue
    pub async fn queue_enabled(&self, prefix: impl Into<String>) -> Result<bool> {
        let prefix = prefix.into();

        let mut conn = self.conn().await?;
        let enabled = conn.get(queue_enabled_key(&prefix)).await?;

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

        let mut conn = self.conn().await?;
        let result = conn.llen(queue_ids_key(&prefix)).await?;

        Ok(result)
    }

    /// Current capacity of the store
    pub async fn store_capacity(&self, prefix: impl Into<String>) -> Result<StoreCapacity> {
        let prefix = prefix.into();

        let mut conn = self.conn().await?;
        let result = conn.get(store_capacity_key(&prefix)).await?;

        let capacity = StoreCapacity::try_from(result)?;
        Ok(capacity)
    }

    /// Current size of the store
    pub async fn store_size(&self, prefix: impl Into<String>) -> Result<usize> {
        let prefix = prefix.into();

        let mut conn = self.conn().await?;
        let result = conn.scard(store_ids_key(&prefix)).await?;

        Ok(result)
    }

    pub async fn waiting_page(&self, prefix: impl Into<String>) -> Result<Option<String>> {
        let prefix = prefix.into();

        let mut conn = self.conn().await?;
        let result = conn.get(waiting_page_key(&prefix)).await?;

        Ok(result)
    }

    pub async fn set_waiting_page(
        &self,
        prefix: impl Into<String>,
        waiting_page: impl Into<String>,
    ) -> Result<()> {
        let prefix = prefix.into();
        let waiting_page = waiting_page.into();

        let mut conn = self.conn().await?;
        conn.set(waiting_page_key(&prefix), waiting_page).await?;

        self.emit(QueueEvent::WaitingPageChanged, None).await;

        Ok(())
    }

    pub async fn cached_waiting_page(&self, prefix: impl Into<String>) -> String {
        let prefix = prefix.into();
        let guard = self.waiting_page_cache.read().await;

        match (*guard).get(&prefix) {
            Some(waiting_page) => waiting_page.clone(),
            None => (*DefaultWaitingPage).clone(),
        }
    }

    pub async fn verify_waiting_page(&self, prefix: impl Into<String>) {
        let prefix = prefix.into();

        let cached = {
            let guard = self.waiting_page_cache.read().await;
            (*guard).get(&prefix).cloned()
        };

        let current = match self.waiting_page(&prefix).await {
            Ok(Some(waiting_page)) => match minify(waiting_page.as_bytes(), &minfiy_cfg) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(minified) => Some(minified),
                    Err(error) => {
                        error!("Failed convert minified waiting page to Redis: {:?}", error);
                        None
                    }
                },
                Err(error) => {
                    error!("Failed to minify waiting page in Redis: {:?}", error);
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                error!(
                    "Failed to load waiting page while verifying cache: {:?}",
                    error
                );
                None
            }
        };

        if cached != current {
            // Cache invalid, get write lock update to latest version
            let mut guard = self.waiting_page_cache.write().await;
            match current {
                Some(waiting_page) => (*guard).insert(prefix.clone(), waiting_page),
                None => (*guard).remove(&prefix),
            };
        }
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

    /// Return the position of a UUID in the queue, or add the UUID to the queue and then
    /// return the position if the UUID does not already exist in the queue
    pub async fn id_position(
        &self,
        prefix: impl Into<String>,
        id: Uuid,
        time: Option<DateTime<Utc>>,
    ) -> Result<QueuePosition> {
        let mut conn = self.conn().await?;

        let (added, position) = self
            .scripts
            .id_position(
                &mut conn,
                prefix,
                id,
                time,
                self.validated_expiry,
                self.quarantine_expiry,
            )
            .await?;

        let position: QueuePosition = position.into();
        if added {
            let event = match position {
                QueuePosition::Store => QueueEvent::StoreAdded,
                QueuePosition::Queue(_) => QueueEvent::QueueAdded,
            };
            self.emit(event, None).await;
        }

        Ok(position)
    }

    /// Remove a given UUID from the queue/store
    pub async fn id_remove(
        &self,
        prefix: impl Into<String>,
        id: Uuid,
        time: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let mut conn = self.conn().await?;
        self.scripts.id_remove(&mut conn, prefix, id, time).await?;
        self.emit(QueueEvent::QueueRemoved, None).await;
        Ok(())
    }

    /// Full queue rotation using scripts in a pipeline
    pub async fn rotate_full(
        &self,
        prefix: impl Into<String>,
        time: Option<DateTime<Utc>>,
    ) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        let rotate = self.scripts.rotate_full(&mut conn, prefix, time).await?;

        if rotate.promoted > 0 {
            self.emit(QueueEvent::StoreAdded, None).await;
        }
        if rotate.queue_expired > 0 {
            self.emit(QueueEvent::QueueExpired, None).await;
        }
        if rotate.store_expired > 0 {
            self.emit(QueueEvent::StoreExpired, None).await;
        }

        Ok(rotate)
    }

    /// Partial queue rotation that only expires IDs, but doesn't promote IDs from queue to store
    pub async fn rotate_expire(
        &self,
        prefix: impl Into<String>,
        time: Option<DateTime<Utc>>,
    ) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        let rotate = self.scripts.rotate_expire(&mut conn, prefix, time).await?;

        if rotate.promoted > 0 {
            self.emit(QueueEvent::StoreAdded, None).await;
        }
        if rotate.queue_expired > 0 {
            self.emit(QueueEvent::QueueExpired, None).await;
        }
        if rotate.store_expired > 0 {
            self.emit(QueueEvent::StoreExpired, None).await;
        }

        Ok(rotate)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use tracing_test::traced_test;

    use crate::database::test::create_test_pool;
    use crate::queue::scripts::{
        queue_expiry_secs_key, queue_position_cache_key, store_expiry_secs_key,
    };

    static QUARANTINE: Duration = Duration::from_secs(45);
    static VALIDATED: Duration = Duration::from_secs(600);
    static EMIT_THROTTLE: Duration = Duration::from_secs(100);

    fn test_queue() -> QueueControl {
        let pool = create_test_pool().expect("Failed to create test pool");
        QueueControl::new(pool, QUARANTINE, VALIDATED, EMIT_THROTTLE)
            .expect("Failed to create test QueueControl")
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
            queue_ids_key(&prefix),
            queue_position_cache_key(&prefix),
            queue_expiry_secs_key(&prefix),
            store_ids_key(&prefix),
            store_expiry_secs_key(&prefix),
        ];

        for key in keys {
            conn.del(&key)
                .await
                .expect(format!("Failed to delete {}", key).as_ref());
        }
    }

    /// Add `count` users to the queue
    async fn add_many(
        queue: &QueueControl,
        prefix: impl Into<String>,
        count: usize,
        time: Option<DateTime<Utc>>,
    ) -> Vec<Uuid> {
        let prefix = prefix.into();

        let mut ids = Vec::new();
        for _ in 0..count {
            let id = queue.new_id();

            queue
                .id_position(&prefix, id, time)
                .await
                .expect("Failed to add new ID to queue");

            ids.push(id);
        }

        ids
    }

    async fn exists_in_store(
        prefix: impl Into<String>,
        conn: &mut Connection,
        id: impl Into<String>,
    ) -> bool {
        let id = id.into();
        let prefix = prefix.into();

        let (store_exists, store_expiry_exists): (Option<isize>, Option<isize>) = pipe()
            .sismember(store_ids_key(&prefix), id.clone())
            .hexists(store_expiry_secs_key(&prefix), id.clone())
            .query_async(conn)
            .await
            .expect("Failed to check store");

        let store_exists = store_exists.expect("Store exists incorrectly returned nil");
        let store_expiry_exists =
            store_expiry_exists.expect("Store expiry incorrectly returned nil");

        store_exists == 1 && store_expiry_exists == 1
    }

    async fn push_queue_ids(
        prefix: impl Into<String>,
        queue: &QueueControl,
        conn: &mut Connection,
        count: usize,
    ) {
        let prefix = prefix.into();

        let key = queue_ids_key(prefix);
        conn.del(&key)
            .await
            .expect(format!("Failed to delete {}", key).as_ref());

        let ids = generate_ids(&queue, count);

        conn.lpush(&key, &ids)
            .await
            .expect(format!("Failed to queue ids: {:?}", ids).as_ref());
    }

    async fn push_store_ids(
        prefix: impl Into<String>,
        queue: &QueueControl,
        conn: &mut Connection,
        count: usize,
    ) {
        let prefix = prefix.into();

        let key = store_ids_key(prefix);
        conn.del(&key)
            .await
            .expect(format!("Failed to delete {}", key).as_ref());

        let ids = generate_ids(&queue, count);

        conn.sadd(&key, &ids)
            .await
            .expect(format!("Failed to store ids: {:?}", ids).as_ref());
    }

    async fn hget_u64(conn: &mut Connection, key: &String, value: &String) -> u64 {
        let result: Option<String> = conn
            .hget(key, value)
            .await
            .expect(format!("failed to get fetch {}", key).as_ref());

        let parsed = match result {
            Some(e) => e
                .parse::<u64>()
                .expect(format!("failed to parse u64: {}", key).as_ref()),
            None => 0,
        };
        parsed
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

        QueueControl::new(pool, QUARANTINE, VALIDATED, EMIT_THROTTLE)
            .expect("QueueControl::new() failed");
    }

    async fn clean_keys(prefix: impl Into<String>) {
        let prefix = prefix.into();
        let (_, mut conn) = test_queue_conn().await;
        let keys = conn.keys(format!("{}:*", &prefix)).await.unwrap();
        for key in keys.iter() {
            conn.del(key).await.unwrap();
        }
    }

    #[tokio::test]
    #[traced_test]
    async fn test_init() {
        let prefix = "test_init";

        let queue = test_queue();
        queue
            .init(prefix, false, StoreCapacity::Unlimited)
            .await
            .expect("QueueControl::init() failed");

        clean_keys(prefix).await;
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

        let (queue, mut conn) = test_queue_conn().await;
        push_store_ids(prefix, &queue, &mut conn, expected_store_size).await;
        push_queue_ids(prefix, &queue, &mut conn, expected_queue_size).await;

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
        assert_ne!(result.updated, None);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_settings() {
        let prefix = "test_queue_settings_read";

        let expected_enabled: bool = true;
        let raw_capacity: isize = 4321;
        let expected_capacity = StoreCapacity::try_from(raw_capacity).unwrap();
        let expected_timestamp: i64 = 1757438630;

        let (queue, mut conn) = test_queue_conn().await;

        // Prepare keys
        conn.set(queue_enabled_key(prefix), expected_enabled)
            .await
            .expect("Failed to set ::queue_enabled");

        conn.set(store_capacity_key(prefix), raw_capacity)
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
        assert_eq!(result.updated.unwrap().timestamp(), expected_timestamp);

        clean_keys(prefix).await;
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
        assert_ne!(status.updated, None);

        clean_keys(prefix).await;
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
        assert_ne!(status.updated, None);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_enabled() {
        let prefix = "test_queue_enabled";

        let (queue, mut conn) = test_queue_conn().await;

        let key = queue_enabled_key(prefix);
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

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_queue_size() {
        let prefix = "test_queue_size";

        let (queue, mut conn) = test_queue_conn().await;

        let key = queue_ids_key(prefix);
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

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_store_capacity_sized() {
        let prefix = "test_store_capacity_sized";

        let (queue, mut conn) = test_queue_conn().await;

        let key = store_capacity_key(prefix);
        conn.set(&key, isize::MAX)
            .await
            .expect(format!("Failed to set {}", key).as_ref());

        let actual = queue
            .store_capacity(prefix)
            .await
            .expect("Failed to call store_capacity");

        let expected = StoreCapacity::Sized(isize::MAX as usize);
        assert_eq!(actual, expected);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_store_capacity_unlimited() {
        let prefix = "test_store_capacity_unlimited";

        let (queue, mut conn) = test_queue_conn().await;

        let key = store_capacity_key(prefix);
        conn.set(&key, -1)
            .await
            .expect(format!("Failed to set {}", key).as_ref());

        let actual = queue
            .store_capacity(prefix)
            .await
            .expect("Failed to call store_capacity");

        let expected = StoreCapacity::Unlimited;
        assert_eq!(actual, expected);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_store_size() {
        let prefix = "test_store_size";

        let (queue, mut conn) = test_queue_conn().await;

        let key = store_ids_key(prefix);
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

        clean_keys(prefix).await;
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

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_sync_keys_false() {
        let prefix = "test_sync_keys_false";

        let (queue, mut conn) = test_queue_conn().await;

        let keys = &[
            queue_enabled_key(prefix),
            store_capacity_key(prefix),
            format!("{}:queue_waiting_page", prefix),
            format!("{}:queue_sync_timestamp", prefix),
        ];

        conn.del(&keys).await.expect("Failed to delete keys");

        let actual = queue
            .check_sync_keys(prefix)
            .await
            .expect("Failed to call check_sync_keys");

        assert_eq!(actual, false);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_sync_keys_true() {
        let prefix = "test_sync_keys_true";

        let (queue, mut conn) = test_queue_conn().await;

        conn.set(queue_enabled_key(prefix), 1)
            .await
            .expect("Failed to set :queue_enabled");

        conn.set(store_capacity_key(prefix), 5)
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

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_true_queue() {
        let prefix = "test_has_ids_true_queue";

        let (queue, mut conn) = test_queue_conn().await;
        push_queue_ids(prefix, &queue, &mut conn, 1).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");

        assert!(actual);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_true_store() {
        let prefix = "test_has_ids_true_store";

        let (queue, mut conn) = test_queue_conn().await;
        push_store_ids(prefix, &queue, &mut conn, 1).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");
        assert_eq!(actual, true);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_true_both() {
        let prefix = "test_has_ids_true_both";

        let (queue, mut conn) = test_queue_conn().await;
        push_store_ids(prefix, &queue, &mut conn, 1).await;
        push_queue_ids(prefix, &queue, &mut conn, 1).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");
        assert_eq!(actual, true);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_has_ids_false() {
        let prefix = "test_has_ids_false";

        let (queue, mut conn) = test_queue_conn().await;
        clear_store(prefix, &mut conn).await;

        let actual = queue.has_ids(prefix).await.expect("Failed to call has_ids");
        assert_eq!(actual, false);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_id_position() {
        let prefix = "test_id_position";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 1
        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(1))
            .await
            .expect("Failed to set queue status");

        let count = 5;
        let ids = add_many(&queue, prefix, count, None).await;
        let first_id = &ids[0];
        let last_id = &ids[ids.len() - 1];

        // Check that the first ID is in the store
        let position = queue
            .id_position(prefix, *first_id, None)
            .await
            .expect("Failed to get first position");

        assert_eq!(position, QueuePosition::Store);

        // Check that the last ID is at the back of the line (queue positions are indexed from 1)
        let position = queue
            .id_position(prefix, *last_id, None)
            .await
            .expect("Failed to get last position");

        assert_eq!(position, QueuePosition::Queue(count - 1));

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_id_position_upgrade() {
        let prefix = "test_id_position_upgrade";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 1
        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(0))
            .await
            .expect("Failed to set queue status");

        let id = queue.new_id();
        let id_string = String::from(id);
        let time = DateTime::from_timestamp_secs(1758040541).expect("Failed to create timestamp");
        let redis_key = format!("{}:queue_expiry_secs", prefix);

        // Add item to the queue for quarantine
        queue
            .id_position(prefix, id, Some(time))
            .await
            .expect("Failed to add new ID to queue");

        let expiry = hget_u64(&mut conn, &redis_key, &id_string).await;
        assert_eq!(expiry, time.timestamp() as u64 + QUARANTINE.as_secs());

        // Fetch position a second time (upgrading the ID from quarantine to validated)
        queue
            .id_position(prefix, id, Some(time))
            .await
            .expect("Failed to add new ID to queue");

        let expiry = hget_u64(&mut conn, &redis_key, &id_string).await;
        assert_eq!(expiry, time.timestamp() as u64 + VALIDATED.as_secs());

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_id_remove_store() {
        let prefix = "test_id_remove_store";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 1
        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(1))
            .await
            .expect("Failed to set queue status");

        let count = 5;
        let ids = add_many(&queue, prefix, count, None).await;
        let store_id = &ids[0];
        let store_id_string = store_id.to_string();

        let exists = exists_in_store(prefix, &mut conn, store_id_string.clone()).await;
        assert_eq!(exists, true);

        // Remove first ID from store
        queue
            .id_remove(prefix, *store_id, None)
            .await
            .expect("Failed to removed first ID");

        let exists = exists_in_store(prefix, &mut conn, store_id_string.clone()).await;
        assert_eq!(exists, false);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_id_remove_queue() {
        let prefix = "test_id_remove_queue";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 1
        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(1))
            .await
            .expect("Failed to set queue status");

        let count = 5;
        let ids = add_many(&queue, prefix, count, None).await;
        let id = &ids[1]; // Index 1 is first item in queue
        let id_string = id.to_string();

        let exists: bool = conn
            .hexists(format!("{}:queue_expiry_secs", prefix), id_string.clone())
            .await
            .expect("Failed to fetch queue expiry value");

        assert_eq!(exists, true);

        let time = DateTime::from_timestamp_secs(175760525).expect("failed to create timestamp");

        // "Remove" ID from queue -- really just marks the ID as expired
        queue
            .id_remove(prefix, *id, Some(time))
            .await
            .expect("Failed to removed queue ID");

        let result: Option<String> = conn
            .hget(format!("{}:queue_expiry_secs", prefix), id_string.clone())
            .await
            .expect("Failed to fetch queue expiry value");

        let expiry_time = result.unwrap().parse::<u64>().unwrap();

        // Verify that the "removal" correctly set the time back by 1 second
        assert_eq!(expiry_time, time.timestamp() as u64 - 1);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_rotate_full_timeout() {
        let prefix = "test_rotate_full_timeout";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 1
        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(1))
            .await
            .expect("Failed to set queue status");

        let insert_time =
            DateTime::from_timestamp_secs(1757610168).expect("Failed to create timestamp");
        let rotate_time = insert_time + VALIDATED + Duration::from_secs(1);

        let count = 5;
        let _ = add_many(&queue, prefix, count, Some(insert_time)).await;

        let rotation = queue
            .rotate_full(prefix, Some(rotate_time))
            .await
            .expect("Failed to rotate");

        assert_eq!(rotation.queue_expired, count - 1);
        assert_eq!(rotation.store_expired, 1);
        assert_eq!(rotation.promoted, 0);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_rotate_full_promote() {
        let prefix = "test_rotate_full_promote";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 2
        let store_capacity = 2;

        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(store_capacity))
            .await
            .expect("Failed to set queue status");

        // Setup timeouts and counts
        let insert_time_a =
            DateTime::from_timestamp_secs(1757613534).expect("Failed to create timestamp");
        let insert_time_b = insert_time_a + (VALIDATED * 2);
        let rotate_time = insert_time_a + VALIDATED + Duration::from_secs(1);

        let initial_store_count = 3;
        let followup_queue_count = 5;

        // Add all items to the queue
        let _ = add_many(&queue, prefix, initial_store_count, Some(insert_time_a)).await;
        let _ = add_many(&queue, prefix, followup_queue_count, Some(insert_time_b)).await;

        // Rotate and ensure that all initial IDs are removed, with the followup IDs moved into
        // the store
        let rotation = queue
            .rotate_full(prefix, Some(rotate_time))
            .await
            .expect("Failed to rotate");

        assert_eq!(rotation.queue_expired, initial_store_count - store_capacity);
        assert_eq!(rotation.store_expired, store_capacity);
        assert_eq!(rotation.promoted, store_capacity);

        let queue_size = queue
            .queue_size(prefix)
            .await
            .expect("Failed to get queue size");
        assert_eq!(queue_size, followup_queue_count - store_capacity);

        let store_size = queue
            .store_size(prefix)
            .await
            .expect("Failed to get store size");
        assert_eq!(store_size, store_capacity);

        clean_keys(prefix).await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_rotate_expire() {
        let prefix = "test_rotate_expire";

        let (queue, mut conn) = test_queue_conn().await;

        // Clear store and initialize store capacity to 1
        clear_store(prefix, &mut conn).await;
        queue
            .set_queue_settings(prefix, true, StoreCapacity::Sized(1))
            .await
            .expect("Failed to set queue status");

        let insert_time =
            DateTime::from_timestamp_secs(1757610168).expect("Failed to create timestamp");
        let rotate_time = insert_time + VALIDATED + Duration::from_secs(1);

        let count = 5;
        let _ = add_many(&queue, prefix, count, Some(insert_time)).await;

        let rotation = queue
            .rotate_expire(prefix, Some(rotate_time))
            .await
            .expect("Failed to rotate");

        assert_eq!(rotation.queue_expired, count - 1);
        assert_eq!(rotation.store_expired, 1);
        assert_eq!(rotation.promoted, 0);

        clean_keys(prefix).await;
    }
}
