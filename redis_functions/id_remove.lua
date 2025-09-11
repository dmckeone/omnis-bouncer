-----------------------------------------------------------------------------------------------------------------------
-- ID REMOVE
--
-- Remove a UUID from the queue/store
--
-- OPTIMIZATION: Removing an ID from the queue is too expensive in a single operation (See: store_promote), so queue
--               "removal" only marks the queue ID as immediately expired.
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: id - STRING
-- ARGV[3]: time - INTEGER
-----------------------------------------------------------------------------------------------------------------------

local in_queue = redis.call('HEXISTS', ARGV[1] .. ':queue_expiry_secs', ARGV[2])
if in_queue ~= nil and in_queue == 1 then
    -- In Queue: Mark as expired (1 second earlier than what is considered the current time)
    redis.call('HSET', ARGV[1] .. ':queue_expiry_secs', ARGV[2], ARGV[3] - 1)
else
    -- In Store: Remove from store
    redis.call('HDEL', ARGV[1] .. ':store_expiry_secs', ARGV[2])
    redis.call('SREM', ARGV[1] .. ':store_ids', ARGV[2])
end
