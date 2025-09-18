use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::queue::{QueueEvent, QueueSettings, QueueStatus};

#[derive(Debug, Serialize)]
pub struct Info {
    pub name: String,
}

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

#[derive(Debug, Clone)]
pub enum Event {
    SettingsChanged,
    WaitingPageChanged,
    QueueAdded,
    QueueExpired,
    QueueRemoved,
    StoreAdded,
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

impl From<Event> for String {
    fn from(event: Event) -> Self {
        match event {
            Event::SettingsChanged => String::from("settings:updated"),
            Event::WaitingPageChanged => String::from("waiting_page:updated"),
            Event::QueueAdded => String::from("queue:added"),
            Event::QueueExpired => String::from("queue:expired"),
            Event::StoreAdded => String::from("store:added"),
            Event::StoreExpired => String::from("store:expired"),
            Event::QueueRemoved => String::from("queue:removed"),
        }
    }
}
