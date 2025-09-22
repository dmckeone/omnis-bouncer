-----------------------------------------------------------------------------------------------------------------------
-- STORE PROMOTE
--
-- Promote any Queue IDs into the Store if the Store has some available capacity
--
-- ARGV[1]: prefix - STRING
-----------------------------------------------------------------------------------------------------------------------

local queue_ids_key = ARGV[1] .. ':queue_ids'
local queue_expiry_secs_key = ARGV[1] .. ':queue_expiry_secs'
local queue_position_cache_key = ARGV[1] .. ':queue_position_cache'
local store_capacity_key = ARGV[1] .. ':store_capacity'
local store_ids_key = ARGV[1] .. ':store_ids'
local store_expiry_secs_key = ARGV[1] .. ':store_expiry_secs'

-- Determine store capacity
local store_capacity = redis.call('GET', store_capacity_key)
if store_capacity == false or store_capacity == nil then
    -- Assume a nil key is an infinite store
    store_capacity = -1
else
    store_capacity = tonumber(store_capacity)
end

local store_size = redis.call('SCARD', store_ids_key);

-- Determine number of IDs to transfer from Queue to Store
local transfer_size = 0;
if store_size < store_capacity then
    if store_capacity > 0 then
        transfer_size = store_capacity - store_size
    else
        transfer_size = redis.call('LLEN', queue_ids_key);
    end
end

-- Pop all IDs from Queue for transfer
local transfer_ids = {}
local transfer_count = 0
for i=0, transfer_size - 1 do
    local uuid_id = redis.call('LPOP', queue_ids_key)
    if uuid_id then
        redis.call('HDEL', queue_position_cache_key, uuid_id)
        table.insert(transfer_ids, uuid_id)
        transfer_count = transfer_count + 1
    end
end

-- Add popped IDs into the store, transferring expiry from queue to store
local moved = 0
if transfer_count > 0 then
    redis.call('SADD', store_ids_key, unpack(transfer_ids))
    for index, uuid_id in pairs(transfer_ids) do
        local expiry = redis.call('HGET', queue_expiry_secs_key, uuid_id)
        if expiry ~= nil then
            redis.call('HSET', store_expiry_secs_key, uuid_id, expiry)
        end
        redis.call('HDEL', queue_expiry_secs_key, uuid_id)
        moved = moved + 1
    end
end
return moved