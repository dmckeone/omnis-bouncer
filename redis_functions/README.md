# Redis Function Data Structures & Functions

The Redis functions use a series of a data structures to coordinate the store and queue interaction.

The mental model for the Omnis Bouncer is based on an analogy of a brick and mortar store. The "capacity" is
similar to fire-rated capacity, or the number of people physically permitted in the "store". The "queue" would be the
lineup outside the "store", waiting to get into the "store". When the "store" is full, then as people exit the
matching number of people would be let in from the "queue" and into the "store".

For example, if the store capacity is 30 people, the store is full, and 2 people leave the store (30 - 2 = 28), then 2
people would be permitted to exit the queue and enter the store (28 + 2 = 30), bringing the store back to full capacity.

All the functions support a key prefix, which allows multiple queues/stores to be coordinated out of the same
Redis database, using the same functions

## Management Knobs

* **Store**
    * `:store_capacity`: `INTEGER` - Maximum number of IDs permitted in the store. `< 0` indicates an infinite
      size store.  `0` is a closed store that will not let anyone in from the queue.
* **Queue**
    * `:queue_enabled`: `INTEGER` `0` - Queue Disabled, `1`: Queue Enabled
    * `:queue_waiting_page`: `STRING` - Static HTML content of waiting page to serve to IDs waiting in the queue
    * `:queue_sync_timestamp`: `INTEGER` - Store result of [TIME](https://redis.io/docs/latest/commands/time/) when
      queue
      state was last synced to a database

## Events

* `:events`: `PUBLISH`/`SUBSCRIBE` channel for events

## Internal State Tracking

* **Store**
    * `:store_ids`: `SET` - Set of IDs within the store
    * `:store_expiry_secs`: `HASH` - Hash map (**key**: ID, **value
      **: [TIME](https://redis.io/docs/latest/commands/time/) when ID should expire from the store)
* **Queue**
    * `:queue_ids`: `LIST` - Ordered list of IDs within the queue
    * `:queue_expiry_secs`: `HASH` - Hash map (**key**: ID, **value
      **: [TIME](https://redis.io/docs/latest/commands/time/) when ID should expiry from the queue)
    * `:queue_position_cache`: `HASH` - Hash map (**key**: ID, **value**: last known integer queue position), this
      optimization allows for quick lookups of queue position at the cost of slower modifications to the queue.