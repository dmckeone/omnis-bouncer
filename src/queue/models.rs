use crate::errors::{Error, Result};

pub struct QueueRotate {
    pub store_removed: usize,
    pub moved: usize,
    pub queue_removed: usize,
}

#[derive(PartialEq, Debug)]
pub enum StoreCapacity {
    Sized(usize),
    Unlimited,
}

impl TryFrom<isize> for StoreCapacity {
    type Error = Error;

    fn try_from(size: isize) -> Result<StoreCapacity> {
        match size {
            ..-1 => Err(Error::StoreCapacityOutOfRange(size)),
            -1 => Ok(StoreCapacity::Unlimited),
            0.. => Ok(StoreCapacity::Sized(size as usize)),
        }
    }
}

impl TryFrom<String> for StoreCapacity {
    type Error = Error;

    fn try_from(value: String) -> Result<StoreCapacity> {
        StoreCapacity::try_from(value.parse::<isize>()?)
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

mod test {
    use super::*;

    #[test]
    fn test_from_isize_error() {
        let expected: isize = -2;
        let actual = StoreCapacity::try_from(expected);
        match actual {
            Err(Error::StoreCapacityOutOfRange(v)) => assert_eq!(v, expected),
            _ => panic!("Should have emitted capacity error"),
        }
    }

    #[test]
    fn test_from_isize_unlimited() {
        let value: isize = -1;
        let actual = StoreCapacity::try_from(value).unwrap();
        assert_eq!(actual, StoreCapacity::Unlimited);
    }

    #[test]
    fn test_from_isize_max() {
        let value: isize = isize::MAX;
        let actual = StoreCapacity::try_from(value).unwrap();
        assert_eq!(actual, StoreCapacity::Sized(isize::MAX as usize));
    }
}
