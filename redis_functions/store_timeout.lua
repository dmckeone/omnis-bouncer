-----------------------------------------------------------------------------------------------------------------------
-- STORE TIMEOUT (HOUSEKEEPING)
--
-- Remove timed out UUIDs from the store, based on the current_time from [TIME](https://redis.io/docs/latest/commands/time/)
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: current_time - INTEGER
-----------------------------------------------------------------------------------------------------------------------

local store_ids_key = ARGV[1] .. ':store_ids'
local store_expiry_secs_key = ARGV[1] .. ':store_expiry_secs'

local tokens = redis.call('SMEMBERS', store_ids_key)
local removed = 0
for index, uuid_id in pairs(tokens) do
    local expiry = redis.call( 'HGET', store_expiry_secs_key, uuid_id )
    if expiry ~= nil and expiry < ARGV[2] then
        redis.call( 'HDEL', store_expiry_secs_key, uuid_id )
        redis.call( 'SREM', store_ids_key, uuid_id )
        removed = removed + 1
    end
end
return removed