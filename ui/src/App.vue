<script setup lang="ts">
import './main.css'
import {useQueueStatus} from "@/stores/queue.ts";
import {storeToRefs} from "pinia";

const store = useQueueStatus();
const {status} = storeToRefs(store);
</script>

<template>
  <div class="hero bg-base-200 min-h-screen">
    <div class="hero-content text-center">
      <div class="max-w-md">
        <h1 class="text-5xl font-bold">Omnis Bouncer</h1>
        <div class="stats shadow">
          <div class="stat place-items-center">
            <div class="stat-title">Store Capacity</div>
            <div class="stat-value">{{ status.store_capacity }}</div>
          </div>

          <div class="stat place-items-center">
            <div class="stat-title">Store Size</div>
            <div class="stat-value">{{ status.store_size }}</div>
          </div>

          <div class="stat place-items-center">
            <div class="stat-title">Queue Size</div>
            <div class="stat-value">{{ status.queue_size }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped></style>
