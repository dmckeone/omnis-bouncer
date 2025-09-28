<script setup lang="ts">
import { ref, type Ref } from 'vue'
import StatGroup from './StatGroup.vue'
import StatPanel from './StatPanel.vue'

const title: Ref<string> = ref('Stat')
const value: Ref<string> = ref('100')
</script>

<template>
  <Story auto-props-disabled>
    <div class="flex fixed w-screen min-h-screen bg-gray">
      <StatGroup class="flex-auto bg-blue">
        <StatPanel :title="title" :value="value" />
        <StatPanel :title="title" :value="value" />
        <StatPanel :title="title" :value="value" />
        <StatPanel :title="title" :value="value" />
      </StatGroup>
    </div>
    <template #controls>
      <HstText v-model="title" title="Title" />
      <HstText v-model="value" title="Value" />
    </template>
  </Story>
</template>

<style scoped></style>
