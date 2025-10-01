use chrono::{DateTime, Utc};
use deadpool_redis::{Config, Connection, Pool, Runtime, redis::cmd};
use futures_util::StreamExt;
use redis::Client;
use std::sync::Arc;
use tokio::select;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::{Notify, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use tracing::error;

use crate::errors::{Error, Result};

pub fn create_redis_client(uri: impl Into<String>) -> Result<Client> {
    let uri = uri.into();
    let client = Client::open(uri)?;
    Ok(client)
}

#[derive(Debug, Clone)]
pub struct RedisSubscriber {
    sender: Arc<Sender<String>>,
}

impl RedisSubscriber {
    pub async fn from_client(
        client: Client,
        channel_name: String,
        cancel: Arc<Notify>,
    ) -> Result<RedisSubscriber> {
        let (sender, receiver) = broadcast::channel(50);
        let sender = Arc::new(sender);

        let (mut sink, mut stream) = client.get_async_pubsub().await?.split();
        sink.subscribe(&channel_name).await?;

        let task_sender = sender.clone();
        tokio::spawn(async move {
            let _receiver = receiver; // Move receiver into task to keep it alive for this lifetime
            loop {
                select!(
                    _ = cancel.notified() => break,
                    msg = stream.next() => {
                        let msg = match msg {
                            Some(m) => m,
                            None => continue
                        };

                        let payload: String = match msg.get_payload() {
                            Ok(p) => p,
                            Err(error) => {
                                error!("Failed to read Redis subscriber payload: {:?}", error);
                                continue;
                            }
                        };

                        if let Err(error) = task_sender.send(payload.clone()) {
                            error!("Failed to emit broadcast Redis subscriber payload \"{:?}\": {:?}", payload, error);
                        }
                    },
                );
            }
        });

        Ok(Self { sender })
    }

    pub fn receiver(&self) -> Receiver<String> {
        self.sender.subscribe()
    }

    pub fn stream(&self) -> BroadcastStream<String> {
        BroadcastStream::new(self.receiver())
    }
}

pub fn create_redis_pool(uri: impl Into<String>) -> Result<Pool> {
    let cfg = Config::from_url(uri.into());
    let pool = cfg.create_pool(Some(Runtime::Tokio1))?;
    Ok(pool)
}

pub async fn get_connection(pool: &Pool) -> Result<Connection> {
    Ok(pool.get().await?)
}

// Get current time from server
pub async fn current_time(conn: &mut Connection) -> Result<DateTime<Utc>> {
    let result: (Option<i64>, Option<u32>) = cmd("TIME").query_async(conn).await?;
    let seconds = result.0.ok_or(Error::RedisTimeIsNil)?;
    let nanoseconds = result.1.ok_or(Error::RedisTimeIsNil)?;
    let datetime = DateTime::from_timestamp(seconds, nanoseconds).ok_or(Error::RedisTimeIsNil)?;
    Ok(datetime)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use std::env;
    use tracing::warn;

    pub fn create_test_pool() -> Option<Pool> {
        let uri = match env::var("TEST_REDIS_URI") {
            Ok(u) => u,
            Err(e) => {
                warn!("TEST_REDIS_URI error: {:?}", e);
                return None;
            }
        };

        let uri = uri.trim();

        let pool = match create_redis_pool(uri.trim()) {
            Ok(p) => p,
            Err(e) => {
                warn!("Redis server not available: {:?}", e);
                return None;
            }
        };

        Some(pool)
    }
}
