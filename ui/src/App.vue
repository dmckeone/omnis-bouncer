<script setup lang="ts">
import { storeToRefs } from 'pinia'
import TopNav from '@/components/TopNav.vue'
import UpstreamTable from '@/components/UpstreamTable.vue'
import QueueStatus from '@/components/QueueStatus.vue'
import { useQueueStatus } from '@/stores/queue.ts'
import { useUpstreams } from '@/stores/upstreams.ts'

const queueStore = useQueueStatus()
const { config, status } = storeToRefs(queueStore)

const upstreamsStore = useUpstreams()
const { upstreams } = storeToRefs(upstreamsStore)
</script>

<template>
  <TopNav :title="config != null ? config.name : ''">
    <QueueStatus :status="status" />
    <hr class="mx-5 border-accent" />
    <UpstreamTable :upstreams="upstreams" />
  </TopNav>
</template>

<style scoped></style>
