use crate::constants::ERROR_NULL_STRING;
use crate::errors::{Error, Result};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct QueueSettings {
    pub enabled: bool,
    pub capacity: StoreCapacity,
    pub sync_timestamp: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct QueueStatus {
    pub enabled: bool,
    pub capacity: StoreCapacity,
    pub queue_size: usize,
    pub store_size: usize,
    pub sync_timestamp: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct QueueRotate {
    pub queue_removed: usize,
    pub store_removed: usize,
    pub promoted: usize,
}

impl QueueRotate {
    pub fn has_changes(&self) -> bool {
        self.queue_removed > 0 || self.store_removed > 0 || self.promoted > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Store,
    Queue(usize),
}

impl Serialize for Position {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Queue(size) => serializer.serialize_u64(*size as u64),
            Self::Store => serializer.serialize_u64(0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreCapacity {
    Sized(usize),
    Unlimited,
}

impl Serialize for StoreCapacity {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Sized(size) => serializer.serialize_u64(*size as u64),
            Self::Unlimited => serializer.serialize_u64(0),
        }
    }
}

impl<'de> Deserialize<'de> for StoreCapacity {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StoreCapacityVisitor;

        impl<'de> de::Visitor<'de> for StoreCapacityVisitor {
            type Value = StoreCapacity;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "-1 for unlimited capacity, or any positive integer for a fixed size",
                )
            }

            fn visit_i64<E>(self, value: i64) -> core::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    ..0 => Ok(StoreCapacity::Unlimited),
                    0.. => Ok(StoreCapacity::Sized(value as usize)),
                }
            }
        }

        deserializer.deserialize_i64(StoreCapacityVisitor)
    }
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

#[derive(Clone, Debug, PartialEq)]
pub struct QueueSyncTimestamp(pub usize);

impl TryFrom<&str> for QueueSyncTimestamp {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        let parsed = value.trim().parse::<usize>()?;
        let timestamp = Self(parsed);
        Ok(timestamp)
    }
}

impl TryFrom<String> for QueueSyncTimestamp {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        Self::try_from(value.as_ref())
    }
}

impl TryFrom<Option<isize>> for QueueSyncTimestamp {
    type Error = Error;
    fn try_from(value: Option<isize>) -> Result<Self> {
        match value {
            Some(v) => Ok(Self::from(v)),
            None => Err(Error::QueueSyncTimestampOutOfRange(String::from(
                ERROR_NULL_STRING,
            ))),
        }
    }
}

impl From<isize> for QueueSyncTimestamp {
    fn from(value: isize) -> Self {
        Self(value as usize)
    }
}

impl TryFrom<Option<usize>> for QueueSyncTimestamp {
    type Error = Error;
    fn try_from(value: Option<usize>) -> Result<Self> {
        match value {
            Some(v) => Ok(Self::from(v)),
            None => Err(Error::QueueSyncTimestampOutOfRange(String::from(
                ERROR_NULL_STRING,
            ))),
        }
    }
}

impl From<usize> for QueueSyncTimestamp {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<QueueSyncTimestamp> for usize {
    fn from(value: QueueSyncTimestamp) -> Self {
        value.0
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
        fn test_queue_sync_timestamp_from_option_string_none() {
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

    mod queue_sync_timestamp {
        use super::*;

        #[test]
        fn test_usize_from_queue_sync_timestamp() {
            assert_eq!(usize::from(QueueSyncTimestamp(123)), 123);
        }

        #[test]
        fn test_queue_sync_timestamp_from_isize() {
            let value: isize = 456;
            assert_eq!(QueueSyncTimestamp::from(value), QueueSyncTimestamp(456));
        }

        #[test]
        fn test_queue_sync_timestamp_from_usize() {
            let value: usize = 123;
            assert_eq!(QueueSyncTimestamp::from(value), QueueSyncTimestamp(123));
        }

        #[test]
        fn test_queue_sync_timestamp_from_string() {
            assert_eq!(
                QueueSyncTimestamp::try_from("123").unwrap(),
                QueueSyncTimestamp(123)
            );
        }

        #[test]
        fn test_queue_sync_timestamp_from_option_string_none() {
            let v: Option<isize> = None;
            match QueueSyncTimestamp::try_from(v) {
                Err(Error::QueueSyncTimestampOutOfRange(v)) => assert_eq!(v, ERROR_NULL_STRING),
                _ => assert!(false),
            }
        }

        #[test]
        fn test_queue_sync_timestamp_from_string_error() {
            match QueueSyncTimestamp::try_from("abc") {
                Err(_) => assert!(true),
                _ => panic!("Should've failed converting 'abc' to a timestamp"),
            };
        }
    }
}
