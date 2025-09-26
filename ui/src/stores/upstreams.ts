import { defineStore } from 'pinia'

import { API_URI } from '@/constants'
import { useFetch } from '@vueuse/core'
import type { ShallowRef } from 'vue'
import type { Upstream } from '@/models.ts'

export const useUpstreams = defineStore('upstreams', () => {
  const {
    data,
    error,
    execute,
  }: {
    data: ShallowRef<[Upstream] | null>
    error: ShallowRef<any>
    execute: (throwOnFailed?: boolean) => Promise<any>
  } = useFetch(API_URI + 'api/upstreams')
    .get()
    .json()

  // Refetch every 10 seconds
  setTimeout(() => execute(), 10 * 1000)

  return { upstreams: data, error }
})
