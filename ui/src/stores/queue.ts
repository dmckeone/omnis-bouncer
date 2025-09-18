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

    const fetchInfo = async () => {
        try {
            const response = await fetch(API_URI + '/api/info')
            info.value = await response.json()
        } finally {
        }
    }
    fetchInfo();

    let fetchingStatus = false;
    const fetchStatus = async () => {
        if (fetchingStatus) {
            return;
        }
        fetchingStatus = true;
        try {
            const response = await fetch(API_URI + '/api/status')
            status.value = await response.json()
        } finally {
            fetchingStatus = false;
        }
    }
    fetchStatus();

    const eventSource = new EventSource(API_URI + "/api/sse");
    eventSource.onmessage = function (messageEvent: MessageEvent) {
        const event = messageEvent.data;
        const is_interesting = INTERESTING_EVENTS_RE.test(event);
        console.log(event, is_interesting)
        if (is_interesting) {
            fetchStatus();
        }
    }

    return {info, status};
})
