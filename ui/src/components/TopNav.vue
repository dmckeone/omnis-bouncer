<script setup lang="ts">
const props = defineProps<{
  title?: string
}>()
</script>

<template>
  <div class="min-h-screen">
    <div class="navbar bg-base-100 shadow-sm">
      <div class="flex-1">
        <a v-if="props.title != undefined" class="btn btn-ghost text-xl">{{ props.title }}</a>
      </div>
    </div>
    <div class="flex fixed w-screen">
      <div class="flex flex-col">
        <slot></slot>
      </div>
    </div>
    <div class="w-5"></div>
  </div>
</template>

<style scoped></style>
