<script setup lang="ts">
import {storeToRefs} from "pinia";
import {computed} from "vue";
import StatGroup from "@/components/StatGroup.vue";
import StatPanel from "@/components/StatPanel.vue";
import {POWER_ICON, STORE_ICON, SWITCH_ICON, WAITING_ROOM_ICON} from "@/icons.ts";
import {useQueueStatus} from "@/stores/queue.ts";
import {useUpstreams} from "@/stores/upstreams.ts";

const queueStore = useQueueStatus();
const {config, status} = storeToRefs(queueStore);

const upstreamsStore = useUpstreams();
const {upstreams} = storeToRefs(upstreamsStore);

const enabled_description = computed(() => {
  if (status.value != null) {
    return status.value.queue_enabled ? 'On' : 'Off';
  } else {
    return null;
  }
});
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
        <StatGroup v-if="status != null" class="flex-auto m-5 max-w-3xl">
          <StatPanel title="Queue Enabled"
                     :value="enabled_description"
                     :class="{
                     'text-success': status.queue_enabled,
                     'text-error': !status.queue_enabled
                   }"
          >
            <template #figure>
              <div v-html="POWER_ICON"/>
            </template>
          </StatPanel>
          <StatPanel
              title="Store Capacity"
              :value="status.store_capacity == -1 ? '∞' : status.store_capacity"
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
        <hr class="mx-5 border-accent">
        <div class="overflow-x-auto rounded-box border border-base-content/5 bg-base-100 flex-auto m-5 max-w-3xl">
          <table class="table table-zebra">
            <!-- head -->
            <thead class="text-lg">
            <tr>
              <th>Upstream Host</th>
              <th>Connections</th>
              <th>Sticky Sessions</th>
            </tr>
            </thead>
            <tbody class="text-md">
            <!-- row 1 -->
            <tr v-for="upstream in upstreams" :key="upstream.uri">
              <th>{{ upstream.uri }}</th>
              <td>{{ upstream.connections }}</td>
              <td>{{ upstream.sticky_sessions }}</td>
            </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
    <div class="w-5"></div>
  </div>
</template>

<style scoped></style>
