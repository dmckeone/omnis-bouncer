<script setup lang="ts">
import {storeToRefs} from "pinia";
import {useQueueStatus} from "@/stores/queue.ts";
import StatGroup from "@/components/StatGroup.vue";
import StatPanel from "@/components/StatPanel.vue";
import {POWER_ICON, STORE_ICON, SWITCH_ICON, WAITING_ROOM_ICON} from "@/icons.ts";
import {computed} from "vue";

const store = useQueueStatus();
const {info, status} = storeToRefs(store);
const enabled_description = computed(() => status.value.queue_enabled ? 'On' : 'Off');
</script>

<template>
  <div class="min-h-screen">
    <div class="navbar bg-base-100 shadow-sm">
      <div class="flex-1">
        <a class="btn btn-ghost text-xl">{{ info.name }}</a>
      </div>
    </div>
    <div class="flex fixed w-screen">
      <StatGroup class="flex-auto m-5 max-w-3xl">
        <StatPanel title="Queue Enabled"
                   :value="enabled_description"
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
      </StatGroup>
    </div>
    <div class="w-5"></div>
  </div>
</template>

<style scoped></style>
