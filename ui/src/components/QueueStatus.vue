<script setup lang="ts">
import type { QueueStatus } from '@/models.ts'
import { computed } from 'vue'
import { POWER_ICON, STORE_ICON, SWITCH_ICON, WAITING_ROOM_ICON } from '@/icons.ts'
import StatPanel from '@/components/StatPanel.vue'
import StatGroup from '@/components/StatGroup.vue'

const props = defineProps<{
  status?: QueueStatus | null
}>()

const enabled_description = computed(() => {
  if (props.status != null) {
    return props.status.queue_enabled ? 'On' : 'Off'
  } else {
    return null
  }
})
</script>

<template>
  <StatGroup class="flex-auto m-5 max-w-3xl">
    <StatPanel
      v-if="props.status != null"
      title="Queue Enabled"
      :value="enabled_description"
      :class="{
        'text-success': props.status.queue_enabled,
        'text-error': !props.status.queue_enabled,
      }"
    >
      <template #figure>
        <div v-html="POWER_ICON" />
      </template>
    </StatPanel>
    <StatPanel
      v-if="props.status != null"
      title="Store Capacity"
      :value="props.status.store_capacity == -1 ? '∞' : props.status.store_capacity"
      class="text-info"
    >
      <template #figure>
        <div v-html="SWITCH_ICON" />
      </template>
    </StatPanel>
    <StatPanel v-if="props.status != null" title="Store Size" :value="props.status.store_size">
      <template #figure>
        <div v-html="STORE_ICON" />
      </template>
    </StatPanel>
    <StatPanel v-if="props.status != null" title="Queue Size" :value="props.status.queue_size">
      <template #figure>
        <div v-html="WAITING_ROOM_ICON" />
      </template>
    </StatPanel>
  </StatGroup>
</template>

<style scoped></style>
