import {defineStore} from 'pinia'

import {API_URI} from '@/constants';
import {useFetch} from "@vueuse/core";


export const useUpstreams = defineStore('upstreams', () => {
    const {
        data,
        error,
        execute,
    } = useFetch(API_URI + "api/upstreams").get().json()

    // Refetch every 10 seconds
    setTimeout(() => execute(), 10 * 1000);

    return {upstreams: data, error}
})
