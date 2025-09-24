use chrono::{DateTime, Utc};
use std::default::Default;
use tracing::error;

use crate::errors::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueSettings {
    pub enabled: bool,
    pub capacity: StoreCapacity,
    pub updated: Option<DateTime<Utc>>,
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            capacity: StoreCapacity::Unlimited,
            updated: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueStatus {
    pub enabled: bool,
    pub capacity: StoreCapacity,
    pub queue_size: usize,
    pub store_size: usize,
    pub updated: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueRotate {
    pub queue_expired: usize,
    pub store_expired: usize,
    pub promoted: usize,
}

impl QueueRotate {
    pub fn new(queue_removed: usize, store_removed: usize, promoted: usize) -> Self {
        Self {
            queue_expired: queue_removed,
            store_expired: store_removed,
            promoted,
        }
    }

    pub fn has_changes(&self) -> bool {
        self.queue_expired > 0 || self.store_expired > 0 || self.promoted > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuePosition {
    NotPresent,
    Store,
    Queue(usize),
}

impl QueuePosition {
    pub fn from_redis(status: usize, position: usize) -> Self {
        match status {
            0 => Self::NotPresent,
            1..=2 => match position {
                0 => QueuePosition::Store,
                1.. => QueuePosition::Queue(position),
            },
            _ => {
                error!("Invalid queue position status: {}", status);
                QueuePosition::NotPresent
            }
        }
    }
}

impl From<isize> for QueuePosition {
    fn from(value: isize) -> Self {
        match value {
            ..=-1 => QueuePosition::NotPresent,
            0 => QueuePosition::Store,
            1.. => QueuePosition::Queue(value as usize),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreCapacity {
    Sized(usize),
    Unlimited,
}

impl TryFrom<Option<String>> for StoreCapacity {
    type Error = Error;

    fn try_from(size: Option<String>) -> Result<Self> {
        match size {
            Some(c) => match c.parse::<isize>() {
                Ok(v) => Self::try_from(v),
                Err(_) => Err(Error::StoreCapacityOutOfRange(c)),
            },
            None => Ok(Self::Unlimited),
        }
    }
}

impl TryFrom<Option<isize>> for StoreCapacity {
    type Error = Error;

    fn try_from(size: Option<isize>) -> Result<Self> {
        match size {
            Some(c) => Self::try_from(c),
            None => Ok(Self::Unlimited),
        }
    }
}

impl TryFrom<isize> for StoreCapacity {
    type Error = Error;

    fn try_from(size: isize) -> Result<Self> {
        match size {
            ..-1 => Err(Error::StoreCapacityOutOfRange(size.to_string())),
            -1 => Ok(Self::Unlimited),
            0.. => Ok(Self::Sized(size as usize)),
        }
    }
}

impl TryFrom<&str> for StoreCapacity {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::try_from(match value.parse::<isize>() {
            Ok(v) => v,
            Err(_) => {
                return Err(Error::StoreCapacityOutOfRange(String::from(value)));
            }
        })
    }
}

impl TryFrom<String> for StoreCapacity {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::try_from(value.as_ref())
    }
}

impl From<StoreCapacity> for isize {
    fn from(val: StoreCapacity) -> isize {
        match val {
            StoreCapacity::Sized(size) => size as isize,
            StoreCapacity::Unlimited => -1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueueEnabled(pub bool);

impl From<bool> for QueueEnabled {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

impl From<QueueEnabled> for bool {
    fn from(value: QueueEnabled) -> Self {
        value.0
    }
}

impl TryFrom<&str> for QueueEnabled {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        match value.trim() {
            "1" => Ok(Self(true)),
            "0" => Ok(Self(false)),
            _ => Err(Error::QueueEnabledOutOfRange(String::from(value))),
        }
    }
}

impl TryFrom<Option<String>> for QueueEnabled {
    type Error = Error;
    fn try_from(value: Option<String>) -> Result<Self> {
        match value {
            Some(v) => match v.parse::<isize>() {
                Ok(e) => Self::try_from(e),
                Err(_) => Err(Error::QueueEnabledOutOfRange(v)),
            },
            None => Ok(Self(false)),
        }
    }
}

impl TryFrom<String> for QueueEnabled {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        Self::try_from(value.as_ref())
    }
}

impl TryFrom<Option<isize>> for QueueEnabled {
    type Error = Error;

    fn try_from(value: Option<isize>) -> Result<Self> {
        match value {
            Some(v) => Self::try_from(v),
            None => Ok(Self(false)),
        }
    }
}

impl TryFrom<isize> for QueueEnabled {
    type Error = Error;
    fn try_from(value: isize) -> Result<Self> {
        match value {
            1 => Ok(Self(true)),
            0 => Ok(Self(false)),
            _ => Err(Error::QueueEnabledOutOfRange(value.to_string())),
        }
    }
}

impl From<QueueEnabled> for isize {
    fn from(value: QueueEnabled) -> Self {
        match value.0 {
            true => 1,
            false => 0,
        }
    }
}

impl From<QueueEnabled> for String {
    fn from(value: QueueEnabled) -> Self {
        match value.0 {
            true => String::from("1"),
            false => String::from("0"),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum QueueEvent {
    SettingsChanged,
    WaitingPageChanged,
    QueueAdded,
    QueueExpired,
    QueueRemoved,
    StoreAdded,
    StoreExpired,
}

impl From<QueueEvent> for String {
    fn from(event: QueueEvent) -> Self {
        match event {
            QueueEvent::SettingsChanged => String::from("settings:updated"),
            QueueEvent::WaitingPageChanged => String::from("waiting_page:updated"),
            QueueEvent::QueueAdded => String::from("queue:added"),
            QueueEvent::QueueExpired => String::from("queue:expired"),
            QueueEvent::StoreAdded => String::from("store:added"),
            QueueEvent::StoreExpired => String::from("store:expired"),
            QueueEvent::QueueRemoved => String::from("queue:removed"),
        }
    }
}

impl TryFrom<&str> for QueueEvent {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        match value {
            "settings:updated" => Ok(QueueEvent::SettingsChanged),
            "waiting_page:updated" => Ok(QueueEvent::WaitingPageChanged),
            "queue:added" => Ok(QueueEvent::QueueAdded),
            "queue:expired" => Ok(QueueEvent::QueueExpired),
            "store:added" => Ok(QueueEvent::StoreAdded),
            "store:expired" => Ok(QueueEvent::StoreExpired),
            "queue:removed" => Ok(QueueEvent::QueueRemoved),
            _ => Err(Error::RedisEventUnknown(String::from(value))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    mod store_capacity {
        use super::*;

        #[test]
        fn test_store_capacity_from_isize_error() {
            let expected: isize = -2;
            let actual = StoreCapacity::try_from(expected);
            match actual {
                Err(Error::StoreCapacityOutOfRange(v)) => assert_eq!(v, String::from("-2")),
                _ => panic!("Should have emitted capacity error"),
            }
        }

        #[test]
        fn test_store_capacity_from_isize_unlimited() {
            let value: isize = -1;
            let actual = StoreCapacity::try_from(value).unwrap();
            assert_eq!(actual, StoreCapacity::Unlimited);
        }

        #[test]
        fn test_store_capacity_from_isize_sized() {
            let value: isize = isize::MAX;
            let actual = StoreCapacity::try_from(value).unwrap();
            assert_eq!(actual, StoreCapacity::Sized(isize::MAX as usize));
        }

        #[test]
        fn test_store_capacity_from_string_error() {
            let expected = "-2";
            let actual = StoreCapacity::try_from(expected);
            match actual {
                Err(Error::StoreCapacityOutOfRange(v)) => assert_eq!(v, String::from("-2")),
                _ => panic!("Should have emitted capacity error"),
            }
        }

        #[test]
        fn test_store_capacity_from_string_unlimited() {
            let value = "-1";
            let actual = StoreCapacity::try_from(value).unwrap();
            assert_eq!(actual, StoreCapacity::Unlimited);
        }

        #[test]
        fn test_store_capacity_from_string_sized() {
            let value = isize::MAX.to_string();
            let actual = StoreCapacity::try_from(value).unwrap();
            assert_eq!(actual, StoreCapacity::Sized(isize::MAX as usize));
        }

        #[test]
        fn test_store_capacity_none_unlimited() {
            let v: Option<String> = None;
            match StoreCapacity::try_from(v) {
                Ok(StoreCapacity::Unlimited) => assert!(true),
                _ => assert!(false),
            }
        }
    }

    mod queue_enabled {
        use super::*;

        #[test]
        fn test_isize_from_queue_enabled_false() {
            assert_eq!(isize::from(QueueEnabled(false)), 0);
        }

        #[test]
        fn test_isize_from_queue_enabled_true() {
            assert_eq!(isize::from(QueueEnabled(true)), 1);
        }

        #[test]
        fn test_queue_enabled_from_isize_zero() {
            assert_eq!(QueueEnabled::try_from(0).unwrap(), QueueEnabled(false));
        }

        #[test]
        fn test_queue_enabled_from_isize_one() {
            assert_eq!(QueueEnabled::try_from(1).unwrap(), QueueEnabled(true));
        }

        #[test]
        fn test_queue_enabled_from_isize_error() {
            match QueueEnabled::try_from(2) {
                Err(Error::QueueEnabledOutOfRange(_)) => assert!(true),
                _ => assert!(false),
            }
        }

        #[test]
        fn test_queue_enabled_from_string_zero() {
            assert_eq!(QueueEnabled::try_from("0").unwrap(), QueueEnabled(false));
        }

        #[test]
        fn test_queue_enabled_from_string_one() {
            assert_eq!(QueueEnabled::try_from("1").unwrap(), QueueEnabled(true));
        }

        #[test]
        fn test_queue_enabled_from_string_error() {
            match QueueEnabled::try_from("2") {
                Err(Error::QueueEnabledOutOfRange(_)) => assert!(true),
                _ => assert!(false),
            }
        }

        #[test]
        fn test_queue_sync_timestamp_from_option_string_none() {
            let v: Option<String> = None;
            match QueueEnabled::try_from(v) {
                Ok(v) => assert_eq!(v, QueueEnabled(false)),
                _ => assert!(false),
            }
        }

        #[test]
        fn test_string_from_queue_enabled_false() {
            assert_eq!(String::from(QueueEnabled(false)), "0");
        }

        #[test]
        fn test_string_from_queue_enabled_true() {
            assert_eq!(String::from(QueueEnabled(true)), "1");
        }

        #[test]
        fn test_bool_from_queue_enabled_true() {
            assert_eq!(bool::from(QueueEnabled(true)), true);
        }

        #[test]
        fn test_bool_from_queue_enabled_false() {
            assert_eq!(bool::from(QueueEnabled(false)), false);
        }
    }
}
