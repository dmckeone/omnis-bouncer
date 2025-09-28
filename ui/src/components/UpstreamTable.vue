<script setup lang="ts">
import type { Upstream } from '@/models.ts'

const props = defineProps<{
  upstreams: Upstream[] | null
}>()
</script>

<template>
  <div class="overflow-x-auto rounded-box bg-base-100 flex-auto m-5 max-w-3xl drop-shadow-md">
    <table class="table table-zebra">
      <!-- head -->
      <thead class="text-mg">
        <tr class="bg-info">
          <th class="text-info-content">Upstream Host</th>
          <th class="text-info-content">Connections</th>
          <th class="text-info-content">Sticky Sessions</th>
        </tr>
      </thead>
      <tbody class="text-sm" v-if="props.upstreams != null">
        <tr v-for="upstream in props.upstreams" :key="upstream.uri">
          <th>{{ upstream.uri }}</th>
          <td>{{ upstream.connections }}</td>
          <td>{{ upstream.sticky_sessions }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped></style>
