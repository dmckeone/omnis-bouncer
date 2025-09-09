use deadpool_redis::{redis, Connection};
use redis::{pipe, Script};
use uuid::Uuid;

use crate::constants::REDIS_FUNCTIONS_DIR;
use crate::database::current_time;
use crate::errors::{Error, Result};
use crate::queue::models::QueueRotate;

pub(crate) struct Scripts {
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
    pub(crate) fn new() -> Result<Self> {
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

    pub(crate) async fn init(&self, conn: &mut Connection) -> Result<()> {
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
    pub(crate) async fn check_sync_keys(&self, conn: &mut Connection) -> Result<bool> {
        match self.check_sync_keys.invoke_async(conn).await? {
            1 => Ok(true),
            0 => Ok(false),
            val => {
                let msg = format!("Unexpected result from \"check_sync_keys\": {}", val);
                Err(Error::ScriptError(msg))
            }
        }
    }

    /// Return true if the store or queue has any UUIDs, false if both the queue and store are empty
    pub(crate) async fn has_ids(&self, conn: &mut Connection) -> Result<bool> {
        match self.has_ids.invoke_async(conn).await? {
            1 => Ok(true),
            0 => Ok(false),
            val => {
                let msg = format!("Unexpected result from \"has_ids\": {}", val);
                Err(Error::ScriptError(msg))
            }
        }
    }

    /// Add a UUID to the queue/store with expiration times, returning queue position
    pub(crate) async fn id_add(
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
    pub(crate) async fn id_position(
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
    pub(crate) async fn id_remove(
        &self,
        conn: &mut Connection,
        prefix: String,
        id: Uuid,
    ) -> Result<()> {
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
    pub(crate) async fn queue_timeout(
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
    pub(crate) async fn store_promote(
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
    pub(crate) async fn store_timeout(
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
    pub(crate) async fn rotate_full(
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
    pub(crate) async fn rotate_expire(
        &self,
        conn: &mut Connection,
        prefix: String,
    ) -> Result<QueueRotate> {
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

mod test {}
