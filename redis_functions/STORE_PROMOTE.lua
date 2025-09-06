-----------------------------------------------------------------------------------------------------------------------
-- STORE PROMOTE (HOUSEKEEPING)
--
-- Promote an integer number of UUIDs from the queue into the store
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: batch_size - INTEGER
-----------------------------------------------------------------------------------------------------------------------

local batch = {}
local batch_count = 0
for i=0, ARGV[2] - 1 do
    local uuid_id = redis.call('LPOP', ARGV[1] .. ':queue_ids')
    if uuid_id then
        table.insert(batch, uuid_id)
        batch_count = batch_count + 1
    end
end

local moved = 0
if batch_count > 0 then
    redis.call('SADD', ARGV[1] .. ':store_ids', unpack(batch))
    for index, uuid_id in pairs(batch) do
        local expiry = redis.call('HGET', ARGV[1] .. ':queue_expiry_secs', uuid_id)
        if expiry ~= nil then
            redis.call('HSET', ARGV[1] .. ':store_expiry_secs', uuid_id, expiry)
        end
        redis.call('HDEL', ARGV[1] .. ':queue_position_cache', uuid_id)
        redis.call('HDEL', ARGV[1] .. ':queue_expiry_secs', uuid_id)
        moved = moved + 1
    end
end
return moved