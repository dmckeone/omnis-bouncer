use deadpool_redis::{redis, Connection, Pool as RedisPool};
use redis::{pipe, AsyncTypedCommands, Script};
use tracing::warn;
use uuid::Uuid;

use crate::constants::REDIS_FUNCTIONS_DIR;
use crate::database::{current_time, get_connection};
use crate::errors::{Error, Result};

pub struct QueueRotate {
    pub store_removed: usize,
    pub moved: usize,
    pub queue_removed: usize,
}

/// All Function
struct Scripts {
    check_sync_keys: Script,
    has_ids: Script,
    id_add: Script,
    id_position: Script,
    id_remove: Script,
    queue_timeout: Script,
    store_promote: Script,
    store_timeout: Script,
}

impl Scripts {
    /// Load a single embedded script from this package
    fn read(name: &str) -> Result<Script> {
        let file_name = format!("{}.lua", name);
        let file = match REDIS_FUNCTIONS_DIR.get_file(file_name) {
            Some(f) => f,
            None => {
                let msg = format!("Unable to read embedded script: {}", name);
                return Err(Error::ScriptError(msg));
            }
        };
        let contents = match file.contents_utf8() {
            Some(c) => c,
            None => {
                let msg = format!("Unable to read embedded script: {}", name);
                return Err(Error::ScriptError(msg));
            }
        };
        let script = Script::new(contents);
        Ok(script)
    }

    /// Create a new scripts instance with all script instances parsed and loaded
    fn new() -> Result<Self> {
        let functions = Self {
            check_sync_keys: Self::read("check_sync_keys")?,
            has_ids: Self::read("has_ids")?,
            id_add: Self::read("id_add")?,
            id_position: Self::read("id_position")?,
            id_remove: Self::read("id_remove")?,
            queue_timeout: Self::read("queue_timeout")?,
            store_promote: Self::read("store_promote")?,
            store_timeout: Self::read("store_timeout")?,
        };

        Ok(functions)
    }

    async fn init(&self, conn: &mut Connection) -> Result<()> {
        self.check_sync_keys.load_async(conn).await?;
        self.has_ids.load_async(conn).await?;
        self.id_add.load_async(conn).await?;
        self.id_position.load_async(conn).await?;
        self.id_remove.load_async(conn).await?;
        self.queue_timeout.load_async(conn).await?;
        self.store_promote.load_async(conn).await?;
        self.store_timeout.load_async(conn).await?;
        Ok(())
    }

    /// Check that all keys required for syncing the queue/store are available
    async fn check_sync_keys(&self, conn: &mut Connection) -> Result<bool> {
        let result = self.check_sync_keys.invoke_async(conn).await?;

        match result {
            1 => Ok(true),
            0 => Ok(false),
            val => {
                let msg = format!("Unexpected result from \"check_sync_keys\": {}", val);
                Err(Error::ScriptError(msg))
            }
        }
    }

    /// Return true if the store or queue has any UUIDs, false if both the queue and store are empty
    async fn has_ids(&self, conn: &mut Connection) -> Result<bool> {
        let result = self.has_ids.invoke_async(conn).await?;

        match result {
            1 => Ok(true),
            0 => Ok(false),
            val => {
                let msg = format!("Unexpected result from \"has_ids\": {}", val);
                Err(Error::ScriptError(msg))
            }
        }
    }

    /// Add a UUID to the queue/store with expiration times, returning queue position
    async fn id_add(
        &self,
        conn: &mut Connection,
        prefix: String,
        id: Uuid,
        time: usize,
        validated_expiry: usize,
        quarantine_expiry: usize,
    ) -> Result<usize> {
        let position = self
            .id_add
            .arg(prefix)
            .arg(String::from(id))
            .arg(time)
            .arg(validated_expiry)
            .arg(quarantine_expiry)
            .invoke_async(conn)
            .await?;

        Ok(position)
    }

    /// Return the position of a UUID in the queue, or add the UUID to the queue and then
    /// return the position if the UUID does not already exist in the queue
    async fn id_position(
        &self,
        conn: &mut Connection,
        prefix: String,
        id: Uuid,
        time: usize,
        validated_expiry: usize,
        quarantine_expiry: usize,
    ) -> Result<usize> {
        let position = self
            .id_position
            .arg(prefix)
            .arg(String::from(id))
            .arg(time)
            .arg(validated_expiry)
            .arg(quarantine_expiry)
            .invoke_async(conn)
            .await?;

        Ok(position)
    }

    /// Remove a given UUID from the queue/store
    async fn id_remove(&self, conn: &mut Connection, prefix: String, id: Uuid) -> Result<()> {
        let _: i64 = self
            .id_remove
            .arg(prefix)
            .arg(String::from(id))
            .invoke_async(conn)
            .await?;

        Ok(())
    }

    /// Remove timed out UUIDs from the queue, based on the current_time
    /// from [TIME](https://redis.io/docs/latest/commands/time/)
    async fn queue_timeout(
        &self,
        conn: &mut Connection,
        prefix: String,
        current_time: u64,
    ) -> Result<u64> {
        let position = self
            .queue_timeout
            .arg(prefix)
            .arg(current_time)
            .invoke_async(conn)
            .await?;

        Ok(position)
    }

    /// Promote an integer number of UUIDs from the queue into the store
    async fn store_promote(
        &self,
        conn: &mut Connection,
        prefix: String,
        batch_size: u64,
    ) -> Result<u64> {
        let position = self
            .store_promote
            .arg(prefix)
            .arg(batch_size)
            .invoke_async(conn)
            .await?;

        Ok(position)
    }

    /// Remove timed out UUIDs from the store, based on the current_time
    /// from [TIME](https://redis.io/docs/latest/commands/time/)
    async fn store_timeout(
        &self,
        conn: &mut Connection,
        prefix: String,
        current_time: u64,
    ) -> Result<u64> {
        let position = self
            .store_timeout
            .arg(prefix)
            .arg(current_time)
            .invoke_async(conn)
            .await?;

        Ok(position)
    }

    /// Full queue rotation using scripts in a pipeline
    async fn rotate_full(
        &self,
        conn: &mut Connection,
        prefix: String,
        batch_size: usize,
    ) -> Result<QueueRotate> {
        let current_time = current_time(conn).await?;
        let (store_removed, moved, queue_removed) = pipe()
            .invoke_script(self.store_timeout.arg(&prefix).arg(current_time))
            .invoke_script(self.store_promote.arg(&prefix).arg(batch_size))
            .invoke_script(self.queue_timeout.arg(&prefix).arg(current_time))
            .query_async(conn)
            .await?;

        Ok(QueueRotate {
            store_removed,
            moved,
            queue_removed,
        })
    }

    /// Partial queue rotation that only expires IDs, but doesn't promote IDs from queue to store
    async fn rotate_expire(&self, conn: &mut Connection, prefix: String) -> Result<QueueRotate> {
        let current_time = current_time(conn).await?;
        let (store_removed, queue_removed) = pipe()
            .invoke_script(self.store_timeout.arg(&prefix).arg(current_time))
            .invoke_script(self.queue_timeout.arg(&prefix).arg(current_time))
            .query_async(conn)
            .await?;

        Ok(QueueRotate {
            store_removed,
            moved: 0,
            queue_removed,
        })
    }
}

fn to_redis_bool(val: bool) -> isize {
    match val {
        true => 1,
        false => 0,
    }
}

fn from_redis_bool(val: isize, default: bool) -> bool {
    match val {
        1 => true,
        0 => false,
        _ => default,
    }
}

pub enum StoreCapacity {
    Sized(usize),
    Unlimited,
}

impl From<isize> for StoreCapacity {
    fn from(size: isize) -> Self {
        match size {
            ..-1 => {
                warn!("::store capacity incorrectly set below -1");
                StoreCapacity::Unlimited
            }
            -1 => StoreCapacity::Unlimited,
            0.. => StoreCapacity::Sized(size as usize),
        }
    }
}

impl Into<isize> for StoreCapacity {
    fn into(self) -> isize {
        match self {
            StoreCapacity::Sized(size) => size as isize,
            StoreCapacity::Unlimited => -1,
        }
    }
}

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
    pub async fn set_queue_status(
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
    pub async fn queue_enabled(&self, prefix: impl Into<String>) -> Result<bool> {
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
            Some(r) => StoreCapacity::from(r.parse::<isize>()?),
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
    pub async fn id_position(
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
    pub async fn id_remove(&self, prefix: String, id: Uuid) -> Result<()> {
        let mut conn = self.conn().await?;
        self.scripts.id_remove(&mut conn, prefix, id).await
    }

    /// Full queue rotation using scripts in a pipeline
    pub async fn rotate_full(&self, prefix: String, batch_size: usize) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        self.scripts
            .rotate_full(&mut conn, prefix, batch_size)
            .await
    }

    /// Partial queue rotation that only expires IDs, but doesn't promote IDs from queue to store
    pub async fn rotate_expire(&self, prefix: String) -> Result<QueueRotate> {
        let mut conn = self.conn().await?;
        self.scripts.rotate_expire(&mut conn, prefix).await
    }
}
