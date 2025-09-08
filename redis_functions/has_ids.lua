-----------------------------------------------------------------------------------------------------------------------
-- HAS IDS
--
-- Return 1 if the queue or the store has any UUIDs, return 0 if there are no UUIDs in the queue or the store.
--
-- ARGV[1]: prefix - STRING
-----------------------------------------------------------------------------------------------------------------------

-- DEV NOTE: nil conditions detect when keys have been flushed, and allow a short-circuit signal to re-initialize keys
local queue_size = redis.call('LLEN',  ARGV[1] .. ':queue_ids')
if queue_size == nil or queue_size > 0 then
    return 1
end
local active_size = redis.call('SCARD',  ARGV[1] .. ':store_ids')
if active_size == nil or active_size > 0 then
    return 1
end
return 0