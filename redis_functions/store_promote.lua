-----------------------------------------------------------------------------------------------------------------------
-- STORE PROMOTE
--
-- Promote any Queue IDs into the Store if the Store has some available capacity
--
-- ARGV[1]: prefix - STRING
-----------------------------------------------------------------------------------------------------------------------

local store_capacity = redis.call('GET', ARGV[1] .. ':store_capacity')
if store_capacity == false or store_capacity == nil then
    -- Assume a nil key is an infinite store
    store_capacity = -1
else
    store_capacity = tonumber(store_capacity)
end

local store_size = redis.call('SCARD', ARGV[1] .. ':store_ids');

local transfer_size = 0;
if store_size < store_capacity then
    if store_capacity > 0 then
        transfer_size = store_capacity - store_size
    else
        transfer_size = redis.call('LLEN', ARGV[1] .. ':queue_ids');
    end
end

local transfer_ids = {}
local transfer_count = 0
for i=0, transfer_size - 1 do
    local uuid_id = redis.call('LPOP', ARGV[1] .. ':queue_ids')
    if uuid_id then
        table.insert(transfer_ids, uuid_id)
        transfer_count = transfer_count + 1
    end
end

local moved = 0
if transfer_count > 0 then
    redis.call('SADD', ARGV[1] .. ':store_ids', unpack(transfer_ids))
    for index, uuid_id in pairs(transfer_ids) do
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