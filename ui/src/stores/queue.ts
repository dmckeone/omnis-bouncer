import {defineStore} from 'pinia'
import {ref} from "vue";

import {API_URI} from '@/constants';

interface AppInfo {
    name: string
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
    const info = ref<AppInfo>({
        name: "Omnis Bouncer"
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

        const uri = API_URI + 'api/info';
        fetchingInfo = true;
        try {
            const response = await fetch(uri)
            if (response.status == 200) {
                info.value = await response.json()
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
    eventSource.onmessage = function (messageEvent: MessageEvent) {
        const event = messageEvent.data;
        const is_interesting = INTERESTING_EVENTS_RE.test(event);
        if (is_interesting) {
            fetchStatus();
        }
    }

    return {info, status};
})
