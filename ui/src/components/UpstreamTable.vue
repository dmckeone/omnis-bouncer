<script setup lang="ts">
import type { Upstream } from '@/models.ts'

const props = defineProps<{
  upstreams: Upstream[] | null
}>()
</script>

<template>
  <div
    class="overflow-x-auto rounded-box border border-base-content/5 bg-base-100 flex-auto m-5 max-w-3xl"
  >
    <table class="table table-zebra">
      <!-- head -->
      <thead class="text-lg">
        <tr>
          <th>Upstream Host</th>
          <th>Connections</th>
          <th>Sticky Sessions</th>
        </tr>
      </thead>
      <tbody class="text-md" v-if="props.upstreams != null">
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
