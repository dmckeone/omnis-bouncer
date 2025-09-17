use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::queue::{QueueSettings, QueueStatus};

#[derive(Debug, Serialize)]
pub struct Settings {
    pub queue_enabled: bool,
    pub store_capacity: isize,
    pub updated: Option<DateTime<Utc>>,
}

impl From<QueueSettings> for Settings {
    fn from(settings: QueueSettings) -> Self {
        Self {
            queue_enabled: settings.enabled,
            store_capacity: settings.capacity.into(),
            updated: DateTime::from_timestamp_secs(settings.updated as i64),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SettingsPatch {
    #[serde(default)]
    pub queue_enabled: Option<bool>,
    #[serde(default)]
    pub store_capacity: Option<isize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Status {
    pub queue_enabled: bool,
    pub store_capacity: isize,
    pub queue_size: usize,
    pub store_size: usize,
    pub updated: Option<DateTime<Utc>>,
}

impl From<QueueStatus> for Status {
    fn from(status: QueueStatus) -> Self {
        Self {
            queue_enabled: status.enabled,
            store_capacity: status.capacity.into(),
            queue_size: status.queue_size,
            store_size: status.store_size,
            updated: DateTime::from_timestamp_secs(status.updated as i64),
        }
    }
}
