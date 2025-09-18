import {defineStore} from 'pinia'
import {ref} from "vue";

import {API_URI} from '@/constants';

interface QueueStatus {
    queue_enabled: boolean,
    store_capacity: number,
    queue_size: number,
    store_size: number,
    updated?: Date,
}

export const useQueueStatus = defineStore('queue', () => {
    const status = ref<QueueStatus>({
        queue_enabled: false,
        store_capacity: 0,
        queue_size: 0,
        store_size: 0,
    })

    let fetching = false;
    const fetchStatus = async () => {
        if (fetching) {
            return;
        }
        fetching = true;
        try {
            const response = await fetch(API_URI + '/api/status')
            status.value = await response.json()
        } finally {
            fetching = false;
        }
    }
    fetchStatus();

    const eventSource = new EventSource(API_URI + "/api/sse");
    eventSource.onmessage = function (event: MessageEvent) {
        console.log("Event: ", event);
        fetchStatus();
    }

    return {status};
})
