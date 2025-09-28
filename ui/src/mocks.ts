import type { Store, StoreDefinition } from 'pinia'
import type { Mock } from 'vitest'
import type { UnwrapRef } from 'vue'

import type { Config, QueueStatus, Upstream } from '@/models.ts'

// Helper function for mocking stores
export function mockedStore<TStoreDef extends () => unknown>(
  useStore: TStoreDef,
): TStoreDef extends StoreDefinition<infer Id, infer State, infer Getters, infer Actions>
  ? Store<
      Id,
      State,
      Record<string, never>,
      {
        [K in keyof Actions]: Actions[K] extends (...args: any[]) => any
          ? // 👇 depends on your testing framework
            Mock<Actions[K]>
          : Actions[K]
      }
    > & {
      [K in keyof Getters]: UnwrapRef<Getters[K]>
    }
  : ReturnType<TStoreDef> {
  return useStore() as any
}

export const mockConfig: Config = {
  name: 'My Bouncer',
  default_locale: 'en',
  locales: ['en'],
  redis_uri: 'redis://127.0.0.1',
  config_upstream: [
    {
      uri: 'http://127.0.0.1:63111',
      connections: 20,
      sticky_sessions: 20,
    },
    { uri: 'http://127.0.0.1:63112', connections: 20, sticky_sessions: 20 },
  ],
  id_cookie_name: 'omnis-bouncer-id',
  position_cookie_name: 'omnis-bouncer-queue-position',
  id_upstream_http_header: 'x-omnis-bouncer-id',
  id_evict_upstream_http_header: 'x-omnis-bouncer-id-evict',
  queue_size_cookie_name: 'omnis-bouncer-queue-size',
  position_http_header: 'x-omnis-bouncer-queue-position',
  queue_size_http_header: 'x-omnis-bouncer-queue-size',
  acquire_timeout: 10,
  connect_timeout: 10,
  cookie_id_expiration: 86400,
  sticky_session_timeout: 60,
  asset_cache_secs: 60,
  buffer_connections: 1000,
  js_client_rate_limit_per_sec: 0,
  api_rate_limit_per_sec: 5,
  ultra_rate_limit_per_sec: 0,
  public_http_port: 3000,
  public_https_port: 3001,
  monitor_https_port: 2999,
  queue_enabled: true,
  queue_rotation_enabled: true,
  store_capacity: 5,
  redis_prefix: 'omnis_bouncer',
  quarantine_expiry: 30,
  validated_expiry: 60,
  publish_throttle: 0,
  ultra_thin_inject_headers: true,
  fallback_ultra_thin_library: 'jsclientmethods',
  fallback_ultra_thin_class: 'rtUltra',
}

export const mockStatus: QueueStatus = {
  queue_enabled: true,
  store_capacity: 10,
  queue_size: 123,
  store_size: 10,
  updated: new Date(2025, 9, 27, 5, 31),
}

export const mockUpstreams: Upstream[] = [
  {
    uri: 'http://127.0.0.1:63111',
    connections: 20,
    sticky_sessions: 20,
  },
  { uri: 'http://127.0.0.1:63112', connections: 20, sticky_sessions: 20 },
]
