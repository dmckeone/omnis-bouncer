<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { useQueueStatus } from '@/stores/queue.ts'
import QueueStatus from '@/components/QueueStatus.vue'
import UpstreamTable from '@/components/UpstreamTable.vue'

const queueStore = useQueueStatus()
const { config } = storeToRefs(queueStore)
</script>

<template>
  <div class="min-h-screen">
    <div class="navbar bg-base-100 shadow-sm">
      <div class="flex-1">
        <a v-if="config != null" class="btn btn-ghost text-xl">{{ config?.name }}</a>
      </div>
    </div>
    <div class="flex fixed w-screen">
      <div class="flex flex-col">
        <QueueStatus />
        <hr class="mx-5 border-accent" />
        <UpstreamTable />
      </div>
    </div>
    <div class="w-5"></div>
  </div>
</template>

<style scoped></style>
