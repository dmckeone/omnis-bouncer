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
--
-- RETURN: 2-Tuple of {added - INTEGER, position - INTEGER}
-----------------------------------------------------------------------------------------------------------------------

local store_ids_key = ARGV[1] .. ':store_ids'
local store_expiry_secs_key = ARGV[1] .. ':store_expiry_secs'

-- Check if uuid_id is in the store
local in_set = redis.call('SISMEMBER', store_ids_key, ARGV[2])
if in_set == 1 then
    redis.call('HSET', store_expiry_secs_key, ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry

    local result = {}
    result[1] = 0
    result[2] = 0
    return result
end


local queue_expiry_secs_key = ARGV[1] .. ':queue_expiry_secs'
local queue_position_cache_key = ARGV[1] .. ':queue_position_cache'


-- Check if uuid_id is in the queue
local queue_position = redis.call('HGET', queue_position_cache_key, ARGV[2])
if queue_position then
    redis.call('HSET', queue_expiry_secs_key, ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry

    local result = {}
    result[1] = 0
    result[2] = queue_position
    return result
end

local store_capacity_key = ARGV[1] .. ':store_capacity'

-- DEV NOTE: All the code below handles adding new tokens to the queue, including quarantine -> validated upgrade

-- Check if the store size is -1 (this means the store size is infinite and we don't need to add the token to the queue)
local store_capacity = redis.call('GET', store_capacity_key)
if store_capacity == false or store_capacity == nil then
    -- Assume a nil key is an infinite store
    store_capacity = -1
else
    store_capacity = tonumber(store_capacity)
end

-- Infinite store, so add to the store
if store_capacity < 0 then
    redis.call('SADD', store_ids_key, ARGV[2])
    redis.call('HSET', store_expiry_secs_key, ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry

    local result = {}
    result[1] = 1
    result[2] = 0
    return result
end

local queue_ids_key = ARGV[1] .. ':queue_ids'

-- Check if the queue is larger than 0 (if so we should add the token to the queue)
local queue_size = redis.call('LLEN', queue_ids_key)
if queue_size ~= nil and queue_size > 0 then
    local pos = redis.call('RPUSH', queue_ids_key, ARGV[2])
    redis.call('HSET', queue_position_cache_key, ARGV[2], pos)
    redis.call('HSET', queue_expiry_secs_key, ARGV[2], ARGV[3] + ARGV[5]) -- quarantine expiry

    local result = {}
    result[1] = 1
    result[2] = pos
    return result
end

-- Check the number of store tokens and see if there is room to add the new token into the store (queue was 0 and
-- store size is not infinite)
local current_size = redis.call('SCARD', store_ids_key)
if current_size ~= nil and current_size < store_capacity then
    -- The store has room, add the ID to the store
    redis.call('SADD', store_ids_key, ARGV[2])
    redis.call('HSET', store_expiry_secs_key, ARGV[2], ARGV[3] + ARGV[4]) -- validated expiry

    local result = {}
    result[1] = 1
    result[2] = 0
    return result
else
    -- The store did not have room, add the ID to the queue
    local pos = redis.call('RPUSH', queue_ids_key, ARGV[2])
    redis.call('HSET', queue_position_cache_key, ARGV[2], pos)
    redis.call('HSET', queue_expiry_secs_key, ARGV[2], ARGV[3] + ARGV[5]) -- quarantine expiry

    local result = {}
    result[1] = 1
    result[2] = pos
    return result
end