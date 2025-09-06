-----------------------------------------------------------------------------------------------------------------------
-- STORE TIMEOUT (HOUSEKEEPING)
--
-- Remove timed out UUIDs from the store, based on the current_time from [TIME](https://redis.io/docs/latest/commands/time/)
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: current_time - INTEGER
-----------------------------------------------------------------------------------------------------------------------

local tokens = redis.call('SMEMBERS', ARGV[1] .. ':store_ids')
local removed = 0
for index, uuid_id in pairs(tokens) do
    local expiry = redis.call( 'HGET', ARGV[1] .. ':store_expiry_secs', uuid_id )
    if expiry ~= nil and expiry < ARGV[2] then
        redis.call( 'HDEL', ARGV[1] .. ':store_expiry_secs', uuid_id )
        redis.call( 'SREM', ARGV[1] .. ':store_ids', uuid_id )
        removed = removed + 1
    end
end
return removed