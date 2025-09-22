-----------------------------------------------------------------------------------------------------------------------
-- QUEUE TIMEOUT (HOUSEKEEPING)
--
-- Remove timed out UUIDs from the queue, based on the current_time from [TIME](https://redis.io/docs/latest/commands/time/)
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: current_time - INTEGER
-----------------------------------------------------------------------------------------------------------------------

local queue_ids_key = ARGV[1] .. ':queue_ids'
local queue_expiry_secs_key = ARGV[1] .. ':queue_expiry_secs'
local queue_position_cache_key = ARGV[1] .. ':queue_position_cache'

local queue_size = redis.call('LLEN', queue_ids_key)
if queue_size == nil then
   queue_size = 0
else
   queue_size = queue_size - 1
end

-- Loop through all IDs in the queue from front to back, keeping a count of removed positions so that IDs later in
-- the line can be modified with their new position
local position_modifier = 0
local removed = 0
for i=0, queue_size do
    local uuid_id = redis.call('LINDEX', queue_ids_key, i + position_modifier)
    local expiry = redis.call('HGET', queue_expiry_secs_key, uuid_id)
    if expiry ~= nil and expiry < ARGV[2] then
        redis.call('HDEL', queue_expiry_secs_key, uuid_id)
        redis.call('HDEL', queue_position_cache_key, uuid_id)
        redis.call('LREM', queue_ids_key, 1, uuid_id)
        position_modifier = position_modifier - 1
        removed = removed + 1
    else
        redis.call('HSET', queue_position_cache_key, uuid_id, i + position_modifier + 1)
    end
end
return removed