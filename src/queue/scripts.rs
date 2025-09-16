use deadpool_redis::{redis, Connection};
use redis::{pipe, AsyncTypedCommands, Script};
use std::time::Duration;
use uuid::Uuid;

use crate::constants::REDIS_FUNCTIONS_DIR;
use crate::database::current_time;
use crate::errors::{Error, Result};
use crate::queue::models::{QueueRotate, StoreCapacity};

pub struct Scripts {
    check_sync_keys: Script,
    has_ids: Script,
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
        let Some(file) = REDIS_FUNCTIONS_DIR.get_file(file_name) else {
            return Err(Error::RedisScriptUnreadable(String::from(name)));
        };
        let Some(contents) = file.contents_utf8() else {
            return Err(Error::RedisScriptUnreadable(String::from(name)));
        };
        let script = Script::new(contents);
        Ok(script)
    }

    /// Create a new scripts instance with all script instances parsed and loaded
    pub fn new() -> Result<Self> {
        let functions = Self {
            check_sync_keys: Self::read("check_sync_keys")?,
            has_ids: Self::read("has_ids")?,
            id_position: Self::read("id_position")?,
            id_remove: Self::read("id_remove")?,
            queue_timeout: Self::read("queue_timeout")?,
            store_promote: Self::read("store_promote")?,
            store_timeout: Self::read("store_timeout")?,
        };

        Ok(functions)
    }

    pub async fn init(&self, conn: &mut Connection) -> Result<()> {
        self.check_sync_keys.load_async(conn).await?;
        self.has_ids.load_async(conn).await?;
        self.id_position.load_async(conn).await?;
        self.id_remove.load_async(conn).await?;
        self.queue_timeout.load_async(conn).await?;
        self.store_promote.load_async(conn).await?;
        self.store_timeout.load_async(conn).await?;
        Ok(())
    }

    /// Check that all keys required for syncing the queue/store are available
    pub async fn check_sync_keys(
        &self,
        conn: &mut Connection,
        prefix: impl Into<String>,
    ) -> Result<bool> {
        let prefix = prefix.into();
        match self.check_sync_keys.arg(&prefix).invoke_async(conn).await? {
            1 => Ok(true),
            0 => Ok(false),
            val => {
                let msg = format!("Unexpected result from \"check_sync_keys\": {}", val);
                Err(Error::RedisScriptUnreadable(msg))
            }
        }
    }

    /// Return true if the store or queue has any UUIDs, false if both the queue and store are empty
    pub async fn has_ids(&self, conn: &mut Connection, prefix: impl Into<String>) -> Result<bool> {
        let prefix = prefix.into();
        match self.has_ids.arg(&prefix).invoke_async(conn).await? {
            1 => Ok(true),
            0 => Ok(false),
            val => {
                let msg = format!("Unexpected result from \"has_ids\": {}", val);
                Err(Error::RedisScriptUnreadable(msg))
            }
        }
    }

    /// Return the position of a UUID in the queue, or add the UUID to the queue and then
    /// return the position if the UUID does not already exist in the queue
    pub async fn id_position(
        &self,
        conn: &mut Connection,
        prefix: impl Into<String>,
        id: Uuid,
        time: u64,
        validated_expiry: Duration,
        quarantine_expiry: Duration,
    ) -> Result<usize> {
        let prefix = prefix.into();
        let position = self
            .id_position
            .arg(prefix)
            .arg(String::from(id))
            .arg(time)
            .arg(validated_expiry.as_secs())
            .arg(quarantine_expiry.as_secs())
            .invoke_async(conn)
            .await?;

        Ok(position)
    }

    /// Remove a given UUID from the queue/store
    pub async fn id_remove(
        &self,
        conn: &mut Connection,
        prefix: impl Into<String>,
        id: Uuid,
        time: u64,
    ) -> Result<()> {
        let prefix = prefix.into();
        let _: Option<String> = self
            .id_remove
            .arg(&prefix)
            .arg(String::from(id))
            .arg(time)
            .invoke_async(conn)
            .await?;

        Ok(())
    }

    /// Full queue/store timeout eviction with queue to store promotion
    pub async fn rotate_full(
        &self,
        conn: &mut Connection,
        prefix: impl Into<String>,
        time: Option<u64>,
    ) -> Result<QueueRotate> {
        let prefix = prefix.into();

        let time = match time {
            Some(t) => t,
            None => current_time(conn).await?,
        };

        // Run eviction scripts and fetch the new sizes and capacity
        type Result = (
            Option<usize>,
            Option<usize>,
            Option<isize>,
            Option<usize>,
            Option<usize>,
        );
        let result: Result = pipe()
            .atomic()
            .invoke_script(self.store_timeout.arg(&prefix).arg(time))
            .invoke_script(self.queue_timeout.arg(&prefix).arg(time))
            .get(format!("{}:store_capacity", prefix))
            .llen(format!("{}:queue_ids", prefix))
            .scard(format!("{}:store_ids", prefix))
            .query_async(conn)
            .await?;

        // Unpack the results
        let store_removed = result.0.unwrap_or(0);
        let queue_removed = result.1.unwrap_or(0);
        let store_capacity = StoreCapacity::try_from(result.2)?;
        let queue_size = result.3.unwrap_or(0);
        let store_size = result.4.unwrap_or(0);

        // Determine transfer size
        let transfer_size = match store_capacity {
            StoreCapacity::Sized(capacity) => capacity - store_size,
            StoreCapacity::Unlimited => queue_size,
        };

        // Transfer items from queue to store
        let promoted = conn
            .invoke_script(self.store_promote.arg(&prefix).arg(transfer_size))
            .await?;

        let rotate = QueueRotate {
            queue_removed,
            store_removed,
            promoted,
        };

        Ok(rotate)
    }

    /// Partial queue rotation that only expires IDs, but doesn't promote IDs from queue to store
    pub async fn rotate_expire(
        &self,
        conn: &mut Connection,
        prefix: impl Into<String>,
        time: Option<u64>,
    ) -> Result<QueueRotate> {
        let prefix = prefix.into();

        let time = match time {
            Some(t) => t,
            None => current_time(conn).await?,
        };

        let (store_removed, queue_removed) = pipe()
            .atomic()
            .invoke_script(self.store_timeout.arg(&prefix).arg(time))
            .invoke_script(self.queue_timeout.arg(&prefix).arg(time))
            .query_async(conn)
            .await?;

        let rotate = QueueRotate {
            queue_removed,
            store_removed,
            promoted: 0,
        };

        Ok(rotate)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_construct() {
        match Scripts::new() {
            Ok(_) => assert!(true),
            Err(e) => panic!("Script::new Error: {:?}", e),
        }
    }

    #[test]
    fn test_read_scripts() {
        let scripts = &[
            "check_sync_keys",
            "has_ids",
            "id_position",
            "id_remove",
            "queue_timeout",
            "store_promote",
            "store_timeout",
        ];

        for script in scripts {
            match Scripts::read(script) {
                Ok(_) => assert!(true),
                _ => panic!("Script Error"),
            }
        }
    }

    #[test]
    fn test_read_script_error() {
        match Scripts::read("not_a_real_script") {
            Err(Error::RedisScriptUnreadable(..)) => assert!(true),
            _ => panic!("Read script that shouldn't be readable"),
        };
    }
}
