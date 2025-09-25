import {defineStore} from 'pinia'
import {useFetch, useTitle} from '@vueuse/core'

import {API_URI} from '@/constants';
import {INTERESTING_EVENTS_RE} from "@/models.ts";
import {watchEffect} from "vue";


export const useQueueStatus = defineStore('queue', () => {
    const title = useTitle("");

    const {
        data: config,
        error: configError,
        execute: configExecute,
    } = useFetch(API_URI + "api/config").get().json()

    watchEffect(() => {
        if (config.value != null) {
            title.value = config.value.name
        }
        console.log(config.value)
    });

    const {
        data: status,
        error: statusError,
        execute: statusExecute,
    } = useFetch(API_URI + "api/status").get().json()

    const eventSource = new EventSource(API_URI + "api/sse");
    eventSource.onerror = function (/* event: Event */) {
        // TODO: Add reconnect if needed
        //console.error("SSE Error: ", event);
    }
    eventSource.onmessage = function (messageEvent: MessageEvent) {
        const event = messageEvent.data;
        const is_interesting = INTERESTING_EVENTS_RE.test(event);
        if (is_interesting) {
            configExecute();
            statusExecute();
        }
    }

    return {config, configError, status, statusError};
})
