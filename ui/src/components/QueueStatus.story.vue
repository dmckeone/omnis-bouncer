<script setup lang="ts">
import { type Ref, ref } from 'vue'
import QueueStatus from '@/components/QueueStatus.vue'
import type { QueueStatus as QueueStatusType } from '@/models'
import { mockStatus } from '@/mocks.ts'

const status: Ref<QueueStatusType> = ref(mockStatus)
</script>

<template>
  <Story auto-props-disabled>
    <QueueStatus :status="status" />

    <template #controls>
      <HstJson v-model="status" title="Status" />
    </template>
  </Story>
</template>

<style scoped></style>
