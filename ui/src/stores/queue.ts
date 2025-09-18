import {defineStore} from 'pinia'
import {ref} from "vue";

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

    const fetchStatus = async () => {
        const response = await fetch('/api/status')
        status.value = await response.json()
    }
    fetchStatus();

    const eventSource = new EventSource("/api/sse");
    eventSource.onmessage = function (event) {
        fetchStatus();
    }

    return {status};
})
