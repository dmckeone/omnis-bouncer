use tracing::warn;

pub(crate) struct QueueRotate {
    pub store_removed: usize,
    pub moved: usize,
    pub queue_removed: usize,
}

pub(crate) enum StoreCapacity {
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

mod test {}
