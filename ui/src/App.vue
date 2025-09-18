<script setup lang="ts">
import {storeToRefs} from "pinia";
import {useQueueStatus} from "@/stores/queue.ts";
import Stats from "@/components/Stats.vue";
import StatPanel from "@/components/StatPanel.vue";
import {POWER_ICON, STORE_ICON, SWITCH_ICON, WAITING_ROOM_ICON} from "@/icons.ts";

const store = useQueueStatus();
const {info, status} = storeToRefs(store);
</script>

<template>
  <div class="min-h-screen">
    <div class="p-10 max-w-md">
      <h1 class="text-5xl font-bold pb-10 text-primary">{{ info.name }}</h1>
      <Stats>
        <StatPanel title="Queue Enabled"
                   :value="status.queue_enabled"
                   :class="{ 'text-success': status.queue_enabled, 'text-error': !status.queue_enabled }"
        >
          <template #figure>
            <div v-html="POWER_ICON"/>
          </template>
        </StatPanel>
        <StatPanel
            title="Store Capacity"
            :value="status.store_capacity"
            class="text-info"
        >
          <template #figure>
            <div v-html="SWITCH_ICON"/>
          </template>
        </StatPanel>
        <StatPanel
            title="Store Size"
            :value="status.store_size"
        >
          <template #figure>
            <div v-html="STORE_ICON"/>
          </template>
        </StatPanel>
        <StatPanel
            title="Queue Size"
            :value="status.queue_size"
        >
          <template #figure>
            <div v-html="WAITING_ROOM_ICON"/>
          </template>
        </StatPanel>
      </Stats>
    </div>
  </div>
</template>

<style scoped></style>
