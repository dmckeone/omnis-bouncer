-----------------------------------------------------------------------------------------------------------------------
-- CHECK SYNC KEYS
--
-- Simple function to check that all keys required for syncing the queue/store are available
--
-- ARGV[1]: prefix - STRING
-----------------------------------------------------------------------------------------------------------------------

local keys = {
    ARGV[1] .. ':queue_enabled',
    ARGV[1] .. ':store_capacity',
    ARGV[1] .. ':queue_sync_timestamp'
}

for i, key in pairs(keys) do
    local exists = redis.call('EXISTS', key)
    if exists == 0 then
        return 0
    end
end
return 1
