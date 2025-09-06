-----------------------------------------------------------------------------------------------------------------------
-- QUEUE TIMEOUT (HOUSEKEEPING)
--
-- Remove timed out UUIDs from the queue, based on the current_time from [TIME](https://redis.io/docs/latest/commands/time/)
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: current_time - INTEGER
-----------------------------------------------------------------------------------------------------------------------

local queue_size = redis.call('LLEN', ARGV[1] .. ':queue_ids')
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
    local uuid_id = redis.call('LINDEX', ARGV[1] .. ':queue_ids', i + position_modifier)
    redis.call('HSET', ARGV[1] .. ':queue_position_cache', uuid_id, i + position_modifier + 1)
    local expiry = redis.call('HGET', ARGV[1] .. ':queue_expiry_secs', uuid_id)
    if expiry ~= nil and expiry < ARGV[2] then
        redis.call('HDEL', ARGV[1] .. ':queue_expiry_secs', uuid_id)
        redis.call('HDEL', ARGV[1] .. ':queue_position_cache', uuid_id)
        redis.call('LREM', ARGV[1] .. ':queue_ids', 1, uuid_id)
        position_modifier = position_modifier - 1
        removed = removed + 1
    end
end
return removed