use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::queue::{QueueEvent, QueueSettings, QueueStatus};

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
            updated: settings.updated,
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
            updated: status.updated,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Event {
    #[serde(rename = "settings:updated")]
    SettingsChanged,
    #[serde(rename = "waiting_page:updated")]
    WaitingPageChanged,
    #[serde(rename = "queue:added")]
    QueueAdded,
    #[serde(rename = "queue:expired")]
    QueueExpired,
    #[serde(rename = "queue:removed")]
    QueueRemoved,
    #[serde(rename = "store:added")]
    StoreAdded,
    #[serde(rename = "store:expired")]
    StoreExpired,
}

impl From<QueueEvent> for Event {
    fn from(queue_event: QueueEvent) -> Self {
        match queue_event {
            QueueEvent::SettingsChanged => Self::SettingsChanged,
            QueueEvent::WaitingPageChanged => Self::WaitingPageChanged,
            QueueEvent::QueueAdded => Self::QueueAdded,
            QueueEvent::QueueExpired => Self::QueueExpired,
            QueueEvent::StoreAdded => Self::StoreAdded,
            QueueEvent::StoreExpired => Self::StoreExpired,
            QueueEvent::QueueRemoved => Self::QueueRemoved,
        }
    }
}
