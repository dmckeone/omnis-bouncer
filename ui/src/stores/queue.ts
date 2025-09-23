import {defineStore} from 'pinia'
import {ref} from "vue";
import {useTitle} from '@vueuse/core'

import {API_URI} from '@/constants';

interface Upstream {
    uri: string;
    connections: number,
    sticky_sessions: number,
}

interface AppInfo {
    name: string,
    redis_uri: string,
    config_upstream: [Upstream],
    id_cookie_name: string,
    position_cookie_name: string,
    queue_size_cookie_name: string,
    position_http_header: string,
    queue_size_http_header: string,
    acquire_timeout: number,
    connect_timeout: number,
    cookie_id_expiration: number,
    sticky_session_timeout: number,
    asset_cache_secs: number,
    buffer_connections: number,
    js_client_rate_limit_per_sec: number,
    api_rate_limit_per_sec: number,
    ultra_rate_limit_per_sec: number,
    public_http_port: number,
    public_https_port: number,
    monitor_https_port: number,
    queue_enabled: boolean,
    queue_rotation_enabled: boolean,
    store_capacity: number,
    redis_prefix: string,
    quarantine_expiry: number,
    validated_expiry: number,
    publish_throttle: number,
}

interface QueueStatus {
    queue_enabled: boolean,
    store_capacity: number,
    queue_size: number,
    store_size: number,
    updated?: Date,
}

const INTERESTING_EVENTS_RE = /^(settings|queue|store):/i;

export const useQueueStatus = defineStore('queue', () => {
    const title = useTitle("");

    const config = ref<AppInfo>({
        name: "Omnis Bouncer",
        redis_uri: "redis://127.0.0.1:6379",
        config_upstream: [{uri: "http://127.0.0.1:63111", connections: 100, sticky_sessions: 10}],
        id_cookie_name: "omnis-bouncer-id",
        position_cookie_name: "omnis-bouncer-queue-position",
        queue_size_cookie_name: "omnis-bouncer-queue-size",
        position_http_header: "x-omnis-bouncer-queue-position",
        queue_size_http_header: "x-omnis-bouncer-queue-size",
        acquire_timeout: 10,
        connect_timeout: 10,
        cookie_id_expiration: 86400,
        sticky_session_timeout: 600,
        asset_cache_secs: 60,
        buffer_connections: 1000,
        js_client_rate_limit_per_sec: 0,
        api_rate_limit_per_sec: 10,
        ultra_rate_limit_per_sec: 10,
        public_http_port: 3000,
        public_https_port: 3001,
        monitor_https_port: 2999,
        queue_enabled: true,
        queue_rotation_enabled: true,
        store_capacity: 5,
        redis_prefix: "omnis_bouncer",
        quarantine_expiry: 45,
        validated_expiry: 600,
        publish_throttle: 0,
    });

    const status = ref<QueueStatus>({
        queue_enabled: false,
        store_capacity: 0,
        queue_size: 0,
        store_size: 0,
    })

    let fetchingInfo = false;
    const fetchInfo = async () => {
        if (!API_URI) {
            return;
        }
        if (fetchingInfo) {
            return
        }

        const uri = API_URI + 'api/config';
        fetchingInfo = true;
        try {
            const response = await fetch(uri)
            if (response.status == 200) {
                config.value = await response.json()
                title.value = config.value.name;
            }
        } catch (e) {
            console.log(`Error querying ${uri}: ${e}`)
        } finally {
            fetchingInfo = false
        }
    }
    fetchInfo();

    let fetchingStatus = false;
    const fetchStatus = async () => {
        if (!API_URI) {
            return;
        }
        if (fetchingStatus) {
            return;
        }

        const uri = API_URI + 'api/status';

        fetchingStatus = true;
        try {

            const response = await fetch(uri)
            if (response.status == 200) {
                status.value = await response.json()
            }
        } catch (e) {
            console.log(`Error querying ${uri}: ${e}`)
        } finally {
            fetchingStatus = false;
        }
    }
    fetchStatus();

    const eventSource = new EventSource(API_URI + "api/sse");
    eventSource.onerror = function (/* event: Event */) {
        // TODO: Add reconnect if needed
        //console.error("SSE Error: ", event);
    }
    eventSource.onmessage = function (messageEvent: MessageEvent) {
        const event = messageEvent.data;
        const is_interesting = INTERESTING_EVENTS_RE.test(event);
        if (is_interesting) {
            fetchStatus();
        }
    }

    return {info: config, status};
})
