export interface Config {
  name: string
  default_locale: string
  locales: string[]
  redis_uri: string
  config_upstream: Upstream[]
  id_cookie_name: string
  position_cookie_name: string
  queue_size_cookie_name: string
  id_upstream_http_header: string
  id_evict_upstream_http_header: string
  position_http_header: string
  queue_size_http_header: string
  acquire_timeout: number
  connect_timeout: number
  cookie_id_expiration: number
  sticky_session_timeout: number
  asset_cache_secs: number
  buffer_connections: number
  js_client_rate_limit_per_sec: number
  api_rate_limit_per_sec: number
  ultra_rate_limit_per_sec: number
  public_http_port: number
  public_https_port: number
  monitor_https_port: number
  queue_enabled: boolean
  queue_rotation_enabled: boolean
  store_capacity: number
  redis_prefix: string
  quarantine_expiry: number
  validated_expiry: number
  publish_throttle: number
  ultra_thin_inject_headers: boolean
  fallback_ultra_thin_library: string | null
  fallback_ultra_thin_class: string | null
}

export interface QueueStatus {
  queue_enabled: boolean
  store_capacity: number
  queue_size: number
  store_size: number
  updated?: Date
}

export interface Upstream {
  uri: string
  connections: number
  sticky_sessions: number
}

export const INTERESTING_EVENTS_RE = /^(settings|queue|store):/i
