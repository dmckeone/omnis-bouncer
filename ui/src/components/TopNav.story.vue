<script setup lang="ts">
import { type Ref, ref } from 'vue'

import TestContainer from '@/components/TestContainer.vue'
import TopNav from '@/components/TopNav.vue'

const title: Ref<string> = ref('My Title')
</script>

<template>
  <Story auto-props-disabled responsive-disabled>
    <TestContainer>
      <TopNav :title="title"></TopNav>
    </TestContainer>

    <template #controls>
      <HstText v-model="title" title="Title" />
    </template>
  </Story>
</template>

<style scoped></style>
