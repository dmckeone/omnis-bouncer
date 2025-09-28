<script setup lang="ts">
import { type Ref, ref } from 'vue'

import QueueStatus from '@/components/QueueStatus.vue'
import TestContainer from '@/components/TestContainer.vue'
import { mockStatus } from '@/mocks.ts'
import type { QueueStatus as QueueStatusType } from '@/models'

const status: Ref<QueueStatusType> = ref(mockStatus)
</script>

<template>
  <Story auto-props-disabled>
    <TestContainer>
      <QueueStatus :status="status" />
    </TestContainer>

    <template #controls>
      <HstJson v-model="status" title="Status" />
    </template>
  </Story>
</template>

<style scoped></style>
