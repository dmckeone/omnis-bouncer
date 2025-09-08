-----------------------------------------------------------------------------------------------------------------------
-- ID (QUEUE) POSITION
--
-- Return the position of a UUID in the queue.  If the UUID is in the store then the function returns 0.  As an
-- optimization, if the UUID is not in the queue or the store, then it is added in the same way as ID_ADD.  This
-- function takes into account expiry dates, to prevent returning stale queue positions.
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: id - STRING
-- ARGV[3]: time - INTEGER
-- ARGV[4]: validated_expiry - INTEGER
-- ARGV[5]: quarantine_expiry - INTEGER
-----------------------------------------------------------------------------------------------------------------------

-- Check if uuid_id is in the store
local in_set = redis.call('SISMEMBER', ARGV[1] .. ':store_ids', ARGV[2])
if in_set == 1 then
    redis.call('HSET', ARGV[1] .. ':store_expiry_secs', ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry
    return 0
end

-- Check if uuid_id is in the queue
local queue_position = redis.call('HGET', ARGV[1] .. ':queue_position_cache', ARGV[2])
if queue_position then
    redis.call('HSET', ARGV[1] .. ':queue_expiry_secs', ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry
    return queue_position
end

-- DEV NOTE: All the code below only handles adding new tokens to the queue and should be identical to ID_ADD

-- Check if the store size is -1 (this means the store size is infinite and we don't need to add the token to the queue)
local max_size = redis.call('GET', ARGV[1] .. ':store_capacity')
if max_size == false or max_size == nil then
    -- Assume a nil key is an infinite store
    max_size = -1
else
    max_size = tonumber(max_size)
end

if max_size < 0 then
    redis.call('SADD', ARGV[1] .. ':store_ids', ARGV[2])
    redis.call('HSET', ARGV[1] .. ':store_expiry_secs', ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry
    return 0
end

-- Check if the queue is larger than 0 (if so we should add the token to the queue)
local queue_size = redis.call('LLEN', ARGV[1] .. ':queue_ids')
if queue_size ~= nil and queue_size > 0 then
    local pos = redis.call('RPUSH', ARGV[1] .. ':queue_ids', ARGV[2])
    redis.call('HSET', ARGV[1] .. ':queue_position_cache', ARGV[2], pos)
    redis.call('HSET', ARGV[1] .. ':queue_expiry_secs', ARGV[2], ARGV[3] + ARGV[5]) -- quarantine expiry
    return pos
end

-- Check the number of store tokens and see if there is room to add the new token into the store (queue was 0 and
-- store size is not infinite)
local current_size = redis.call('SCARD', ARGV[1] .. ':store_ids')
if current_size ~= nil and current_size < max_size then
    -- The store has room, add the ID to the store
    redis.call('SADD', ARGV[1] .. ':store_ids', ARGV[2])
    redis.call('HSET', ARGV[1] .. ':store_expiry_secs', ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry
    return 0
else
    -- The store did not have room, add the ID to the queue
    local pos = redis.call('RPUSH', ARGV[1] .. ':queue_ids', ARGV[2])
    redis.call('HSET', ARGV[1] .. ':queue_position_cache', ARGV[2], pos)
    redis.call('HSET', ARGV[1] .. ':queue_expiry_secs', ARGV[2], ARGV[3] + ARGV[5]) -- quarantine expiry
    return pos
end