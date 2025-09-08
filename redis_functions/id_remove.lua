-----------------------------------------------------------------------------------------------------------------------
-- ID REMOVE
--
-- Remove a UUID from the queue and/or store
--
-- ARGV[1]: prefix - STRING
-- ARGV[2]: id - STRING
-----------------------------------------------------------------------------------------------------------------------

redis.call('HDEL', ARGV[1] .. ':store_expiry_secs', ARGV[2])
redis.call('SREM', ARGV[1] .. ':store_ids', ARGV[2])