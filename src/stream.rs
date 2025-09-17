use async_stream::stream;
use futures_util::{pin_mut, Stream, StreamExt};
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::time::{Duration, Instant};
use tokio::select;
use tokio::time::sleep_until;

pub fn debounce<S>(duration: Duration, stream: S) -> impl Stream<Item = S::Item>
where
    S: Stream,
    S::Item: Debug + Clone + Eq + Hash,
{
    stream! {
        pin_mut!(stream);
        loop {
            // Bookkeeping of unique items and last poll time
            let last_poll = Instant::now();
            let mut items: HashSet<S::Item> = HashSet::new();

            // Consume all items in the stream, de-duplicating as we go, or until we time out for
            // this duration
            loop {
                select! {
                    Some(item) = stream.next() => {
                        items.insert(item.clone());
                    }
                    _ = sleep_until((last_poll + duration).into()) => {
                        break;
                    }
                }
            }

            // Yield all unique items in the set acquired during the timeout period
            for item in items.into_iter() {
                yield item
            }
        }
    }
}
