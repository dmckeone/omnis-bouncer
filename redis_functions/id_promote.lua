-----------------------------------------------------------------------------------------------------------------------
-- ID PROMOTE
--
-- Promote a single Queue ID into the Store, regardless of capacity
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: id - STRING
-- ARGV[3]: time - INTEGER
-- ARGV[4]: validated_expiry - INTEGER
-----------------------------------------------------------------------------------------------------------------------

local queue_ids_key = ARGV[1] .. ':queue_ids'
local queue_expiry_secs_key = ARGV[1] .. ':queue_expiry_secs'
local queue_position_cache_key = ARGV[1] .. ':queue_position_cache'
local store_capacity_key = ARGV[1] .. ':store_capacity'
local store_ids_key = ARGV[1] .. ':store_ids'
local store_expiry_secs_key = ARGV[1] .. ':store_expiry_secs'

-- Remove the ID from the queue (if it exists)
redis.call('LREM', queue_ids_key, 0, ARGV[2])
redis.call('HDEL', queue_position_cache_key, ARGV[2])
redis.call('HDEL', queue_expiry_secs_key, ARGV[2])

-- Add the ID to the store
redis.call('SADD', store_ids_key, ARGV[2])
redis.call('HSET', store_expiry_secs_key, ARGV[2], ARGV[3] + ARGV[4])
