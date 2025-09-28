<script setup lang="ts">
import { type Ref, ref } from 'vue'

import TestContainer from '@/components/TestContainer.vue'
import UpstreamTable from '@/components/UpstreamTable.vue'
import { mockUpstreams } from '@/mocks.ts'
import type { Upstream } from '@/models.ts'

const upstreams: Ref<Upstream[]> = ref(mockUpstreams)
</script>

<template>
  <Story auto-props-disabled>
    <TestContainer>
      <UpstreamTable :upstreams="upstreams" />
    </TestContainer>

    <template #controls>
      <HstJson v-model="upstreams" title="Upstreams" />
    </template>
  </Story>
</template>

<style scoped></style>
