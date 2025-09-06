-----------------------------------------------------------------------------------------------------------------------
-- ID ADD
--
-- Add a UUID to the queue/store with expiration times
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: id - STRING
-- ARGV[3]: time - INTEGER
-- ARGV[4]: validated_expiry - INTEGER
-- ARGV[5]: quarantine_expiry - INTEGER
-----------------------------------------------------------------------------------------------------------------------

-- DEV NOTE: This code should be identical to the "Add" part of ID_POSITION

-- Check if the store capacity is -1 (this means the store capacity is infinite and we don't need to add the token to the queue)
local max_size = redis.call('GET', ARGV[1] .. ':store_capacity')
if max_size == false or max_size == nil then
    -- Assume a nil key is an infinite store
    max_size = -1
else
    max_size = tonumber(max_size)
end

if max_size < 0 then
    -- Store capacity less than zero is an infinite store
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